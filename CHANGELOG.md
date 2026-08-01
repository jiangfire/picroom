# Changelog

All notable changes to Picroom are documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

Post-1.0.0 hardening and feature work. Will roll up into 1.1.0.

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
  Team-level quotas still unlimited.
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
- Multi-backend storage: Local, S3, MinIO, OSS, COS, Qiniu drivers behind a
  capability-split `Storage` trait.
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