# Plan: Post-1.1.1 — Coverage to 80 %, Hygiene, Spec Gaps

> **Status**: Drafted 2026-10-09
> **Parent**: v1.0 remediation tasks (closed with v1.1.1; P4 continues here) ·
> **Baseline**: v1.1.1 (`817344b`), CI fully green
> **Last updated**: 2026-10-10

## 1. Overview

v1.1.1 closed all 32 findings from `review-v1.0.md` (P0–P3). Three work
streams remain, in priority order:

1. **P4 — coverage backfill** (the only phase still tracked as incomplete in
   `tasks-remediation-v1.md`): overall line coverage measured **75.32 %**
   (5369/7128, CI run 37926162262). The spec target is 80 %, i.e. roughly
   **+340 net-new covered lines** at today's denominator.
2. **Hygiene** — the Optional-grade leftovers from the second-round review
   (SQLite authz posture, ListObjectsV2 cross-page, tmp-file reaper, stale
   spec wording, test-double fidelity).
3. **Spec gaps** — formally resolve the three "post-MVP" promises the spec
   still implies: auth rate limiting (OQ-6), per-team custom roles, and the
   SQLite repository layer. Each gets either an implementation or an explicit
   descope in the spec — no more silent "still unlimited" drift.

### Priority order changed 2026-10-10

P4 is **paused after 4.A + the first 4.B pass**, by decision: the coverage
tooling now works and the table is trustworthy, but the remaining P4 work is
test-writing against a target, while streams 2 and 3 contain behaviour that is
wrong or missing for users. Feature work goes first; P4 resumes afterwards with
the numbers in §4 already banked.

What P4 did buy, and what carries over:

- The coverage job has a real per-crate table and a migrated PostgreSQL, so
  `*_pg` code is measurable at all (4.A, merged).
- `crates/admin` 51.97 % → 75.98 % (4.B, partial, merged).
- The rest of the backfill list in `tasks-post-1.1.1.md` stays accurate as
  written; it is simply not next.

### Next up: H3, and why it outranks the rest of Hygiene

`S3Driver::list` issues a **single** `list-type=2` request with no
`continuation-token` and no `max-keys`, and never parses `IsTruncated`. Against
a real S3 or MinIO backend the API caps a response at 1000 keys, so everything
beyond that is **silently dropped** — from the admin UI, from the worker, and
from `s3compat`'s `ListObjectsV2`, which paginates on top of it. `LocalDriver`
returns everything in one page and is unaffected, which is why this has gone
unnoticed.

Guiding constraints (inherited from `plan-remediation-v1.md`):

- Every task ships with the test that fails without it.
- CI stays green after every task; the coverage gate (65 %) only moves when
  the floors have held on two consecutive master runs.
- Docs changes land in the same commit as the behavior they describe.

## 2. Architecture Decisions

- **D-13 — Coverage visibility first.** Tarpaulin only prints the overall
  number; per-crate breakdown must come from `cobertura.xml` before tasks can
  be targeted. A CI step parses and prints the per-crate table (and uploads it
  as an artifact) — no more guessing which crate is weakest.
- **D-14 — Per-crate floors, not just a global number.** When flipping the
  gate to 80 %, enforce a per-crate floor (old baseline worst: admin 48 %,
  audit 53 %, imaging 56 %, api 54 %) so a strong crate cannot mask another.
- **D-15 — SQLite stays dev-only, documented.** The SQLite path has no
  repository layer (login/user/team/image repos are `None`), so "SQLite
  deployments" cannot enforce auth regardless. Wiring a full repo layer is an
  XL effort with no production demand; instead the dev-only posture gets an
  explicit warning in `deployment.md`/`README.md` (see H1). Revisit only if a
  single-host SQLite production use case appears.
- **D-16 — Rate limiting: in-process token bucket, login + OIDC start.**
  OQ-6 asked for it; the simplest honest implementation is a per-IP (and
  per-email) fixed-window counter in `AppState` behind a new
  `[rate_limit]` config block, returning `429` per the existing OpenAPI
  entry. No new dependencies.
- **D-17 — Per-team custom roles: descope in the spec.** The evaluation
  engine, `team_members.role` CHECK, and every route assume the four built-in
  roles. Implementing arbitrary role sets means a role registry, migration,
  and cache invalidation — an L-sized feature with zero user demand to date.
  Spec §10.1 gets a "custom roles are post-v1; the built-in four are the only
  roles" statement instead of an implication they exist.

## 3. Task List

### Phase P4 — Coverage 75.32 % → 80 %

- **Task 4.A: Per-crate coverage visibility (D-13)** *(XS, CI-only)*
  - Add a step after tarpaulin that parses
    `target/coverage/cobertura.xml` and prints a per-crate
    `lines-covered/total, %` table into the job log; upload the XML as a
    workflow artifact.
  - Acceptance: the coverage job log shows one line per workspace crate.
  - Verify: CI green; numbers recorded in this file's table below.
  - Files: `.github/workflows/ci.yml`

- **Task 4.B: `crates/admin` backfill** *(M)* — CLI error paths, exit codes,
  `config validate` semantic branches, `migrate` status classification.
  Verify: `cargo tarpaulin -p picroom-admin` (or CI table) ≥ 80 %.

- **Task 4.C: `crates/audit` backfill** *(M)* — DB sink error paths, reader
  filters (`limit`/`before` edges), redaction. Verify: ≥ 80 %.

- **Task 4.D: `crates/imaging` backfill** *(M)* — resize edge cases
  (1-px inputs, extreme aspect ratios), probe rejection paths, encoder
  quality clamps. Verify: ≥ 80 %.

- **Task 4.E: `crates/api` handler branches** *(M)* — 403/404 mapping on
  every route, multipart error paths, admin validation failures.

- **Task 4.F: remaining crates to the floor** *(S)* — s3compat is ~91 %
  already; confirm worker/service/storage/domain clear 80 % and add
  targeted tests only for the stragglers the 4.A table reveals.

- **Task 4.G: flip the gate (D-14)** *(XS, last)* — `--fail-under 80` plus
  per-crate floors; only after 4.A–4.F hold green on two consecutive runs.
  Update `spec.md` §1.4 S5 note and the CI job header comment.

### Checkpoint P4

- [x] CI coverage job log prints per-crate table (run 38056694504)
- [ ] Every crate ≥ 80 % lines; overall ≥ 80 % — outstanding: audit 41.98 %,
      service 56.21 %, infra 71.15 %, admin 75.98 %, storage 77.25 %
- [ ] Gate at 80 % green on two consecutive master runs
- [ ] `spec.md` S5 matches reality; `tasks-remediation-v1.md` P4 closed

### Phase H — Hygiene (re-review Optional leftovers)

- **Task H1: document the SQLite posture (D-15)** *(XS)* — warning in
  `deployment.md` + `README.md`: SQLite mode has no auth enforcement or
  repositories; production requires PostgreSQL.
- **Task H2: `require_sessions` cutover runbook** *(XS)* — add to
  `docs/operations.md`: after one JWT TTL post-deploy, set
  `[auth].require_sessions = true`; document the one-time mass-401 risk.
- **Task H3: ListObjectsV2 pagination across driver pages** *(M)* — loop
  `storage().list` via `next_cursor` until `max_keys` or exhausted; bucket
  the `IsTruncated` flag off the loop, not a single page. Test with a
  fake listing > max_keys.
- **Task H4: ListObjectsV2 `delimiter`** *(S)* — implement
  `CommonPrefixes` grouping (or reject the parameter with
  `NotImplemented` explicitly) instead of silently ignoring it.
- **Task H5: LocalDriver temp-file sweep** *(S)* — on driver construction,
  best-effort delete dot-prefixed `*.tmp` older than 24 h; log deletions.
- **Task H6: spec §10.1 wording** *(XS)* — state built-in roles are the
  only roles; remove the `team.invite` implication (D-17).
- **Task H7: expired-session fidelity** *(XS)* — `MemSessions` test double
  honors `expires_at`; add the expired-but-unrevoked → 401 test.

### Checkpoint H

- [ ] All H tasks merged; docs updated in the same commits
- [ ] `cargo test --workspace` green; CI green

### Phase G — Spec gaps: implement or descope

- **Task G1: auth rate limiting (OQ-6, D-16)** *(M)* — fixed-window
  counters keyed by client IP + email on `POST /api/v1/auth/login` and OIDC
  start; `[rate_limit]` config block (`login_max_attempts`,
  `login_window_secs`); `429` with `Retry-After`; tests: N+1-th attempt
  → 429, window expiry resets, per-IP isolation.
- **Task G2: per-team custom roles — descope (D-17)** *(XS, docs)* —
  spec §10.1 statement per D-17; note in ADR-0005.
- **Task G3: SQLite repository layer — descope (D-15)** *(XS, docs)* —
  covered by H1's documentation; add the explicit statement to
  `spec.md` (dev-only, no repositories).

### Checkpoint G

- [ ] Rate limiter live with tests; OpenAPI `429` documented *and* true
- [ ] Custom roles and SQLite layer explicitly descoped in spec/ADR-0005
- [ ] No doc sentence in the repo claims an unimplemented feature

## 4. Current coverage snapshot

**CI (tarpaulin, 2026-10-10, run 38056694504)** — authoritative. Measured with
the coverage job's migrated PostgreSQL, so the `*_pg` paths are included:

| Crate | Lines | % | To 80 % |
|---|---|---|---|
| audit | 34/81 | 41.98 | **+31** |
| service | 742/1320 | 56.21 | **+314** |
| infra | 74/104 | 71.15 | +10 |
| admin | 367/483 | 75.98 | +20 |
| storage | 516/668 | 77.25 | +19 |
| s3compat | 300/366 | 81.97 | — |
| domain | 96/115 | 83.48 | — |
| worker | 501/592 | 84.63 | — |
| api | 2458/2873 | 85.56 | — |
| auth | 394/460 | 85.65 | — |
| imaging | 100/105 | 95.24 | — |
| *(other)* | 0/85 | 0.00 | — |
| **total** | **5582/7252** | **76.97 %** | +220 |

`*(other)*` is the Tauri desktop client — `desktop/src-tauri/src/commands/upload.rs`
(52 lines) and `desktop/src-tauri/src/store.rs` (33 lines), both untested. It is
not a workspace crate, so it never affects a per-crate floor; whether it should
join `bin/**` in `--exclude-files` is an open call, not a measurement problem.

Overall moved 75.32 % → 76.97 % against a slightly smaller denominator (7128 →
7252): giving the job a database added covered lines while dropping `bin/**`
rows that the earlier run had counted.

**Local proxy (cargo-llvm-cov)** is no longer recorded here. It disagreed with
tarpaulin badly enough to misdirect the work — audit read 76.81 % against
tarpaulin's 41.98 %, api read 71.42 % against 85.56 %. The two tools count
coverable lines very differently (audit 138 vs 81, api 1781 vs 2873), so a local
proxy is not a usable substitute for the CI table; read the table the coverage
job prints.

Tarpaulin cannot produce a report on a developer Windows box (tests run fine,
but profraw merging fails with `parser failure: Nom(Satisfy)` because its
engines target Unix), which is why per-crate data now comes from the CI log.

### Open question: the coverage job had no PostgreSQL — resolved

`coverage` had no `services:` block and no `DATABASE_URL`, so every `pg_gated`
test skipped and the whole `*_pg` surface was unreachable. The job now gets the
same `postgres:16` service and a migrated schema as the `test` job, so those
paths execute and their coverage is real.

That change immediately exposed a latent defect: `pg_gated::ensure_schema`
creates its tables with `CREATE TABLE IF NOT EXISTS`, so once migrations have
run those statements are no-ops and the suite silently starts testing the
production schema instead of its own fixtures — a different `storage_policy`
default, plus a foreign key on `owner_id` its fake UUID does not satisfy. The
fixtures now live in their own `pg_gated_test` schema (see commit `998b51d`).

## 5. Risks and Mitigations

| Risk | Impact | Mitigation |
|---|---|---|
| Tarpaulin flakiness masks per-crate regressions | Med | Gate on two consecutive green runs before flipping; keep the 65 % floor until then |
| Coverage tasks turn into coverage-gamed tests | Med | Tests target error/edge paths named in tasks, not trivial getters |
| New clippy (1.99 doc_markdown) bites doc comments | Low (noise) | Local toolchain already at 1.99 — run clippy before push |
| pg_gated-style concurrent-schema races in new DB tests | Med | Reuse the advisory-lock pattern from `pg_gated::ensure_schema` |
| Rate limiter adds latency to login | Low | Fixed-window counters are two hashmap ops; no locks across await |

## 6. Parallelization

| Lane | Tasks | Constraint |
|---|---|---|
| A | 4.A → 4.B–4.F → 4.G | Sequential (each informs the next) |
| B | H3, H4, H5 | Independent of each other |
| C | H1, H2, H6, H7, G2, G3 | Docs/XS; any time |
| D | G1 | Independent; needs `[rate_limit]` config first |

## 7. Open Questions

- Is 80 % measured per-crate floors or overall only? (Plan assumes both —
  D-14; overriding this changes Task 4.G only.)
- Should the rate limiter also cover `POST /api/v1/admin/users` (password
  setter)? Default: no, login/OIDC only.
