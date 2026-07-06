# Picroom — Task Breakdown (v1.0)

> **Status**: Draft for review · **Companion to**: `docs/spec.md`, `docs/plan.md`
> **Last updated**: 2026-07-05

Each task follows the TDD loop: **Red (test) → Green (impl) → Refactor**.
Each task lists its acceptance criteria, verification command, and the files
it will touch. Tasks are ordered by dependency. No task should require
modifying more than ~5 files.

Conventions:

- ✅ Acceptance criteria is a testable statement.
- 🧪 Verify gives the command(s) to confirm the task is done.
- 📁 Files lists the absolute or workspace-relative paths to be touched.
- Commit message format: `task(<phase>-<id>): <short description>`

---

## Phase 0 — Repo skeleton & CI

### Task 0.1 — Workspace root

- ✅ Acceptance: `Cargo.toml` declares workspace with all 11 member crates
  and shared `[workspace.dependencies]`.
- 🧪 Verify: `cargo build --workspace` succeeds with empty crates.
- 📁 Files: `Cargo.toml`, `Cargo.lock` (generated).

### Task 0.2 — Toolchain pinning

- ✅ Acceptance: `rust-toolchain.toml` pins stable toolchain.
- 🧪 Verify: `rustup show active-toolchain` matches file.
- 📁 Files: `rust-toolchain.toml`, `.cargo/config.toml`.

### Task 0.3 — .gitignore + .dockerignore

- ✅ Acceptance: `target/`, IDE files, secrets ignored; build context small.
- 🧪 Verify: `git status --ignored` shows `target/`; `docker build --dry-run` works.
- 📁 Files: `.gitignore`, `.dockerignore`.

### Task 0.4 — README + LICENSE

- ✅ Acceptance: `LICENSE` is full MIT text; `README.md` has badges,
  install, quick-start.
- 🧪 Verify: `reuse lint` clean; visual review.
- 📁 Files: `LICENSE`, `README.md`.

### Task 0.5 — CONTRIBUTING + SECURITY

- ✅ Acceptance: contributing guide covers workflow, code style, TDD mandate.
- 🧪 Verify: visual review.
- 📁 Files: `CONTRIBUTING.md`, `SECURITY.md`.

### Task 0.6 — deny.toml + tarpaulin.toml

- ✅ Acceptance: deny allows only MIT/Apache-2.0/MPL-2.0; tarpaulin excludes tests.
- 🧪 Verify: `cargo deny check` clean.
- 📁 Files: `deny.toml`, `tarpaulin.toml`.

### Task 0.7 — Dockerfile

- ✅ Acceptance: multi-stage build, runtime image based on `gcr.io/distroless/cc-debian12`,
  non-root user, ≤ 30 MB.
- 🧪 Verify: `docker build -f docker/Dockerfile .` succeeds; image size
  measured.
- 📁 Files: `docker/Dockerfile`.

### Task 0.8 — docker-compose.yml (dev)

- ✅ Acceptance: brings up PG 16 + MinIO + mailhog; exposes ports.
- 🧪 Verify: `docker compose -f docker/docker-compose.yml up -d` succeeds.
- 📁 Files: `docker/docker-compose.yml`.

### Task 0.9 — CI workflow

- ✅ Acceptance: fmt + clippy + test + audit + deny + coverage all run on PR.
- 🧪 Verify: push a commit, CI is green.
- 📁 Files: `.github/workflows/ci.yml`.

### Task 0.10 — Release workflow

- ✅ Acceptance: tag-driven multi-arch image push to ghcr.io.
- 🧪 Verify: `git tag v0.0.1` triggers release workflow.
- 📁 Files: `.github/workflows/release.yml`.

---

## Phase 1 — `domain` crate

### Task 1.1 — DomainError

- ✅ Acceptance: `DomainError` enum with variants per spec §9.
- 🧪 Verify: `cargo test -p picroom-domain domain::error` green.
- 📁 Files: `crates/domain/src/error.rs`.

### Task 1.2 — StorageKey

- ✅ Acceptance: `StorageKey::parse` validates rules; rejects path traversal,
  leading slash, empty, too-long.
- 🧪 Verify: table-driven tests cover valid/invalid inputs.
- 📁 Files: `crates/domain/src/storage_key.rs`.

### Task 1.3 — Page<T>

- ✅ Acceptance: `Page<T>` and `PageReq` with cursor-based pagination.
- 🧪 Verify: round-trip serialization + cursor encode/decode.
- 📁 Files: `crates/domain/src/page.rs`.

### Task 1.4 — User entity

- ✅ Acceptance: `User`, `UserId`; constructors validate email.
- 🧪 Verify: invalid email rejected; serialization stable.
- 📁 Files: `crates/domain/src/user.rs`.

### Task 1.5 — Team entity

- ✅ Acceptance: `Team`, `TeamId`, `TeamMember`.
- 🧪 Verify: add/remove member logic tested.
- 📁 Files: `crates/domain/src/team.rs`.

### Task 1.6 — Role + Permission

- ✅ Acceptance: `Role` enum (Admin, Manager, Uploader, Viewer, Custom);
  `Permission` enum with image/team/user/audit/storage_policy actions.
- 🧪 Verify: `Role::default_permissions()` returns expected set per role.
- 📁 Files: `crates/domain/src/role.rs`, `crates/domain/src/permission.rs`.

### Task 1.7 — Image entity

- ✅ Acceptance: `Image`, `ImageId`, `ImageVariant`.
- 🧪 Verify: aspect_ratio, hash helpers tested; serde round-trip.
- 📁 Files: `crates/domain/src/image.rs`.

### Task 1.8 — Clock trait

- ✅ Acceptance: `Clock` trait + `SystemClock` impl + `FakeClock` (test).
- 🧪 Verify: time injection works.
- 📁 Files: `crates/domain/src/clock.rs`.

### Task 1.9 — ID generator

- ✅ Acceptance: UUID v7 generator; monotonic within same millisecond.
- 🧪 Verify: 1000 IDs unique + ordered.
- 📁 Files: `crates/domain/src/id.rs`.

### Task 1.10 — Domain lib entry

- ✅ Acceptance: `pub use` for every public type.
- 🧪 Verify: `cargo doc -p picroom-domain` builds clean.
- 📁 Files: `crates/domain/src/lib.rs`.

---

## Phase 2 — `infra` crate

### Task 2.1 — Config loader

- ✅ Acceptance: `figment` with env (PICROOM_*) > TOML > default precedence.
- 🧪 Verify: env overrides TOML; missing required keys error clearly.
- 📁 Files: `crates/infra/src/config.rs`, `config/example.toml`.

### Task 2.2 — DB pool

- ✅ Acceptance: `PgPool` + `SqlitePool` (feature-gated); health check.
- 🧪 Verify: integration test connects to testcontainers PG.
- 📁 Files: `crates/infra/src/db.rs`.

### Task 2.3 — Cache (optional)

- ✅ Acceptance: `Cache` trait + in-memory impl; Redis impl feature-gated.
- 🧪 Verify: round-trip get/set.
- 📁 Files: `crates/infra/src/cache.rs`.

### Task 2.4 — Logging

- ✅ Acceptance: JSON logs to stdout; level from `RUST_LOG`.
- 🧪 Verify: snapshot test on log output.
- 📁 Files: `crates/infra/src/logging.rs`.

### Task 2.5 — Telemetry hook

- ✅ Acceptance: `metrics` registry + `/metrics` exporter (impl in `api`).
- 🧪 Verify: counter increments visible.
- 📁 Files: `crates/infra/src/telemetry.rs`.

### Task 2.6 — Infra lib entry

- ✅ Acceptance: re-exports + `init()` orchestrator.
- 🧪 Verify: `cargo test -p picroom-infra` green.
- 📁 Files: `crates/infra/src/lib.rs`.

---

## Phase 3 — `storage` crate

### Task 3.1 — Traits

- ✅ Acceptance: `StorageReader`, `StorageWriter`, `StorageLister`,
  `StorageSigner` traits + supertrait `Storage`.
- 🧪 Verify: `cargo build -p picroom-storage`.
- 📁 Files: `crates/storage/src/driver/mod.rs`.

### Task 3.2 — StorageError

- ✅ Acceptance: `StorageError` (NotFound, PermissionDenied, Network,
  Backend, Config) with `From` impls.
- 🧪 Verify: error display + source chain tested.
- 📁 Files: `crates/storage/src/error.rs`.

### Task 3.3 — AnyStorage enum

- ✅ Acceptance: enum dispatch implements all traits; no `dyn`.
- 🧪 Verify: round-trip via enum matches each driver.
- 📁 Files: `crates/storage/src/any.rs`.

### Task 3.4 — Contract test

- ✅ Acceptance: `contract_test::put_get_delete_roundtrip` +
  `list_paginated` + `sign_url_valid` shared by all drivers.
- 🧪 Verify: local driver passes; reused by 3.5–3.8.
- 📁 Files: `crates/storage/src/contract_test.rs`.

### Task 3.5 — LocalDriver

- ✅ Acceptance: read/write/list under configured root; atomic write
  via temp + rename.
- 🧪 Verify: contract test green.
- 📁 Files: `crates/storage/src/driver/local.rs`.

### Task 3.6 — S3Driver

- ✅ Acceptance: AWS S3 + MinIO compatible; multipart; presigned URLs.
- 🧪 Verify: contract test green against testcontainers MinIO.
- 📁 Files: `crates/storage/src/driver/s3.rs`.

### Task 3.7 — Signing helpers

- ✅ Acceptance: `SigV4::sign` + `OssV1::sign` (post-MVP V1 only, but trait ready).
- 🧪 Verify: matches AWS test vectors.
- 📁 Files: `crates/storage/src/signing.rs`.

### Task 3.8 — Storage lib entry

- ✅ Acceptance: re-exports + `from_config` factory.
- 🧪 Verify: `cargo test -p picroom-storage` green.
- 📁 Files: `crates/storage/src/lib.rs`.

---

## Phase 4 — `imaging` crate

### Task 4.1 — Processor trait

- ✅ Acceptance: `Processor` trait with `process(ctx, input) -> Result<Output>`.
- 🧪 Verify: `cargo build -p picroom-imaging`.
- 📁 Files: `crates/imaging/src/lib.rs`.

### Task 4.2 — Pipeline runner

- ✅ Acceptance: sequential `Pipeline` of processors; context propagates.
- 🧪 Verify: pipeline test asserts order.
- 📁 Files: `crates/imaging/src/pipeline.rs`.

### Task 4.3 — ProbeProcessor

- ✅ Acceptance: reads EXIF, dimensions, MIME.
- 🧪 Verify: known fixtures produce known values.
- 📁 Files: `crates/imaging/src/processor/probe.rs`.

### Task 4.4 — ResizeProcessor

- ✅ Acceptance: scales down to max dimension; preserves aspect; skips if smaller.
- 🧪 Verify: golden output images checked in.
- 📁 Files: `crates/imaging/src/processor/resize.rs`.

### Task 4.5 — AvifProcessor

- ✅ Acceptance: encodes AVIF; quality + speed configurable.
- 🧪 Verify: golden test; benchmark ≤ 1.5 s for 4 MB.
- 📁 Files: `crates/imaging/src/processor/avif.rs`.

### Task 4.6 — WebpProcessor

- ✅ Acceptance: encodes WebP; quality configurable.
- 🧪 Verify: golden test.
- 📁 Files: `crates/imaging/src/processor/webp.rs`.

### Task 4.7 — ThumbnailProcessor

- ✅ Acceptance: produces 3 sizes (200/400/800 px).
- 🧪 Verify: outputs match fixtures.
- 📁 Files: `crates/imaging/src/processor/thumbnail.rs`.

### Task 4.8 — WatermarkProcessor (post-MVP, trait-ready)

- ✅ Acceptance: text + image overlays; position configurable.
- 🧪 Verify: golden test.
- 📁 Files: `crates/imaging/src/processor/watermark.rs`.

---

## Phase 5 — `auth` crate

### Task 5.1 — Password (argon2)

- ✅ Acceptance: hash + verify; default params meet OWASP recommendations.
- 🧪 Verify: known-vector round-trip.
- 📁 Files: `crates/auth/src/password.rs`.

### Task 5.2 — JWT

- ✅ Acceptance: issue + verify; supports HS256 / RS256.
- 🧪 Verify: expired token rejected; forged signature rejected.
- 📁 Files: `crates/auth/src/jwt.rs`.

### Task 5.3 — API token

- ✅ Acceptance: random token stored as hash; revoke supported.
- 🧪 Verify: rotation tested.
- 📁 Files: `crates/auth/src/api_token.rs`.

### Task 5.4 — OIDC

- ✅ Acceptance: discover + callback + token exchange + ID-token validation.
- 🧪 Verify: mock provider round-trip.
- 📁 Files: `crates/auth/src/oidc.rs`.

### Task 5.5 — RBAC

- ✅ Acceptance: `check(actor, action, resource) -> Decision` with
  deny-overrides-allow semantics.
- 🧪 Verify: table-driven test covers all role × action combos.
- 📁 Files: `crates/auth/src/rbac.rs`.

### Task 5.6 — Auth lib entry

- ✅ Acceptance: re-exports + `AuthService` orchestrator.
- 🧪 Verify: `cargo test -p picroom-auth` green.
- 📁 Files: `crates/auth/src/lib.rs`.

---

## Phase 6 — `audit` crate

### Task 6.1 — Event struct

- ✅ Acceptance: `Event` with all fields per spec; serde stable.
- 🧪 Verify: snapshot test.
- 📁 Files: `crates/audit/src/event.rs`.

### Task 6.2 — AuditSink trait + DbAuditSink

- ✅ Acceptance: async write + read-by-filter.
- 🧪 Verify: insert + query round-trip.
- 📁 Files: `crates/audit/src/sink.rs`.

### Task 6.3 — Audit lib entry

- ✅ Acceptance: `record!` macro or function for ergonomic callsites.
- 🧪 Verify: usage example compiles.
- 📁 Files: `crates/audit/src/lib.rs`.

---

## Phase 7 — `service` crate

### Task 7.1 — UploadService

- ✅ Acceptance: validate → probe → persist original → enqueue job;
  quota check.
- 🧪 Verify: mockall-driven unit tests + integration test with real storage.
- 📁 Files: `crates/service/src/upload.rs`.

### Task 7.2 — ImageQueryService

- ✅ Acceptance: list / get / variants; filter by owner, team, hash.
- 🧪 Verify: pagination + ACL filtering.
- 📁 Files: `crates/service/src/query.rs`.

### Task 7.3 — DeleteService

- ✅ Acceptance: permission check → delete variants → delete original →
  audit.
- 🧪 Verify: deletes all rows + storage objects.
- 📁 Files: `crates/service/src/delete.rs`.

### Task 7.4 — QuotaService

- ✅ Acceptance: hard cap rejects with 413; soft cap warns.
- 🧪 Verify: per-user + per-team counters updated.
- 📁 Files: `crates/service/src/quota.rs`.

### Task 7.5 — PermissionService

- ✅ Acceptance: thin wrapper over `picroom-auth::rbac`.
- 🧪 Verify: consistent with crate.
- 📁 Files: `crates/service/src/permission.rs`.

### Task 7.6 — Service lib entry

- ✅ Acceptance: re-exports + `Services` struct.
- 🧪 Verify: `cargo test -p picroom-service` green; coverage ≥ 80 %.
- 📁 Files: `crates/service/src/lib.rs`.

---

## Phase 8 — `worker` crate

### Task 8.1 — Job enum + JobQueue trait

- ✅ Acceptance: `Job` + `JobQueue` trait + DB-backed impl with `SKIP LOCKED`.
- 🧪 Verify: enqueue + dequeue round-trip.
- 📁 Files: `crates/worker/src/job.rs`.

### Task 8.2 — Retry policy

- ✅ Acceptance: exponential backoff, max 5 retries, then DLQ.
- 🧪 Verify: simulated failures land in DLQ.
- 📁 Files: `crates/worker/src/retry.rs`.

### Task 8.3 — Worker pool

- ✅ Acceptance: configurable concurrency; graceful shutdown.
- 🧪 Verify: 1000 jobs throughput test.
- 📁 Files: `crates/worker/src/pool.rs`.

### Task 8.4 — Worker lib entry

- ✅ Acceptance: `Worker::run()` entry.
- 🧪 Verify: `cargo test -p picroom-worker` green.
- 📁 Files: `crates/worker/src/lib.rs`.

---

## Phase 9 — `api` crate

### Task 9.1 — Router scaffolding

- ✅ Acceptance: `/healthz`, `/readyz`, `/metrics`, `/api/v1/*` mounted.
- 🧪 Verify: `cargo run -- api` and `curl /healthz` returns 200.
- 📁 Files: `crates/api/src/router.rs`.

### Task 9.2 — Auth middleware + extractors

- ✅ Acceptance: `AuthUser` extractor reads JWT/OIDC/API-token.
- 🧪 Verify: unit tests for each path; integration test via cookie.
- 📁 Files: `crates/api/src/extractors/auth.rs`, `crates/api/src/middleware/auth.rs`.

### Task 9.3 — Permission guard

- ✅ Acceptance: rejects with 403 on denial; logs audit.
- 🧪 Verify: table-driven.
- 📁 Files: `crates/api/src/middleware/permission.rs`.

### Task 9.4 — Request ID + tracing

- ✅ Acceptance: per-request span with `request_id`, `actor`, `route`.
- 🧪 Verify: log line includes all fields.
- 📁 Files: `crates/api/src/middleware/trace.rs`.

### Task 9.5 — Auth handlers

- ✅ Acceptance: `/auth/login`, `/auth/oidc/:provider/callback`, `/auth/logout`.
- 🧪 Verify: integration tests cover happy + sad paths.
- 📁 Files: `crates/api/src/handlers/auth.rs`.

### Task 9.6 — Image handlers

- ✅ Acceptance: upload (multipart), list, get, variants, delete.
- 🧪 Verify: integration tests with real storage.
- 📁 Files: `crates/api/src/handlers/images.rs`.

### Task 9.7 — Team + admin handlers

- ✅ Acceptance: CRUD for team, members, roles, users, audit.
- 🧪 Verify: integration tests.
- 📁 Files: `crates/api/src/handlers/teams.rs`, `crates/api/src/handlers/admin.rs`.

### Task 9.8 — Error mapper

- ✅ Acceptance: `DomainError` → HTTP status + JSON body.
- 🧪 Verify: table of error → status code.
- 📁 Files: `crates/api/src/error.rs`.

### Task 9.9 — API lib entry

- ✅ Acceptance: `app(router) -> Router` factory.
- 🧪 Verify: `cargo test -p picroom-api` green.
- 📁 Files: `crates/api/src/lib.rs`.

---

## Phase 10 — `s3compat` crate

### Task 10.1 — SigV4 verifier

- ✅ Acceptance: parses `Authorization` header; validates signature.
- 🧪 Verify: AWS test vectors pass.
- 📁 Files: `crates/s3compat/src/sigv4.rs`.

### Task 10.2 — Bucket dispatch

- ✅ Acceptance: path-style URL parsing; bucket → policy mapping.
- 🧪 Verify: unit tests.
- 📁 Files: `crates/s3compat/src/bucket.rs`.

### Task 10.3 — Object handlers

- ✅ Acceptance: PUT, GET, HEAD, DELETE on `/s3/:bucket/:key`.
- 🧪 Verify: `aws s3 cp` round-trip.
- 📁 Files: `crates/s3compat/src/object.rs`.

### Task 10.4 — Multipart upload

- ✅ Acceptance: init / part / complete / abort.
- 🧪 Verify: 100 MB file uploads via 5 MB parts.
- 📁 Files: `crates/s3compat/src/multipart.rs`.

### Task 10.5 — ListObjectsV2

- ✅ Acceptance: paginated listing with prefix + delimiter.
- 🧪 Verify: AWS CLI `aws s3 ls` matches.
- 📁 Files: `crates/s3compat/src/list.rs`.

### Task 10.6 — S3compat lib entry

- ✅ Acceptance: `mount(router) -> Router`.
- 🧪 Verify: `cargo test -p picroom-s3compat` green.
- 📁 Files: `crates/s3compat/src/lib.rs`.

---

## Phase 11 — `admin` crate

### Task 11.1 — Migrate subcommand

- ✅ Acceptance: `admin migrate {run,revert,status}`.
- 🧪 Verify: against test DB.
- 📁 Files: `crates/admin/src/migrate.rs`.

### Task 11.2 — User subcommand

- ✅ Acceptance: create / list / set-role / disable.
- 🧪 Verify: e2e shell test.
- 📁 Files: `crates/admin/src/user.rs`.

### Task 11.3 — Team subcommand

- ✅ Acceptance: create / list / add-member.
- 🧪 Verify: e2e shell test.
- 📁 Files: `crates/admin/src/team.rs`.

### Task 11.4 — Audit tail

- ✅ Acceptance: `audit tail --follow --filter actor=…`.
- 🧪 Verify: streams events.
- 📁 Files: `crates/admin/src/audit.rs`.

### Task 11.5 — Config validate

- ✅ Acceptance: `config validate` + `config print`.
- 🧪 Verify: error on bad config.
- 📁 Files: `crates/admin/src/config_cmd.rs`.

### Task 11.6 — Storage test

- ✅ Acceptance: round-trip a file via configured policy.
- 🧪 Verify: works for each driver.
- 📁 Files: `crates/admin/src/storage_test.rs`.

---

## Phase 12 — `picroom` binary

### Task 12.1 — main entry

- ✅ Acceptance: `picroom <api|worker|admin> [--config PATH]` dispatcher.
- 🧪 Verify: each subcommand reachable; `--help` works.
- 📁 Files: `bin/picroom/src/main.rs`.

### Task 12.2 — Graceful shutdown

- ✅ Acceptance: SIGTERM → drain 30s → exit 0.
- 🧪 Verify: integration test kills process; exit code 0.
- 📁 Files: `bin/picroom/src/shutdown.rs`.

### Task 12.3 — Startup banner

- ✅ Acceptance: `--version` prints version + commit hash + license.
- 🧪 Verify: visual review.
- 📁 Files: `bin/picroom/src/banner.rs`.

### Task 12.4 — Smoke test script

- ✅ Acceptance: `scripts/smoke.sh` exercises api + s3 + worker.
- 🧪 Verify: passes locally + in CI.
- 📁 Files: `scripts/smoke.sh`.

---

## Phase 13 — Hardening & docs

### Task 13.1 — Audit + deny clean

- ✅ Acceptance: zero vulnerabilities; MIT-only deps.
- 🧪 Verify: `cargo audit` + `cargo deny check` green.
- 📁 Files: (any).

### Task 13.2 — Coverage ≥ 80 %

- ✅ Acceptance: workspace coverage ≥ 80 % lines.
- 🧪 Verify: `cargo tarpaulin --workspace` ≥ 80.
- 📁 Files: (any).

### Task 13.3 — Deployment docs

- ✅ Acceptance: `docs/deployment.md` covers compose + k8s.
- 🧪 Verify: docs review.
- 📁 Files: `docs/deployment.md`.

### Task 13.4 — Operations docs

- ✅ Acceptance: `docs/operations.md` covers backup, restore, upgrade.
- 🧪 Verify: docs review.
- 📁 Files: `docs/operations.md`.

### Task 13.5 — Security docs

- ✅ Acceptance: `docs/security.md` covers threat model, hardening.
- 🧪 Verify: docs review.
- 📁 Files: `docs/security.md`.

### Task 13.6 — OpenAPI generation

- ✅ Acceptance: `docs/api/openapi.yaml` generated from code annotations.
- 🧪 Verify: `utoipa` produces valid OpenAPI 3.1.
- 📁 Files: `docs/api/openapi.yaml`.

### Task 13.7 — Release tag

- ✅ Acceptance: `git tag v1.0.0` + GitHub release drafted.
- 🧪 Verify: tag visible; release notes attached.
- 📁 Files: (git).

---

## Progress Tracking

Use a single checkbox per task in a PR description:

```
- [ ] task(0-1): workspace root
- [ ] task(0-2): toolchain pinning
...
```

Each task should be one PR. Larger tasks (e.g. 7.1) may span multiple commits
but one PR.

---

## Done Definition (overall)

Picroom v1.0 is **done** when:

1. All tasks above are checked.
2. `cargo test --workspace` green.
3. `cargo clippy --all-targets -- -D warnings` clean.
4. `cargo fmt --check` clean.
5. `cargo audit` clean.
6. `cargo deny check` clean.
7. `cargo tarpaulin --workspace` ≥ 80 %.
8. `docker compose up` works.
9. `picroom api` serves `/healthz` 200.
10. `picroom admin migrate run` succeeds.
11. `aws s3 cp` round-trip works.
12. README quick-start reproduces on a fresh machine.

---

_End of tasks v1.0 — 100 tasks across 14 phases._