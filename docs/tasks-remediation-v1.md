# Tasks: Picroom v1.0 Remediation

> **Status**: Executed 2026-10-07 — P0–P3 complete (all 33 findings addressed); P4 partially executed (coverage gate raised 60 → 65, 80 % target pending a measured Linux run)
> **Parent**: [`plan-remediation-v1.md`](plan-remediation-v1.md) · **Review**: [`review-v1.0.md`](review-v1.0.md)
> **Last updated**: 2026-10-07

Each task is sized S/M — implementable and verifiable in one focused session.
Every task names the findings (R-xx) and decisions (D-x) it closes.

**Rule (D-12): every task ships with the test that fails without its fix.** A task
whose **Verify** line has no new test is not done.

Legend: ⬜ not started · 🔄 in progress · ✅ done · 🔲 deliberately deferred

---

## Phase P0 — Stop the bleeding

Release blocker. Lane A, sequential.

### ✅ Task 0.1: Close the S3 multipart fallthrough (R-01, D-2)
- Guard `put_object` / `delete_object` (`crates/s3compat/src/object.rs:64,100`):
  if the query contains `uploadId` or `partNumber`, return `501 NotImplemented`
  **before touching storage**
- Use the escaping XML helper in `error.rs` — not the private `object.rs:127`
  `s3_xml_error` shadow
- Wire or delete `upload_part` / `complete_multipart` / `abort_multipart`; no
  unrouted dead code may remain
- **Tests**: `DELETE /s3/b/k?uploadId=x` → 501 *and the object is still readable*;
  `PUT …?partNumber=1&uploadId=U` → 501 with no write
- **Verify**: `cargo test -p picroom-s3compat`

### ✅ Task 0.2: Persist thumbnail rows and stop hiding the error (R-02, D-4)
- `crates/worker/src/processor.rs:70` → `kind = "thumbnail"`, `size = Some(size)`
- `processor.rs:142` must **not** swallow an insert failure — return it so the job
  retries or dead-letters
- **Tests**: `GenerateThumbnail{size:200}` against Postgres yields exactly one
  `image_variants` row with `kind='thumbnail', size=200`; a forced insert error
  lands the job in the DLQ
- **Verify**: `cargo test -p picroom-worker` (testcontainers harness)

### ✅ Task 0.3: Enqueue variant jobs only after the row is committed (R-04, D-3)
- Split `UploadService::upload` into `stage()` (validate → probe → store → insert)
  and `enqueue_variants()`
- `crates/api/src/handlers/images.rs:70,91` calls them in that order
- **Tests**: a worker racing the enqueue never sees a job without its `images` row
- **Verify**: `cargo test -p picroom-api -p picroom-service`

### ✅ Task 0.4: Install a real body limit (R-06, D-1)
- Add `DefaultBodyLimit::max(max_body_mb * 1MB)` to the router in
  `bin/picroom/src/api_cmd.rs`; keep `RequestBodyLimitLayer`
- Assert `max_body_mb > 0` at config load
- **Tests**: 5 MB body accepted, 150 MB rejected when `max_body_mb = 100`
- **Verify**: `cargo test -p picroom-api` + manual `aws s3 cp` of a 5 MB JPEG
  (criterion **S11**)

### ✅ Task 0.5: Make the CI lint gate green (R-14)
- Delete `QuotaService::remaining_team` and `charge_user` (`quota.rs:91,99`) —
  **no `#[allow]`**; 2.6 reimplements what is genuinely needed
- `crates/storage/src/driver/s3.rs:84,662` — drop `async` (or await something real)
- Delete the `crates/storage/src/signing.rs` skeleton and the test that asserts
  it errors
- **Verify**: `cargo clippy --all-targets --all-features --locked -- -D warnings` → exit 0

> **Execution note (2026-10-07)**: R-03 was closed with task 3.1 (per the
> plan's phase assignment), not in P0. Task 1.5's `tasks-admin-client.md`
> correction landed in task 1.9. Task 3.5 **deleted** `ApiTokenService`
> (no handler referenced it) rather than wiring it. Task 2.6 added a
> `team_quotas` table (migration 0014) instead of altering the `quotas`
> primary key. Task 3.3 stores the JWT in the OS keychain via `keyring`;
> `capabilities/default.json` now allows only `https://*` plus
> `http://localhost:*` / `http://127.0.0.1:*` for dev.

### Checkpoint P0
- [ ] Clippy gate exits 0
- [ ] `cargo test --workspace` green, with regression tests for R-01…R-04
- [ ] Manual: multipart-shaped DELETE leaves the object intact
- [ ] Manual: a thumbnail row exists after upload
- [ ] Manual: 5 MB upload succeeds (S11 restored)

---

## Phase P1 — Authorization & secrets

The ACL spine, tasks 1.1 → 1.6, is strictly sequential. The good news: the
`resource_acls` table already exists (`migrations/0002_storage_and_images.sql:54`),
so no table needs to be created — only `effect` (D-9) and a SQLite mirror (D-11).

### ✅ Task 1.1: Merge the duplicated permission types (R-32, D-7)
- Move `Permission` / `ResourceType` / `PermissionAction` into `picroom-domain`,
  keeping the canonical `ResourceType` list already there
  (`Image, Team, User, Audit, StoragePolicy, System`)
- Re-export from `picroom-auth` for one deprecation cycle, then delete the
  parallel definitions in `crates/domain/src/permission.rs`
- **Tests**: serde round-trip for every variant, in both directions
- **Verify**: `cargo test -p picroom-domain -p picroom-auth`
- **Note**: no behaviour change — this exists so 1.3 has one model to extend

### ✅ Task 1.2: `resource_acls` repository + migrations (D-9, D-11)
- `0009_resource_acl_effect.sql` — add
  `effect VARCHAR(6) NOT NULL DEFAULT 'allow' CHECK (effect IN ('allow','deny'))`
  and widen the UNIQUE constraint to include `effect`
- `0010_sqlite_resource_acls.sql` — the same DDL for the SQLite dev path
- `ResourceAclRepository` trait + Postgres and SQLite implementations:
  `list_grants(resource)`, `replace_grants(resource, grants)`, `revoke(subject)`
- **Tests**: grants persist and round-trip on **both** backends; `replace` is
  idempotent; a revoke removes exactly one row
- **Verify**: `cargo test -p picroom-service` (runs against both drivers)

### ✅ Task 1.3: Implement the §10.3 evaluation engine (R-09, D-9)
- `RbacEngine::check` takes `Actor` + `Resource` + loaded grants and evaluates in
  the documented order: **explicit deny → team membership role → resource ACL →
  default deny**
- Remove the `roles.contains(&Role::Admin) ⇒ Allow` short-circuit at
  `rbac.rs:177` — an explicit deny must now beat admin
- Wire the `Resource` struct (currently dead code) as the engine's input
- **Tests** — one per rule, including the cases that fail today:
  - a `deny` row beats an `admin` role
  - a `deny` row beats a `manager` team role
  - an `allow` grant grants access with no role
  - no grant, no role → `Deny`
  - `admin` with no deny → still allowed
  - an owner (`owner_id` match) is allowed for their own resource
- **Verify**: `cargo test -p picroom-auth -- rbac`

### ✅ Task 1.4: Move enforcement into the service layer (R-09, D-7)
- Give `UploadService` / `DeleteService` entry points an `Actor { user_id, roles }`
  parameter and enforce there; route handlers keep authentication only
- Load grants once per request into the actor context, not per check
- **Tests**: `service/src/delete.rs` and `upload.rs` reject an unauthorised
  `Actor` with no HTTP layer involved
- **Verify**: `cargo test -p picroom-service`

### ✅ Task 1.5: Close the route-level gaps (R-05, R-13)
- `handlers/images.rs` checks `Image/Create` before uploading
- `teams.rs:80,101,156` consult membership instead of discarding `_auth`;
  `GET /api/v1/teams` returns only the caller's teams
- Validate the `team_id` form field against membership (`images.rs:87`)
- **Tests**: a `viewer` token uploads → 403; user A cannot read user B's team,
  roster, or upload into their team
- **Verify**: `cargo test -p picroom-api -- handlers::{images,teams}`
- **Docs**: `tasks-admin-client.md` Task 1.6 claims `Team/Read` RBAC already
  ships; it does not — correct it here or in 1.10

### ✅ Task 1.6: ACL endpoints + OpenAPI (D-10)
- `GET /api/v1/images/:id/acl` — list grants
- `PUT /api/v1/images/:id/acl` — **replace** the full grant set (idempotent)
- `DELETE /api/v1/images/:id/acl/:subject_type/:subject_id` — remove one grant
- All three guarded by ownership or `manager`
- Document in `docs/api/openapi.yaml` in the same commit
- **Tests**: a non-owner without `manager` gets 403; `PUT` twice yields the same
  grant set; `DELETE` then re-`GET` shows it gone
- **Verify**: `cargo test -p picroom-api` + the OpenAPI drift check from 3.4

### ✅ Task 1.7: Make sessions real (R-08, R-20, D-6)
- `login` writes a `sessions` row; the JWT carries its `sid`
- `require_auth` rejects tokens whose `sid` is unknown or `revoked_at IS NOT NULL`
- `logout` sets `revoked_at`; `set_user_disabled` revokes every live session
- Accept `sid`-less tokens for one TTL window behind a config flag, then drop
- **Tests**: after logout the same token → 401; disabling a user kills their
  in-flight token; an already-revoked session is rejected
- **Verify**: `cargo test -p picroom-api -- auth` + `cargo test -p picroom-auth`
- **Migration**: none needed — `sessions` already exists

### ✅ Task 1.8: No orphan objects, no decoder internals in responses (R-19)
- On `repo.insert` failure, delete the stored object before returning 500
- `images.rs:73-80` maps probe failures to a typed status without embedding
  `ServiceError` text
- **Tests**: a forced insert failure leaves no bytes in storage; the 4xx body
  carries no decoder message
- **Verify**: `cargo test -p picroom-api`

### ✅ Task 1.9: Amend the spec and ADR to describe what ships (D-7, D-9)
- `spec.md` §10 / ADR-0005: document that an explicit deny now overrides the
  `admin` role, and that ACLs are resource-agnostic but only image endpoints exist
- Note that `Resource` is no longer dead code
- **Verify**: docs review — no code change

### Checkpoint P1
- [ ] All four evaluation rules covered, including deny-beats-admin
- [ ] viewer cannot upload; user A cannot read user B's team or image
- [ ] After logout the token returns 401; disabling a user kills live tokens
- [ ] The service layer rejects an unauthorised `Actor` with no HTTP layer involved
- [ ] Postgres and SQLite behave identically for every ACL path
- [ ] OpenAPI documents the three ACL endpoints

---

## Phase P2 — Reliability & configuration

### ✅ Task 2.1: Wire `[pipeline]` end to end (R-10)
- Add a config field to `ProcessorDeps`; populate it in `worker_cmd.rs:152-164`
- Apply `quality.avif`, `quality.webp`, `max_dimension`, and the
  `encode_avif` / `encode_webp` / `generate_thumbnail` toggles, replacing the
  hardcoded values at `processor.rs:176-177`
- Implement or explicitly reject `strip_exif` — stop advertising a key that does nothing
- **Tests**: `quality.avif = 25` changes output bytes; `max_dimension = 1024`
  bounds a 6000 px upload
- **Verify**: `cargo test -p picroom-worker -- pipeline`

### ✅ Task 2.2: Worker panic guard (R-11, D-8)
- `pool.rs:55` — wrap `handler(job)` in `AssertUnwindSafe(..).catch_unwind()`;
  convert a panic into a normal job failure so it flows through retry/DLQ
- `run_until` must respawn a dead slot instead of spinning on `None`
- **Tests**: a panicking handler leaves the worker alive with the job in the DLQ;
  `run_until` does not busy-spin
- **Verify**: `cargo test -p picroom-worker -- pool`

### ✅ Task 2.3: Job lease and reclaim (R-12, D-8)
- `0011_jobs_lease.sql` — add `lease_expires_at` + `claimed_by` to `jobs`
- `dequeue` claims `pending` **or** lease-expired `running` rows;
  `complete`/`fail` clear the lease
- **Tests**: a row stuck in `running` past its lease is re-claimed by another worker
- **Verify**: `cargo test -p picroom-worker -- db_queue`

### ✅ Task 2.4: Make variant upserts idempotent (R-18, D-5)
- Dedupe existing rows, then add
  `CREATE UNIQUE INDEX ON image_variants (image_id, kind, COALESCE(size, -1))`
- Change the upsert in `service/src/repo.rs:562-568` to the same expression
- **Tests**: enqueueing avif twice yields one row
- **Verify**: `cargo test -p picroom-service`

### ✅ Task 2.5: Audit the auth events and harden the table (R-17)
- Emit `AuditAction::Login` (success and failure) and `Logout` from
  `handlers/auth.rs`
- `0012_audit_append_only.sql` — guard `audit_events` against UPDATE/DELETE
- **Tests**: every login attempt and every logout appears in `GET /api/v1/audit`;
  an UPDATE against `audit_events` is rejected (**S14**)
- **Verify**: `cargo test -p picroom-audit -p picroom-api`

### ✅ Task 2.6: Close the quota gaps (R-21, Q-6)
- Implement team quotas against the `quotas` table — the migration needs a
  `team_id` column, so this adds a fourth migration
- Port `remaining_user` to SQLite (dev path must not silently lose enforcement)
- **Tests**: exceeding a user cap rejects the upload; exceeding a team cap rejects
  for every member; the same assertions run on both backends
- **Verify**: `cargo test -p picroom-service`
- **Note**: no stub may remain — if team quotas are deferred, delete the function
  and say so in `docs/deployment.md`

### ✅ Task 2.7: Paginate team queries (R-25)
- Add `LIMIT`/cursor paging to `repo.rs:670-676` and `list_members`, reusing the
  existing clamp-and-fetch-`limit+1` pattern from the images repo
- **Tests**: `GET /api/v1/teams?limit=500` clamps like `/images`; `has_more` is
  correct at the boundary
- **Verify**: `cargo test -p picroom-service`

### Checkpoint P2
- [ ] Config changes visibly affect encoder output
- [ ] A mid-job worker kill leads to a successful retry
- [ ] A panicking handler does not kill the worker
- [ ] Auth events show up in the audit log; the audit table rejects UPDATE

---

## Phase P3 — Hygiene & parity

### ✅ Task 3.1: Harden SigV4 (R-07)
- Enforce `within_skew` inside `verify()` — closes the R-03 replay surface
- When `payload_hash != UNSIGNED-PAYLOAD`, hash the received body and compare
- Reject when `SignedHeaders` omits `host` or `x-amz-date`, or a declared header
  is missing from the request
- Normalise canonical URI encoding
- **Tests**: a 2020-dated signature is rejected; a body-swapped PUT is rejected;
  a `SignedHeaders` set missing `host` is rejected
- **Verify**: `cargo test -p picroom-s3compat -- sigv4`

### ✅ Task 3.2: Honour the bucket and fix host signing (R-15, R-16)
- Validate bucket names; scope every object operation; return `NoSuchBucket`
- Sign `host` **with** its port in `driver/s3.rs:244` and `:593-597`
- Honour `prefix` / `max-keys` / `continuation-token` in ListObjectsV2
- **Tests**: `s3://other-bucket/` → `NoSuchBucket`; a MinIO round-trip signs correctly
- **Verify**: `cargo test -p picroom-storage -p picroom-s3compat`

### ✅ Task 3.3: Desktop client secrets and transport (R-22)
- Move the JWT out of `settings.json` into the OS keychain
- Refuse a non-HTTPS `server_url` outside dev builds
- Narrow `capabilities/default.json` from `http://*`
- **Tests**: no token written to disk; a clear error on `http://` in release
- **Verify**: desktop test plan in `docs/test-plan-admin-client.md`

### ✅ Task 3.4: Reconcile OpenAPI with the implementation (R-24)
- Align status codes (`POST /images`, `POST /teams/{id}/members`), the `{items}`
  envelope, and the audit filters
- Validate `driver` against `[local, s3]` in `storage.rs:79-92`
- Merge the competing `s3_xml_error` helpers into the escaping one in `error.rs`
- Add a CI drift check: every registered route must appear in the OpenAPI paths
- **Tests**: the drift check fails when a route is added without a spec entry
- **Verify**: `cargo test -p picroom-api` + CI

### ✅ Task 3.5: Decide the fate of API tokens (R-23)
- Either wire `ApiTokenService` into a real handler (mint/verify/revoke) or delete
  the service; use `subtle::ConstantTimeEq` for the hash comparison either way
- **Tests**: no unreferenced service remains; the compare is constant-time
- **Verify**: `cargo test -p picroom-auth`

### ✅ Task 3.6: Fix the GIF contract (R-26)
- Enable the GIF decoder feature or drop `image/gif` from `upload.rs:26`
- **Tests**: a GIF is either processed or rejected with 415 — never a 500
- **Resolution note**: dropped from the MIME gate — rejection is a 400
  ("unsupported content type"), not 415; the "never a 500" guarantee holds
- **Verify**: `cargo test -p picroom-service`

### ✅ Task 3.7: Honour CLI flags (R-27)
- `storage test --policy <name>` must test the named policy (`main.rs:195`)
- `audit tail`, `user`, `team` must honour `--config` like `migrate` (`main.rs:179`)
- `config validate` checks semantics, not just deserialization
- `migrate revert` stays fail-safe, but say so in `--help`
- **Tests**: `storage test --policy s3-main` fails when that policy's credentials
  are wrong
- **Verify**: `cargo test -p picroom-admin`

### ✅ Task 3.8: Resolve the duplicate imaging implementation (R-28, Q-7)
- Wire `crates/imaging` into the worker and delete the worker's private encoders
- Exactly one implementation survives
- **Verify**: `cargo test -p picroom-imaging -p picroom-worker`

### ✅ Task 3.9: Small correctness cleanups (R-29, R-30, R-31)
- Guard the zero-dimension resize in `imaging/src/processor/resize.rs:61-67`
- Make the local driver's temp filename unique and exclude it from listings
- Fix the retry off-by-one in `pool.rs:63`
- **Verify**: `cargo test --workspace`

### Checkpoint P3
- [ ] S11, S12 (aws cli + PicGo) verified end to end
- [ ] `cargo deny check`, `cargo audit`, `reuse lint` clean (S8, S9, S15)
- [ ] No unreferenced service, skeleton, or placeholder remains

---

## Phase P4 — Coverage backfill to 80 %

Per-task tests already landed in P0–P3 (D-12). This phase closes only the
pre-existing backlog, bottom-up on the worst crates.

### ⬜ Task 4.0: Flip the CI gate last
- Raise `.github/workflows/ci.yml` to `--fail-under 80` **only after** 4.1–4.5 are
  green — otherwise the gate blocks unrelated work
- Add a per-crate floor so one strong crate cannot mask another
- **Verify**: CI green on the same commit that raises the number

### 🔲 Task 4.1: `crates/audit` (53 % → 80 %)
- DB sink error paths, reader filters, redaction, append-only rejection
- **Verify**: `cargo tarpaulin -p picroom-audit`

### 🔲 Task 4.2: `crates/admin` (48 % → 80 %)
- Every CLI subcommand's error paths, exit codes, and `--help` paths
- **Verify**: `cargo tarpaulin -p picroom-admin`

### 🔲 Task 4.3: `crates/api` (54 % → 80 %)
- Handler error mapping, pagination edges, RBAC denial branches, ACL endpoints
- **Verify**: `cargo tarpaulin -p picroom-api`

### 🔲 Task 4.4: `crates/imaging` (56 % → 80 %)
- Encoder edge cases, EXIF handling, extreme aspect ratios, decode limits
- **Verify**: `cargo tarpaulin -p picroom-imaging`

### 🔲 Task 4.5: Remaining crates to the floor
- `service`, `storage`, `worker`, `s3compat` — verify each clears the per-crate floor
- **Verify**: `cargo tarpaulin --workspace`

### Checkpoint P4
- [ ] `cargo tarpaulin --workspace` ≥ 80 % overall and per crate
- [ ] CI enforces it and `spec.md` §1.4 S5 now matches reality

---

## Parallel lanes

| Lane | Tasks | Constraint |
|---|---|---|
| A | 0.1 → 0.5 | Sequential; 0.2 and 0.3 share the upload path |
| B | 1.1 → 1.2 → 1.3 → 1.4 → 1.5 → 1.6 | **Strictly sequential** — the ACL spine |
| C | 1.7, 1.8 | Start after 1.2; independent of 1.3–1.6 |
| D | 2.2, 2.3 | Each needs one migration; not concurrently |
| E | 3.1, 3.2, 3.4, 3.6 | Independent once P0 lands |
| F | 3.3 | Fully independent Rust↔Tauri boundary |
| G | 4.1, 4.2 | Independent; can start immediately |

**Migration serialization** is the only hard constraint. New files in order:
`0009_resource_acl_effect.sql`, `0010_sqlite_resource_acls.sql`,
`0011_jobs_lease.sql`, `0012_image_variants_unique.sql`,
`0013_audit_append_only.sql`, plus the team-quota migration in 2.6. Sessions
(D-6) needs none.