# Changelog

All notable changes to Picroom are documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

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
  deployment, operations, and security runbooks.

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
- OIDC SSO is not wired (handlers return 501).
- S3 multipart upload (`InitiateMultipartUpload`/`UploadPart`/`Complete`)
  returns an honest 501; single-shot PUT is supported.
- Watermark and EXIF stripping return `Err` (not implemented).
- `cargo audit` ignores 2 advisories from transitive `testcontainers` deps
  (dev-only); documented in `deny.toml`/`audit.toml`.