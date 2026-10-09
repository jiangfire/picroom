# Changelog

All notable changes to Picroom are documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [1.1.1] - 2026-10-09

### Fixed — v1.0 review remediation (docs/review-v1.0.md, all 32 findings R-01..R-32)

Critical (data loss / auth bypass):

- S3 multipart-shaped `PUT`/`DELETE` no longer fall through to whole-object
  handlers; they answer `501 NotImplemented` without touching storage (R-01).
- Thumbnail rows persist as `kind='thumbnail'` with the size in `size`
  (the old `thumbnail_200` kind violated the DB CHECK and rows vanished
  while jobs reported success) (R-02).
- `SigV4` verifies request freshness (±15 min) and hashes the received body
  when a payload hash is declared; `SignedHeaders` must include `host` and
  `x-amz-date` and every declared header must be present (R-03, R-07).
- Variant jobs are enqueued only after the `images` row is committed (R-04).

High:

- Authorization moved into the service layer (`UploadService`, `DeleteService`)
  with the full spec §10.3 evaluation order: explicit deny → ownership → team
  role → ACL allow → global role → default deny. An explicit deny now
  overrides even the `admin` role (R-05, R-09).
- `server.max_body_mb` is actually enforced via `DefaultBodyLimit`; uploads
  over the limit answer 413 (R-06).
- Team endpoints are scoped: `GET /teams` returns the caller's teams,
  `GET /teams/{id}[/members]` requires membership or `Team/Read` (404
  otherwise), and the `team_id` upload field is validated against membership
  (R-13).
- `logout` and disabling a user revoke outstanding tokens via real sessions
  (JWT `sid` ↔ `sessions` table) (R-08, R-20).
- ACLs: migrations 0009/0010, `ResourceAclRepository` (Pg + SQLite),
  `GET/PUT/DELETE /api/v1/images/{id}/acl`, OpenAPI documented (D-9–D-11).
- CI lint gate is green: quota stubs and the signing skeleton deleted, no
  `#[allow]` used (R-14).

Medium/Low:

- `[pipeline]` config is honored end to end: encoder quality, `max_dimension`,
  and the encode toggles; `strip_exif` documented as variants-only (R-10).
- Worker: per-job panic guard (jobs fail into retry/DLQ, slots survive) and
  job leases with dead-worker reclaim (R-11, R-12).
- Bucket scoping with `NoSuchBucket`; `host[:port]` signing everywhere
  (header-signed requests AND presigned URLs); `prefix`/`max-keys`/
  `continuation-token` in ListObjectsV2 (R-15, R-16).
- Login/logout are audited; `audit_events` is append-only (trigger-rejected
  UPDATE/DELETE) (R-17).
- `image_variants` upsert is idempotent (COALESCE unique index, migration
  0012) (R-18).
- Team quotas (`team_quotas`, migration 0014) with the SQLite dev path
  enforced; `charge_user` stub deleted (R-21).
- Desktop: JWT moved to the OS keychain, release builds refuse plain-HTTP
  server URLs, capabilities narrowed to https + localhost (R-22).
- OpenAPI reconciled with the implementation; a two-way route drift check
  runs in CI (R-24).
- Team queries are paginated and clamped (R-25).
- GIF uploads are rejected at the MIME gate instead of 500ing in the decoder
  (R-26).
- `storage test --policy` tests the named policy; `audit`/`user`/`team`
  honor `--config`; `config validate` checks semantics (R-27).
- `crates/imaging` is the single encoder implementation (worker's private
  copies deleted); resize guards zero dimensions; local temp files are unique
  and hidden from listings; the retry off-by-one no longer duplicates DLQ
  entries (R-28–R-31, R-32 type merge into `picroom-domain`).

Coverage gate raised 60 → 65 (spec target 80 % tracked in P4).

## [1.1.0] - 2026-10-07

Post-1.0.0 hardening and feature work (the `v1.1.0` tag).

### Added
- OIDC / SSO end-to-end (`/api/v1/auth/oidc/:provider/{login,callback}`):
  provider discovery, state/nonce CSRF binding, id_token verified against
  the provider JWKS (RS256/ES256), accounts auto-provisioned as `viewer`
  (or `admin` via `auth.oidc.admin_emails`). Closes the 1.0.0
  "OIDC SSO not wired" gap. See `crates/auth/src/oidc.rs` and
  `crates/api/src/handlers/auth.rs`.
- `GET /i/:key` public image-byte route + dependency-free magic-byte
  Content-Type sniffer; capability URLs (`img/{uuid_v7}.bin`) are the v1
  public-link model (ADR-0008).
- `GET /api/v1/images/:id/link` and `GET /api/v1/images/:id/file` for
  authenticated public/signed-URL generation; honors
  `server.public_url_base` when set, otherwise falls back to path-relative.
- Admin endpoints: `GET /api/v1/admin/users` (paginated),
  `POST /api/v1/admin/users/:id/{disable,enable}`,
  `PATCH /api/v1/admin/users/:id/role`,
  `GET /api/v1/teams` + `/teams/:id/members`,
  `GET/POST /api/v1/admin/storage/policies`.
- PostgreSQL implementations of the `admin user …` CLI (`user_create_pg`,
  `user_list_pg`, `user_set_role_pg`, `user_disable_pg`); SQLite paths
  coexist.
- Hard quota enforcement for the PG path: `QuotaService` rejects uploads
  once `quotas.max_bytes - SUM(bytes) < payload` (default 10 GiB per user).
  (Team-level caps arrived in 1.1.1 via the `team_quotas` table.)
- Desktop admin client (`desktop/`): Tauri 2 + Vue 3 native GUI for
  drag-drop upload, public-link copy, image/user/team/storage/audit admin
  screens. Multi-profile support. `desktop/src-tauri` is a standalone
  Cargo project (not a workspace member) — see ADR-0008 and
  [`docs/spec-admin-client.md`](docs/spec-admin-client.md).

### Fixed
- S3 `ListObjectsV2` actually lists instead of always returning 400.
- Storage-policy wiring in CI (root cause: missing migration row).
- IDOR bypass in image handlers (`Option<AuthUser>` → required `AuthUser`)
  and missing owner check on `GET/DELETE /images/:id`.
- Unbounded request bodies in axum (`RequestBodyLimitLayer` defaults to
  `max_body_mb = 100`).
- Path-traversal protection hardened in `LocalDriver`.

## [1.0.0] - 2026-07-11

First stable release. Single Rust binary self-hosted image hosting service
for teams.

### Added
- Cargo workspace with 11 library crates + 1 binary crate, internal crates
  versioned in lockstep.
- REST API (`/api/v1/`): auth (login/signup/refresh), image upload/list/get/
  delete with owner-scoped access, teams (create/get/add-member), admin user
  management (create-user/set-role), and audit log read endpoint.
- AWS S3-compatible endpoint (`/s3/*`) with SigV4 signature verification
  (constant-time comparison, verified against AWS test vectors) for PUT/GET/
  HEAD/DELETE and `ListObjectsV2`.
- Image pipeline: background worker producing AVIF + WebP variants and
  thumbnails, with retry/backoff and a dead-letter queue.
- Multi-backend storage: `LocalDriver` + `S3Driver` (with a `MinioDriver`
  type alias + `minio()` constructor convenience for MinIO endpoints)
  behind a capability-split `Storage` trait. OSS / COS / Qiniu drivers are
  planned (ADR-0003) but not implemented in v1.
- Authentication: JWT (strong-secret enforced in release builds) + API tokens
  + Argon2id password hashing + RBAC engine (`PermissionService`) wired into
  handlers.
- Per-user storage quota backed by the `quotas` table (default 1 GiB).
- Audit logging written to `audit_events` and readable via API and
  `admin audit tail`.
- Admin CLI: `migrate run`, `user`, `team`, `audit tail`, `storage-test`.
- Multi-tenancy data model with `team_id` persisted on images.
- CI/CD pipeline: fmt + clippy (`-D warnings`) + test + `cargo audit` +
  `cargo deny` + tarpaulin coverage + Postgres integration/E2E, all in the
  `required` gate.
- Docker Compose stack (Postgres 16 + MinIO + MailHog) with one-shot migrate
  service and a multi-stage distroless Dockerfile.
- Example configuration (`docker/config.example.toml`) aligned to the `Config`
  struct.
- OpenAPI 3.1 specification (`docs/api/openapi.yaml`).
- Operational endpoints: `/healthz`, `/readyz` (pings DB), `/metrics`
  (Prometheus).
- Design documentation: `docs/spec.md`, `docs/adr/` (7 ADRs), plus
  deployment, operations, and security runbooks. (8th ADR — the Tauri
  admin client — was added in the post-1.0.0 cycle.)

### Security
- All Rust dependencies pinned to minor versions and audited for MIT-only
  licenses via `cargo deny` (`wildcards = "deny"`).
- Internal errors sanitized at API and S3 boundaries; details logged
  server-side only.
- Path-traversal protection in the local storage driver.
- `unsafe_code = "forbid"` and `unused_must_use = "deny"` enforced workspace-
  wide; no `unwrap()`/`expect()` on production paths.
- SPDX-`MIT` license headers on every source file.

### Known limitations
- S3 multipart upload (`InitiateMultipartUpload`/`UploadPart`/`Complete`)
  returns an honest 501; single-shot PUT is supported.
- Watermark and EXIF stripping return `Err` (not implemented).
- `cargo audit` ignores 2 advisories from transitive `testcontainers` deps
  (dev-only); documented in `deny.toml`/`audit.toml`.