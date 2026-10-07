# Plan: Picroom v1.0 Remediation

> **Status**: Approved 2026-10-07 — scope decided (full per-resource ACL, session-based revocation, coverage raised to 80 %)
> **Parent**: [`review-v1.0.md`](review-v1.0.md) · **Tasks**: [`tasks-remediation-v1.md`](tasks-remediation-v1.md)
> **Last updated**: 2026-10-07

## 1. Overview

`review-v1.0.md` found 33 issues: 4 Critical, 10 High, 14 Medium, 5 Low. The
skeleton is healthy; the gaps are in business semantics. This plan sequences the
work into five phases so the tree stays shippable and CI-green after every phase.

Guiding constraints:

- **No phase may leave the API contract, the OpenAPI document, or the migrations
  in an inconsistent state.** A contract change lands its doc change in the same
  task.
- **Every task ships with the test that proves it.** Coverage is not a phase you
  do at the end — it is a property of every task. Phase P4 only closes the
  pre-existing backlog.
- **Postgres is the source of truth.** SQLite is the dev path; when a table or
  behaviour is added to Postgres it must be mirrored (or explicitly deferred with
  a warning) in the same phase.

### Effort shape

| Phase | Theme | Findings | Shape |
|---|---|---|---|
| **P0** | Stop the bleeding | R-01…R-04, R-14 | 5 tasks, ~1 day |
| **P1** | Authorization & secrets | R-05, R-08, R-13, R-19, R-20, R-32 + **full ACL** | 10 tasks, ~6–8 days |
| **P2** | Reliability & configuration | R-10…R-12, R-17, R-18, R-21, R-25 | 7 tasks, ~3–4 days |
| **P3** | Hygiene & parity | R-07, R-15, R-16, R-22…R-24, R-26…R-32 | 10 tasks, ~3 days |
| **P4** | Coverage backfill to 80 % | S5 | 8 tasks, ~4–6 days |

P0 is the release blocker. P1 carries the largest new surface because per-resource
ACL is now explicitly in scope for v1.

### The good news about ACL

`migrations/0002_storage_and_images.sql:54-65` already defines `resource_acls`:

```sql
CREATE TABLE IF NOT EXISTS resource_acls (
    id            UUID PRIMARY KEY,
    resource_type VARCHAR(64) NOT NULL,
    resource_id   UUID NOT NULL,
    subject_type  VARCHAR(32) NOT NULL CHECK (subject_type IN ('user', 'team')),
    subject_id    UUID NOT NULL,
    permission    VARCHAR(32) NOT NULL
                  CHECK (permission IN ('read','create','update','delete','admin')),
    granted_at    TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    UNIQUE (resource_type, resource_id, subject_type, subject_id, permission)
);
```

It has **zero code consumers**. So per-resource ACL needs a repository, an
evaluation engine, and endpoints — not a new table. The only schema change is an
`effect` column for explicit deny (D-9) and a SQLite mirror (D-11).

## 2. Architecture decisions

Non-obvious calls. Each is a decision, not an accident.

### D-1 — Body limits: `DefaultBodyLimit`, not `RequestBodyLimitLayer` *(decided)*

The 2 MiB cap comes from axum's *extractor-level* default, which
`RequestBodyLimitLayer` does not touch — independent mechanisms (R-06). Install
`DefaultBodyLimit::max(max_body_mb * 1MB)` on the router in `api_cmd.rs`, and
**keep** `RequestBodyLimitLayer` as the outer transport backstop for any future
streaming route. S3 and REST share one knob (default applied — see §7).

### D-2 — Multipart: block the fallthrough in-handler, do not add routes *(decided)*

Registering extra routes for `PUT`/`DELETE` on the same path re-introduces axum's
method-merge semantics we are escaping. Instead `put_object` and `delete_object`
check their query string **first**: if `uploadId` or `partNumber` is present,
return the documented `501 NotImplemented` XML and touch no state. One guard,
impossible to route around, keeps ADR-0004's "clients fall back to a single PUT"
contract intact.

### D-3 — Job enqueue moves out of `upload()` *(decided)*

`upload()` today both stores bytes and enqueues jobs, so the caller cannot order
enqueue after the DB insert. Split into `stage()` (validate → probe → persist →
insert row) and `enqueue_variants()` (called only after `stage()` returns). Makes
the correct order the only order without threading a commit callback.

### D-4 — Thumbnail identity: `kind='thumbnail'` + `size=<n>` *(decided)*

`image_variants` already has a `size` column and `UNIQUE (image_id, kind, size)`.
Storing `kind='thumbnail', size=200` satisfies the CHECK constraint *and* makes
the upsert idempotent.

### D-5 — Variant uniqueness: `COALESCE` in a partial unique index *(decided)*

Postgres treats NULLs as distinct, so `ON CONFLICT (image_id, kind, size)` never
fires for AVIF/WebP (R-18). Add
`CREATE UNIQUE INDEX ... ON image_variants (image_id, kind, COALESCE(size, -1))`
and change the upsert to the same expression. Keeps the nullable column and all
existing rows; no backfill.

### D-6 — Revocation via the existing `sessions` table *(decided by NEO 2026-10-07)*

`migrations/0004_sessions_and_oidc.sql` already defines `sessions` with
`revoked_at`, and it has no consumer. Rather than bolting on a JWT denylist:
`login` writes a session row, the JWT carries its `sid`, and `require_auth`
rejects tokens whose `sid` is unknown or revoked. Revocation becomes one indexed
lookup, and `set_user_disabled` can revoke every live session for a user in a
single statement — which also closes R-20.

*No migration needed — the table exists.*

### D-7 — One permission model, enforced in the service layer *(decided)*

Merge the duplicated `Permission` / `ResourceType` / `PermissionAction` types
(R-32) into `picroom-domain`, keeping the canonical `ResourceType` list already
there (`Image, Team, User, Audit, StoragePolicy, System`). Then give
service-layer entry points an `Actor { user_id, roles }` parameter and enforce
there; route handlers keep authentication only.

Rationale: route-only enforcement is exactly why S3 and worker paths are
unauthorized. Moving the check down makes the hole structurally impossible to
reopen. Land the type merge as its own task first so this stays reviewable.

### D-8 — Worker: `catch_unwind` per job + lease columns *(decided)*

Wrap `handler(job)` in `AssertUnwindSafe(..).catch_unwind()` so one bad job
resets that slot instead of killing it, and record the panic as a job failure so
it flows through the existing retry/DLQ path. Separately add `lease_expires_at`
and `claimed_by` to `jobs`, make `dequeue` also claim lease-expired `running`
rows, and have `complete`/`fail` clear the lease. At-least-once delivery without
a separate reaper process; both concerns stay in the one queue implementation.

### D-9 — Explicit deny is a column on `resource_acls`, not a new table *(decided)*

`spec.md` §10.3 makes explicit deny the highest-priority rule, but the existing
`permission` CHECK enum has no `deny` member. Additively extend:

```sql
ALTER TABLE resource_acls
  ADD COLUMN effect VARCHAR(6) NOT NULL DEFAULT 'allow'
  CHECK (effect IN ('allow', 'deny'));
```

and widen the UNIQUE constraint to include `effect`. Evaluation then becomes:
**any matching `deny` row wins; otherwise team role; otherwise any `allow` ACL
row; otherwise deny.**

Rationale: one table, deny and allow are the same shape, a deny is expressible
without inventing a parallel convention, and the default keeps every existing row
meaningful.

### D-10 — ACL management is replace-semantics on one endpoint per resource *(decided)*

New surface (all RBAC-guarded by ownership or `manager`):

| Method | Path | Semantics |
|---|---|---|
| `GET` | `/api/v1/images/:id/acl` | list current grants |
| `PUT` | `/api/v1/images/:id/acl` | **replace** the full grant set (idempotent) |
| `DELETE` | `/api/v1/images/:id/acl/:subject_type/:subject_id` | remove one grant |

Team membership stays where it is (`/api/v1/teams/:id/members`) and is evaluated
as rule 2, not as an ACL row. Rationale: `PUT` with a full set is idempotent,
avoids PATCH-vs-PUT ambiguity, and keeps the grant list a single readable
document. ACLs on `Team` / `StoragePolicy` resources are modelled in the engine
but get no endpoints until something needs them — the table and evaluator are
resource-agnostic, so this is additive later.

### D-11 — Mirror `resource_acls` into SQLite via a new migration *(decided)*

`migrations/0005_sqlite_init.sql` has no `resource_acls` table. Rather than editing
an already-shipped init migration, add a forward-only
`0009_sqlite_resource_acls.sql` with the same DDL. Rationale: SQLite is the dev
path; leaving ACLs Postgres-only would mean developers never exercise the code
they are shipping.

### D-12 — Coverage 80 % is enforced per crate, with tests written alongside *(decided by NEO 2026-10-07)*

`spec.md` §1.4 S5 says ≥80 %; CI currently enforces 60 % and the worst crates sit
at 48–56 %. Two changes:

1. **Per-task discipline** — every P0–P3 task must include a test that fails
   without its fix. This is where most of the coverage gain comes from, for free.
2. **CI raises to `--fail-under 80`**, plus a per-crate floor so one strong crate
   cannot mask another. Current gaps to close in P4: `admin` 48 %, `audit` 53 %,
   `api` 54 %, `imaging` 56 %, `service` and `storage` in between.

## 3. Phase P0 — Stop the bleeding

Goal: no data loss, no false job success, CI green. **Ship blocker.**

1. **R-01** multipart fallthrough guard (D-2)
2. **R-02** thumbnail `kind`/`size` + stop swallowing the insert error (D-4)
3. **R-04** enqueue after insert (D-3)
4. **R-06** `DefaultBodyLimit` (D-1) — restores S11
5. **R-14** CI lint green: delete the `quota.rs` stubs rather than silencing the lint

### Checkpoint P0

- [ ] `cargo clippy --all-targets --all-features --locked -- -D warnings` → exit 0
- [ ] `cargo test --workspace` green, with a regression test per finding
- [ ] Manual: `aws s3 cp` a 5 MB JPEG → 200; 150 MB → 413
- [ ] Manual: `DELETE /s3/b/k?uploadId=x` → 501, object still readable
- [ ] Manual: after upload, an `image_variants` row exists for the thumbnail

## 4. Phase P1 — Authorization & secrets

The largest phase. Tasks 1.1 → 1.3 are strictly sequential; 1.4–1.5 can run
alongside once 1.2 lands.

- **1.1** merge the permission types (D-7) — foundation, no behaviour change
- **1.2** `resource_acls` repository + `effect` migration + SQLite mirror (D-9, D-11)
- **1.3** evaluation engine implementing §10.3 in order: deny → team role → ACL → default deny
- **1.4** move enforcement into the service layer (D-7)
- **1.5** close the route-level gaps: upload RBAC, team scoping, `team_id` validation
- **1.6** ACL endpoints + OpenAPI (D-10)
- **1.7** sessions: revocation, disable-user cascade (D-6)
- **1.8** orphan cleanup on insert failure; stop leaking decoder internals
- **1.9** amend `spec.md` §10 / ADR-0005 to describe what now actually ships
- **1.10** correct `tasks-admin-client.md` Task 1.6, which claims ACL ships today

### Checkpoint P1

- [ ] Evaluation-order unit tests pass for all four rules, including deny-beats-admin
- [ ] `viewer` cannot upload; user A cannot read user B's team or image
- [ ] An explicit deny overrides an admin role (the case `Role::Admin` currently
      short-circuits at `rbac.rs:177`)
- [ ] After logout the token returns 401; disabling a user kills live tokens
- [ ] The service layer rejects an unauthorised `Actor` with no HTTP layer involved
- [ ] SQLite and Postgres behave identically for the ACL paths

## 5. Phase P2 — Reliability & configuration

- **R-10** thread `[pipeline]` into `ProcessorDeps`; apply quality, `max_dimension`, `strip_exif`
- **R-11** worker panic guard (D-8)
- **R-12** job lease (D-8)
- **R-18** variant unique index (D-5)
- **R-17** emit `AuditAction::Login`/`Logout`; guard `audit_events` against UPDATE/DELETE
- **R-21** quota: team dimension, SQLite path, remove the `charge_user` stub
- **R-25** paginate team queries

### Checkpoint P2

- [ ] `quality.avif = 25` demonstrably changes output bytes
- [ ] Killing a worker mid-job leads to a successful retry
- [ ] A panicking handler leaves the worker alive and the job in the DLQ
- [ ] Every auth event appears in `GET /api/v1/audit`

## 6. Phase P3 — Hygiene & parity

- **R-07** SigV4 body integrity + mandatory `SignedHeaders` + URI encoding
- **R-15 / R-16** bucket scoping and the host-with-port signing fix
- **R-22** desktop: OS keychain for the JWT, refuse non-HTTPS outside dev
- **R-23** constant-time token compare; wire API tokens into a handler or delete the service
- **R-24** reconcile OpenAPI with the implementation; add a route-drift check to CI
- **R-26** drop GIF from the accepted MIME list or enable the decoder
- **R-27** honour `--policy` and `--config` in the CLI
- **R-28** wire `crates/imaging` into the worker, or delete the placeholders
- **R-29–R-31** resize guard, local-driver temp files, retry off-by-one

### Checkpoint P3

- [ ] `aws s3 cp` / `aws s3 ls` / PicGo all work against MinIO (S11, S12)
- [ ] `cargo deny check`, `cargo audit`, `reuse lint` clean (S8, S9, S15)
- [ ] No unreferenced service, skeleton, or placeholder remains

## 7. Phase P4 — Coverage backfill

Only the pre-existing backlog; per-task tests are already handled in P0–P3.

Targets, from `cov_baseline.log`: `admin` 48 %, `audit` 53 %, `api` 54 %,
`imaging` 56 %. Work bottom-up on the error-mapping and CLI surfaces, which are
where the untested lines cluster.

- [ ] Raise `.github/workflows/ci.yml` to `--fail-under 80` with a per-crate floor
- [ ] `crates/audit`: DB sink, reader filters, redaction
- [ ] `crates/admin`: every CLI subcommand's error paths and exit codes
- [ ] `crates/api`: handler error mapping, pagination edges, RBAC denial branches
- [ ] `crates/imaging`: encoder edge cases, EXIF handling, extreme aspect ratios
- [ ] DB-backed paths covered by the existing testcontainers harness, not skipped

## 8. Decisions taken

| # | Decision | By |
|---|---|---|
| Q-1 | Multipart S3 stays a `501` stub (per `spec.md` §8.2 and ADR-0004); only the fallthrough is closed | default applied |
| Q-2 | S3 and REST share `server.max_body_mb` | default applied |
| Q-3 | **Revocation uses the `sessions` table**, not a JWT denylist | **NEO, 2026-10-07** |
| Q-4 | **v1 ships real per-resource ACL**, per `spec.md` §10 in full — not a documented reduction | **NEO, 2026-10-07** |
| Q-5 | **Coverage is raised to 80 %**, not the spec amended down | **NEO, 2026-10-07** |
| Q-6 | SQLite gets quota enforcement (port the query), since it is the dev path | default applied |
| Q-7 | `crates/imaging` is **wired into the worker**; the worker's duplicate encoders are deleted | default applied |

The four defaults were derivable from the spec and ADRs, so they were applied
rather than escalated. Any of them can be overridden without disturbing the
phases — they change task content, not task order.

## 9. Risks and mitigations

| Risk | Impact | Mitigation |
|---|---|---|
| D-6 changes the token format, breaking existing clients | High | Ship the session `sid` as a new claim; accept `sid`-less tokens for one TTL window, then drop that path behind a config flag |
| Deny rules make `Role::Admin` no longer absolute | Medium | Intentional — that short-circuit at `rbac.rs:177` is the bug. Document it in ADR-0005 so operators understand a deny now beats admin |
| ACL evaluation adds a query to every authorized request | Medium | Load grants once per request into the `Actor` context; the `UNIQUE` constraint already gives an index for the lookup |
| D-7 (service-layer authz) touches many call sites at once | High | Type merge first as its own task; make the signature change compile-fail-driven, one entry point at a time |
| The variant unique index fails on existing duplicate rows | Medium | Dedupe first: `DELETE ... WHERE id NOT IN (SELECT MIN(id) ... GROUP BY image_id, kind, COALESCE(size,-1))` |
| Raising the body limit re-opens memory exhaustion | Medium | Keep `RequestBodyLimitLayer`; make `/i/*key` stream rather than buffer |
| Silencing `unused async` would cement dead code | Medium | Hard rule: no `#[allow]` on those two sites — delete the stub |
| 80 % coverage becomes a gate that blocks unrelated work | Medium | Land the per-task tests through P0–P3 first; flip the CI number in P4 when the headroom actually exists |
| New ACL endpoints expand the public contract | Low | D-10 defines the surface up front; OpenAPI ships in the same task |

## 10. Definition of Done

A phase is done when all of these hold:

- [ ] `cargo fmt --all -- --check` clean
- [ ] `cargo clippy --all-targets --all-features --locked -- -D warnings` clean
- [ ] `cargo test --workspace` green, including a regression test per finding fixed
- [ ] OpenAPI updated in the same commit as any contract change
- [ ] `spec.md` amended in the same commit as any documented behaviour change
- [ ] Migrations are additive and forward-only; no `revert` path is relied upon
- [ ] No new `todo!()` / `unimplemented!()` introduced
- [ ] Postgres and SQLite paths agree, or the divergence is documented and warned about
- [ ] Findings in [`review-v1.0.md`](review-v1.0.md) carry the fixing commit

## 11. Parallelization

| Lane | Tasks | Constraint |
|---|---|---|
| A | P0 (all) | Sequential — 0.2 and 0.3 share the upload path |
| B | 1.1 → 1.2 → 1.3 → 1.4 → 1.5 → 1.6 → 1.9 | Strictly sequential; the ACL spine |
| C | 1.7 (sessions), 1.8 (orphans) | Start after 1.2; independent of 1.3–1.6 |
| D | 2.2, 2.3 | Each needs one migration; not concurrently |
| E | 3.1, 3.2, 3.5, 3.6 | Independent once P0 lands |
| F | 3.3 (desktop) | Fully independent Rust↔Tauri boundary |
| G | P4 audit / admin coverage | Independent; can start immediately |

**Migration serialization** is the only hard constraint. New files, in order:
`0009_sqlite_resource_acls.sql`, `0010_jobs_lease.sql`,
`0011_image_variants_unique.sql`, `0012_audit_append_only.sql`. Sessions (D-6)
needs none.