# Review: Picroom v1.0 Functionality Completeness & Correctness

> **Status**: Complete (read-only audit, no source changes) · scope decided 2026-10-07
> **Parent**: [`spec.md`](spec.md) · **Plan**: [`plan-remediation-v1.md`](plan-remediation-v1.md) · **Tasks**: [`tasks-remediation-v1.md`](tasks-remediation-v1.md)
> **Last updated**: 2026-10-07
> **Scope**: `crates/*` (12 crates, ~15.6k lines), `bin/picroom`, `desktop/`, `migrations/*`, `docs/api/openapi.yaml`
> **Commit reviewed**: `591d049` (master, 5 uncommitted doc changes)

This document records the evidence behind the remediation plan. It is a
snapshot, not a living status board — after fixes land, the finding IDs (R-xx)
stay stable and [`tasks-remediation-v1.md`](tasks-remediation-v1.md) tracks
their state.

## 1. Verdict

The engineering baseline is sound: the workspace compiles clean, 264 tests pass,
formatting is clean, the binary is well under budget, and the REST surface
matches the spec 1:1. The problems are **not** in the skeleton — they are in the
**business semantics beneath the endpoints**: authorization is largely absent at
the service layer, configuration is parsed but never applied, job lifecycle
guarantees are weaker than the pipeline diagram implies, and the S3 compatibility
layer has two data-loss paths.

Four defects are reachable and data-destructive. They should be fixed before any
external exposure.

### Verification status of findings

| Mark | Meaning |
|---|---|
| ✅ 已复核 | Read and confirmed against the source by the reviewer |
| ⚠️ 待复核 | Reported by delegated analysis, not independently re-read — confirm before fixing |

Every Critical and High finding is ✅ 已复核.

## 2. Baseline (measured)

| Check | Command | Result |
|---|---|---|
| Build | `cargo build --workspace` | ✅ clean |
| Tests | `cargo test --workspace` | ✅ **264 passed / 0 failed** |
| Format | `cargo fmt --all -- --check` | ✅ clean |
| Lint (CI gate) | `cargo clippy --all-targets --all-features --locked -- -D warnings` | ❌ **exit 101** |
| Binary size (S1) | `picroom.exe` | ✅ 11.87 MB (budget 40 MB) |
| Coverage | `cov_baseline.log` | ⚠️ api 54% / admin 48% / audit 53% / imaging 56% |

Two standing discrepancies:

- **CI is currently red.** The only failures are two `unused async` lint groups
  (`crates/storage/src/driver/s3.rs:84,662`, `crates/service/src/quota.rs:91,99`).
- **Coverage bar is lower than the spec.** `spec.md` §1.4 S5 requires ≥80 % line
  coverage; `.github/workflows/ci.yml` enforces `--fail-under 60`. Per-crate
  numbers in `cov_baseline.log` sit below even 60 for several crates.

## 3. Completeness

### 3.1 Contract alignment — good

`crates/api/src/router.rs` registers all 22 endpoints of `spec.md` §8.1. The
endpoints deliberately absent from the spec (`GET /api/v1/me`, `PATCH
/api/v1/teams/:id`, per-variant `/avif|/webp|/thumbnail`) are indeed absent from
the code, and the uncommitted `docs/api/openapi.yaml` change removes exactly
those. **The documentation is being corrected toward the implementation, which is
the right direction.** Keep those changes.

### 3.2 Configuration that is parsed and ignored — bad

| Config key | Declared | Actually applied |
|---|---|---|
| `[pipeline].quality.avif` | `crates/infra/src/config.rs:111-134` | ❌ hardcoded `60.0` at `crates/worker/src/processor.rs:176` |
| `[pipeline].quality.webp` | same | ❌ `webp_encode` passes no quality at all |
| `[pipeline].max_dimension` | same | ❌ never applied; a 12000 px upload encodes at full size |
| `[pipeline].strip_exif` | same | ❌ `processor.rs:73` returns `Err("strip-exif not yet implemented")` |
| `server.max_body_mb` | `config.rs:63` | ⚠️ unreachable — see R-06 |
| `quota.default_user_bytes` | `api_cmd.rs:40` | ✅ wired on the Postgres path |

Operator-visible symptom: tuning quality or `max_dimension` produces
byte-identical output with no error.

### 3.3 Spec'd features that are stubs

- **Team quota** — `spec.md` §9 lists per-team caps; `quota.rs:91`
  (`remaining_team`) returns `u64::MAX` unconditionally.
- **Quota on SQLite** — `api_cmd.rs:41` constructs `QuotaService::new()` with no
  pool, which reports unlimited. SQLite deployments have no quota enforcement at all.
- **`charge_user`** — `quota.rs:99` is a retained no-op with no caller.
- **EXIF stripping / watermark** — `processor.rs:72-73`; the upload path never
  enqueues either job kind, so originals keep full GPS EXIF.
- **`crates/imaging` processors** — `avif.rs:47`, `webp.rs:41`, `thumbnail.rs:41`
  all return input unchanged and are unused in production; the worker
  re-implements encoding in `processor.rs`, bypassing the crate entirely.
- **`migrate revert`** — `main.rs:167-170` always errors.
- **`config validate`** — `crates/admin/src/config_cmd.rs:26-30` only checks
  deserialization; exits 0 on semantically invalid config.
- **`storage test --policy`** — `main.rs:195` binds then discards the flag, always
  testing the default policy. A CI gate built on this reports false success.
- **`crates/storage/src/signing.rs:57`** — `verify()` is a `Phase 10` skeleton
  returning `Err("not implemented (skeleton)")`, and `canonical_request` returns
  an empty string. No non-test caller; dead code.
- **`sessions` table** (`migrations/0004_sessions_and_oidc.sql`) and the
  `revoked_at` field on `ApiToken` (`crates/auth/src/api_token.rs:28`) have **no
  code consumer** — no repository, no handler, no query.

## 4. Findings

Severity: **C**ritical (data loss / auth bypass) · **H**igh · **M**edium · **L**ow.

### Critical

#### R-01 — S3 multipart `PUT`/`DELETE` fall through to real object handlers ✅
`crates/s3compat/src/routes.rs:22-33`

Only `POST` is bound to `create_multipart`. The functions `upload_part`,
`complete_multipart` and `abort_multipart` (`multipart.rs:41,54,67`) have **no
route at all**. axum merges disjoint methods on a repeated `route()`, so:

| Request | Intended | Actual |
|---|---|---|
| `PUT /s3/b/k?partNumber=1&uploadId=U` | 501 stub | writes the **fragment as the whole object**, `200 + ETag` |
| `DELETE /s3/b/k?uploadId=U` | 501 stub | **deletes the real object**, `204` |

rclone or PicGo aborting a multipart upload destroys existing data.

#### R-02 — Thumbnail rows are never persisted; job reports success ✅
`crates/worker/src/processor.rs:70` writes `kind = format!("thumbnail_{size}")`
→ `"thumbnail_200"`, but `migrations/0002_storage_and_images.sql:43` constrains
`CHECK (kind IN ('avif','webp','thumbnail','watermark'))`. The insert error is
swallowed at `processor.rs:142` as a `tracing::warn!`, and the job is marked
succeeded. Bytes land in storage; no `image_variants` row; the API cannot see any
thumbnail.

#### R-03 — SigV4 has no clock-skew enforcement; signatures replay forever ✅
`crates/s3compat/src/middleware.rs:54-79`

`amz_date` is parsed and handed to `verify()`, but `sigv4::within_skew`
(`sigv4.rs:212`) has **zero non-test callers** — confirmed: it appears only at
`sigv4.rs:365` and `:375` inside `#[cfg(test)]`. Any captured PUT/DELETE
`Authorization` header replays indefinitely.

#### R-04 — Jobs are enqueued before the `images` row exists ✅
`crates/api/src/handlers/images.rs:70` calls `upload()`, which enqueues
avif/webp/thumbnail jobs internally (`upload.rs:214-259`). The row is only
inserted afterwards at `images.rs:91`. A fast worker picks the job up, finds no
`images` row, and retries or dead-letters a perfectly valid upload.

### High

#### R-05 — `POST /api/v1/images` performs no RBAC check ✅
`crates/api/src/handlers/images.rs:23-104` extracts `AuthUser` but only reads
`user_id`; `permissions.check` is never called. `rbac.rs:84-88` grants
`Image/Create` to uploader and above. A `viewer` token can upload arbitrary
images, contradicting `spec.md` §8.1 ("RBAC-enforced").

#### R-06 — The configured body limit is unreachable; real cap is 2 MiB ✅
`crates/s3compat/src/object.rs:68` extracts `body: Bytes`, and the REST upload
uses `Multipart`. axum 0.7 applies its own `DefaultBodyLimit` of 2 MiB inside the
extractor. `bin/picroom/src/api_cmd.rs:115` installs
`tower_http::limit::RequestBodyLimitLayer` — a **different mechanism** — and
`DefaultBodyLimit::max` appears **nowhere in the workspace** (verified by grep).
`server.max_body_mb = 100` and `UploadService::max_bytes` are both unreachable;
a 3 MB JPEG gets `413`. Criterion **S11 does not hold**.

#### R-07 — SigV4 neither verifies the body nor enforces `SignedHeaders` ⚠️
`crates/s3compat/src/middleware.rs:59-68`

- `payload_hash` is trusted from `x-amz-content-sha256`, defaulting to
  `UNSIGNED-PAYLOAD`; the received body is never hashed and compared.
- `canonical_headers` (`middleware.rs:97-109`) silently drops any signed header
  absent from the request, and nothing requires `host` or `x-amz-date` to be signed.
- `canonical_uri` is the raw `req.uri().path()` with no encoding normalisation.

An intercepted signed PUT carrying a real content hash can have its body swapped
and still verify.

#### R-08 — `logout` is a no-op; JWTs cannot be revoked ✅
`crates/api/src/handlers/auth.rs:86-88` returns `204` with no revocation, no
denylist, no `jti`. A stolen bearer token stays valid for its full TTL, and a
demoted admin remains admin until expiry. The `sessions.revoked_at` column and
`ApiToken::revoke` exist but are never consulted.

#### R-09 — The RBAC engine has no deny and no ACL; the service layer does no authz ✅
`crates/auth/src/rbac.rs:176-189` implements only "admin ⇒ allow everything,
otherwise check default permissions". The `Resource` struct carrying `owner_id`
(`rbac.rs:152-159`) is **dead code — never constructed anywhere in the workspace**.
`spec.md` §10.3 (explicit deny → team role → resource ACL → default deny) is
unimplemented. Ownership checks are re-implemented ad hoc at the route layer
(`images.rs:187,219,260,290`), so **any non-HTTP path reaching the service layer
performs no authorization at all** — `service/src/delete.rs` and `upload.rs`
never call `PermissionService`.

> **Scope decided 2026-10-07**: v1 ships real per-resource ACL per §10 in full
> (not a documented reduction). Note that `resource_acls` already exists in
> `migrations/0002_storage_and_images.sql:54-65` with no code consumer, so only
> `effect`, the repository, the evaluator, and the endpoints are new work. See
> [`plan-remediation-v1.md`](plan-remediation-v1.md) D-9/D-10 and tasks 1.2–1.6.

#### R-10 — The entire `[pipeline]` configuration is dead ⚠️
See §3.2. `worker_cmd.rs:152-164` builds `ProcessorDeps` with no config field;
`processor.rs:176-177` hardcodes AVIF quality 60 and speed 6.

#### R-11 — The worker has no panic guard ⚠️
`crates/worker/src/pool.rs:48-99` runs `handler(job).await` inside a bare `loop`.
One panic permanently kills that slot. `run_until` (`pool.rs:149-153`) then spins
on `set.join_next()`, which returns `None` **immediately and forever** once all
slots are dead. With `--concurrency 1` and one bad job the process stays
"healthy", pegs a CPU, processes nothing, and never exits.

#### R-12 — Jobs have no lease; they are lost permanently if a worker dies ⚠️
`crates/worker/src/db_queue.rs:69` selects `status = 'pending'` only. There is no
`lease_expires_at`, no reclaim of stale `running` rows, and no reaper. A worker
killed mid-`avif_encode` leaves the row `running` forever — the image permanently
has no AVIF variant and no retry ever fires.

#### R-13 — Team endpoints are unscoped (IDOR) ✅
`crates/api/src/handlers/teams.rs:80-98` and `:101-122` bind `_auth` and discard
it, then return the requested team or **all** teams. Any authenticated user can
enumerate every team and roster. `images.rs:87` also accepts an arbitrary
`team_id` form field with no membership check and binds it straight into the row.

Note: [`tasks-admin-client.md`](tasks-admin-client.md) Task 1.6 marks team listing
as ✅ done with "`Team/Read` RBAC" — that check does not exist in the code.

#### R-14 — CI lint gate is failing ✅
`cargo clippy --all-targets --all-features --locked -- -D warnings` exits 101.
Two `unused async` groups, one of which (`quota.rs:91,99`) is itself a stub
(§3.3), so removing `async` blindly would cement dead API. Fix the stub, then
the lint.

### Medium

| ID | Finding | Evidence |
|---|---|---|
| R-15 | S3 ignores the bucket entirely — no isolation, no validation, no `NoSuchBucket` | `s3compat/src/object.rs:39,66,84,100` bind `_bucket`; `bucket.rs:23,53` have no non-test caller ⚠️ |
| R-16 | `S3Driver` signs `host` without the port → `SignatureDoesNotMatch` against MinIO/self-hosted | `storage/src/driver/s3.rs:244` uses `url.host_str()`; also `sign_get_url` at `:593-597` ⚠️ |
| R-17 | Auth events are never audited; audit table is not append-only | `AuditAction::Login`/`Logout` appear **only** in `crates/audit/src/event.rs` tests ✅; `migrations/0003_audit_jobs_tokens.sql:17-33` has no `UPDATE`/`DELETE` guard ✅. S14 unmet ✅ |
| R-18 | Re-running AVIF/WebP jobs duplicates `image_variants` rows | `repo.rs:562-568` relies on `ON CONFLICT (image_id, kind, size)`, but `size` is `NULL` for avif/webp; Postgres treats NULLs as distinct in a unique index ⚠️ |
| R-19 | Orphan objects on insert failure; decoder internals leak to the caller | `images.rs:91-95` leaves stored bytes behind; `images.rs:73-80` returns raw `ServiceError` text at 400 ✅ |
| R-20 | Disabled users keep working | `set_user_disabled` flips a flag; issued JWTs remain valid for their TTL ⚠️ |
| R-21 | Quota gaps | see §3.3 ✅ |
| R-22 | Desktop client stores the JWT in plaintext and does not enforce TLS | `desktop/src-tauri/src/store.rs:26-27`, `capabilities/default.json:17` ⚠️ |
| R-23 | API tokens: non-constant-time compare, and no handler ever mints or verifies one | `auth/src/api_token.rs:83-85` uses `String ==`; `ApiTokenService` is referenced only by its own tests ⚠️ |
| R-24 | OpenAPI ↔ code divergence | `POST /images` spec 201 vs code 200; `POST /teams/{id}/members` spec 201 vs code 204; list endpoints spec bare arrays vs code `{items}`; `GET /audit` declares `actor_id`/`action`/`from`/`to` but code accepts only `limit`/`before`; `driver` enum narrowed to `[local, s3]` but `storage.rs:79-92` accepts any string ✅ |
| R-25 | Team queries are unbounded | `service/src/repo.rs:670-676` has no `LIMIT`; `list_members` likewise ⚠️ |
| R-26 | GIF passes the MIME gate but cannot be decoded → 500 | `upload.rs:26` allows `image/gif`; `Cargo.toml:97` enables only `["jpeg","png","webp"]` ⚠️ |
| R-27 | CLI flags silently ignored | `storage test --policy` (`main.rs:195`), `audit tail`/`user`/`team` ignore `--config` (`main.rs:179`) ⚠️ |
| R-28 | `crates/imaging` is entirely bypassed | see §3.3 ⚠️ |

### Low

| ID | Finding | Evidence |
|---|---|---|
| R-29 | Resize can compute a zero dimension → panic (contained by `spawn_blocking`) | `imaging/src/processor/resize.rs:61-67` ⚠️ |
| R-30 | Local driver temp file `.{name}.tmp` is not unique and leaks into listings | `storage/src/driver/local.rs:132-139`, filter at `:224-230` never matches ⚠️ |
| R-31 | Retry off-by-one pushes duplicate DLQ entries | `pool.rs:63` adds 1 to an already-incremented `attempts` ⚠️ |
| R-32 | `Permission` / `ResourceType` / `PermissionAction` are defined **twice** — `auth/src/rbac.rs:21-58` and `domain/src/permission.rs:11-50` — with no conversion between the two types | ✅ Should be merged before R-09 is fixed, or the two will silently disagree |

## 5. Verified correct

Worth stating so the next reviewer knows what was covered.

- **Path traversal is blocked centrally.** `StorageKey::parse`
  (`domain/src/storage_key.rs:52-62`) rejects `..` segments, leading `/`,
  backslash, NUL and any non-allowlisted character, for reads *and* writes.
- **`/i/*key` cannot serve stored XSS.** Content-Type is an allowlist
  (`public.rs:66-79`) that never emits `text/html` or `image/svg+xml`; unknown
  bytes fall back to `application/octet-stream`. `Cache-Control: immutable` is
  correct for uuid-keyed objects.
- **OIDC CSRF protection is real.** A signed, `aud:"oidc-state"`-separated 600 s
  state token is minted per request and compared against the callback parameter
  (`handlers/auth.rs:155-161`); cookie flags are `HttpOnly` + `SameSite=Lax`.
  A test at `jwt.rs:232` proves a normal bearer token cannot be replayed as state.
- **No user enumeration on login.** Unknown email, wrong password and disabled
  account all return an identical `401 "invalid credentials"`.
- **Error hygiene on the generic path.** `ApiError::internal` logs the detail and
  returns a fixed `"internal server error"` (`api/src/error.rs:70-78`).
- **SigV4 primitives are correct** — canonical-request layout, string-to-sign,
  key derivation, lowercase hex, constant-time comparison (`sigv4.rs:182-208`).
- **Postgres job claiming is genuinely at-least-once** — `FOR UPDATE SKIP LOCKED`
  in a single CTE (`worker/src/db_queue.rs:66-79`).
- **Image read/link/file/delete ownership gates are consistently applied**
  (`images.rs:187,219,260,290`).

## 6. Documentation inconsistencies found

1. [`tasks-admin-client.md`](tasks-admin-client.md) Task 1.6 claims team listing
   ships with `Team/Read` RBAC. It does not (R-13).
2. `spec.md` §1.4 S5 requires ≥80 % coverage; CI enforces 60.
3. `crates/storage/src/signing.rs` still carries "Phase 10" placeholder text and
   a test asserting the skeleton returns an error — a test that locks in dead code.
4. `CHANGELOG.md` hardening claims are stronger than the code (local writes bypass
   the escape check at `local.rs:122,152,114`, which use `resolve_unchecked`).