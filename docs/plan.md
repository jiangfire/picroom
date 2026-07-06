# Picroom — Implementation Plan (v1.0)

> **Status**: Draft for review · **Companion to**: `docs/spec.md` · **Last updated**: 2026-07-05

This plan decomposes the spec into reviewable phases. Each phase has a single
clear deliverable and a verification gate. Phases are ordered by dependency:
infrastructure first, then domain, then services, then surfaces (API / S3 /
worker), then end-to-end wiring.

---

## Phase 0 — Repo skeleton & CI (≈ 1 day)

**Deliverable**: a Cargo workspace with empty crates, CI green, dev compose
running PG + MinIO.

**Tasks**:

1. Convert root `Cargo.toml` into workspace with all 11 crates (placeholders).
2. Author `.gitignore`, `.dockerignore`, `rust-toolchain.toml`,
   `.cargo/config.toml`.
3. Author `README.md`, `LICENSE` (MIT), `CONTRIBUTING.md`, `SECURITY.md`.
4. Author `docker/Dockerfile` (multi-stage) + `docker/docker-compose.yml`.
5. Author `.github/workflows/ci.yml` (fmt + clippy + test + audit + deny +
   coverage).
6. Author `deny.toml` and `tarpaulin.toml`.

**Verification**:

- `cargo build --workspace` succeeds.
- `cargo fmt --check` passes.
- `cargo clippy --all-targets -- -D warnings` passes (with empty crates).
- `docker compose -f docker/docker-compose.yml config` validates.
- CI is green on first push.

**Exit criteria**: someone can clone the repo, run `docker compose up`, and
`cargo test` is green with no crates having logic.

---

## Phase 1 — `domain` crate (≈ 2 days)

**Deliverable**: pure-Rust value objects, entities, traits, error type, all
with 100 % test coverage.

**Tasks**:

1. `Image`, `User`, `Team`, `Role`, `Permission`, `StorageKey`, `Page`,
   `Job` entities.
2. `DomainError` enum using `thiserror` (covers all variants per spec §9).
3. `StorageKey::parse` validation rules.
4. `Permission` enum with `check(actor, resource)` pure function.
5. `Clock` trait + `SystemClock` impl.
6. `id` module: `UuidV7` generator.
7. Unit tests for every entity, ≥ 100 % line coverage.

**Verification**:

- `cargo test -p picroom-domain` 100 % green.
- `cargo tarpaulin -p picroom-domain` reports 100 % coverage.
- `cargo clippy -p picroom-domain -- -D warnings` clean.

**Exit criteria**: domain is usable as a library; no I/O anywhere.

---

## Phase 2 — `infra` crate (≈ 2 days)

**Deliverable**: config loader, DB pool, structured logging, error mapping.

**Tasks**:

1. `config::load` with `figment` (env > TOML > default).
2. `db::pool` with `sqlx::PgPool` + `sqlx::SqlitePool` (feature-gated).
3. `logging::init` with `tracing-subscriber` JSON layer.
4. `clock::SystemClock` + `clock::FakeClock` (test-only).
5. `id::generator` (UUID v7).
6. Tests: config precedence; pool health-check; log format (snapshot).

**Verification**:

- `cargo test -p picroom-infra` green.
- `cargo run --bin picroom -- admin config print` works with env + TOML.
- `RUST_LOG=info cargo run -- api` emits JSON logs to stdout.

**Exit criteria**: every other crate can use `infra::config::Config`.

---

## Phase 3 — `storage` crate (≈ 4 days)

**Deliverable**: `Storage` trait + Local + S3 + MinIO drivers, contract tests
green.

**Tasks**:

1. Trait definitions: `StorageReader` / `StorageWriter` / `StorageLister` /
   `StorageSigner` (per spec §12).
2. `AnyStorage` enum + match dispatch.
3. `LocalDriver` (filesystem under configurable root).
4. `S3Driver` (AWS S3 + MinIO via `aws-sdk-s3` or `aws-sigv4` + custom
   dispatch).
5. `OssDriver`, `CosDriver`, `QiniuDriver` (post-v1.0 scope but interface
   ready).
6. Signing module: SigV4 + OSS V1 helpers.
7. Contract test (`contract_test::put_get_delete_roundtrip`).
8. Integration tests with testcontainers + MinIO.

**Verification**:

- `cargo test -p picroom-storage` green.
- `cargo test -p picroom-storage --features integration` exercises real MinIO.
- All drivers pass the contract test.

**Exit criteria**: a binary can upload a file to any of Local / S3 / MinIO
through the same interface.

---

## Phase 4 — `imaging` crate (≈ 3 days)

**Deliverable**: `Processor` trait + probe, resize, AVIF, WebP, thumbnail,
watermark processors + pipeline runner.

**Tasks**:

1. `Processor` trait + `Pipeline` runner.
2. `ProbeProcessor` (read EXIF, dimensions, format).
3. `ResizeProcessor` (configurable max dimension, aspect preservation).
4. `AvifProcessor` via `ravif` (CPU-only).
5. `WebpProcessor` via `image` crate.
6. `ThumbnailProcessor` (multiple sizes, e.g. 200/400/800 px).
7. `WatermarkProcessor` (text + image, position config).
8. Golden tests: encode fixture → compare against checked-in reference.

**Verification**:

- `cargo test -p picroom-imaging` green.
- `cargo bench -p picroom-imaging --no-run` compiles.
- AVIF encode of a 4 MB JPEG ≤ 1.5 s on 4 cores (criterion benchmark).

**Exit criteria**: pipeline can produce AVIF + WebP + 3 thumbnails from any
input.

---

## Phase 5 — `auth` crate (≈ 3 days)

**Deliverable**: password hashing, JWT, OIDC client, API tokens, RBAC engine.

**Tasks**:

1. `password::hash` + `password::verify` using `argon2`.
2. `jwt::issue` + `jwt::verify` using `jsonwebtoken`.
3. `api_token::mint` + `api_token::verify` (DB-backed, hashed).
4. `oidc::discover` + `oidc::callback` (generic OIDC via `openidconnect`
   crate).
5. `rbac::evaluate` (Permission × Role × Resource, with deny-overrides).
6. Session manager: refresh tokens stored in DB.

**Verification**:

- `cargo test -p picroom-auth` green.
- OIDC mock provider test (using `wiremock`).
- RBAC table tests (every role × every action).

**Exit criteria**: a user can log in via password or OIDC and receive a JWT.

---

## Phase 6 — `audit` crate (≈ 1 day)

**Deliverable**: append-only audit log + sink trait + DB-backed sink.

**Tasks**:

1. `Event` struct (timestamp, actor, action, target, ip, ua).
2. `AuditSink` trait + `DbAuditSink` impl.
3. `audit::record(...)` helper used by every state-changing handler.
4. Snapshot tests for event payloads.

**Verification**:

- `cargo test -p picroom-audit` green.
- Audit row inserted on test uploads (verified in integration).

**Exit criteria**: every API call that mutates state writes exactly one audit
event.

---

## Phase 7 — `service` crate (≈ 3 days)

**Deliverable**: use-case layer orchestrating domain + storage + imaging +
auth + audit.

**Tasks**:

1. `UploadService` (validate → probe → persist original → enqueue job).
2. `ImageQueryService` (list / get / variants).
3. `DeleteService` (with permission check + storage cleanup).
4. `QuotaService` (per-user / per-team bytes).
5. `PermissionService` (RBAC evaluation wrapper).
6. Unit tests with `mockall` for all dependencies.
7. Integration tests against real PG.

**Verification**:

- `cargo test -p picroom-service` green.
- `cargo tarpaulin -p picroom-service` ≥ 80 %.

**Exit criteria**: services can be exercised end-to-end without HTTP layer.

---

## Phase 8 — `worker` crate (≈ 2 days)

**Deliverable**: async job consumer with retry + DLQ.

**Tasks**:

1. `Job` enum (EncodeAvif, EncodeWebp, Thumbnail, Watermark).
2. `JobQueue` trait + DB-backed implementation (`SKIP LOCKED` polling).
3. Retry policy (exponential backoff, max 5 retries).
4. DLQ table for poison messages.
5. Concurrent worker pool (configurable).
6. Integration test: enqueue → consume → assert result.

**Verification**:

- `cargo test -p picroom-worker` green.
- Worker survives 1000 jobs under fault injection.

**Exit criteria**: upload → enqueue → worker produces variants within 5 s.

---

## Phase 9 — `api` crate (≈ 3 days)

**Deliverable**: axum HTTP API.

**Tasks**:

1. Router scaffolding (`/api/v1/...`, `/healthz`, `/readyz`, `/metrics`).
2. Auth middleware (JWT extractor + OIDC callback handler).
3. Permission guard middleware.
4. Request ID + tracing span per request.
5. Handlers for: auth, users, teams, images, audit, admin.
6. Multipart upload handler (streaming, `axum::extract::Multipart`).
7. Error mapper (`DomainError` → HTTP status + JSON body).
8. Integration tests with `axum::TestServer` + real DB.

**Verification**:

- `cargo test -p picroom-api` green.
- `cargo run --bin picroom -- api` serves `/healthz` 200 OK.

**Exit criteria**: every endpoint documented in OpenAPI has at least one happy-
path integration test.

---

## Phase 10 — `s3compat` crate (≈ 3 days)

**Deliverable**: AWS S3-compatible endpoint with SigV4.

**Tasks**:

1. SigV4 signing + verification (re-use `aws-sigv4`).
2. Bucket dispatch (path-style).
3. Object handlers: PUT, GET, HEAD, DELETE.
4. Multipart upload (init / part / complete / abort).
5. ListObjectsV2.
6. PicGo compatibility tests.

**Verification**:

- `cargo test -p picroom-s3compat` green.
- `aws s3 cp foo.jpg s3://test/x.jpg --endpoint-url http://localhost:8080/s3`
  succeeds.
- `rclone lsd :s3:test/` lists the bucket.

**Exit criteria**: any SigV4 client can read/write through Picroom.

---

## Phase 11 — `admin` crate (≈ 1 day)

**Deliverable**: CLI subcommands for ops.

**Tasks**:

1. `admin migrate run / revert / status`.
2. `admin user create / list / set-role / disable`.
3. `admin team create / list / add-member`.
4. `admin audit tail --follow`.
5. `admin config print / validate`.
6. `admin storage test --policy <name>` (round-trip).

**Verification**:

- `cargo test -p picroom-admin` green.
- All subcommands reachable via `picroom admin --help`.

**Exit criteria**: first-day ops are CLI-driven; UI is optional.

---

## Phase 12 — `picroom` binary (≈ 1 day)

**Deliverable**: single binary with subcommand dispatch.

**Tasks**:

1. `bin/picroom/src/main.rs` parses argv → dispatches to api / worker / admin.
2. Graceful shutdown (SIGTERM / SIGINT → drain).
3. Startup banner (version, build hash, license).
4. Smoke test script.

**Verification**:

- `cargo build --release --bin picroom` produces binary.
- Binary size ≤ 40 MB.
- `./picroom --version` prints version.
- `./picroom api &` then `curl localhost:8080/healthz` returns 200.

**Exit criteria**: single binary satisfies all three modes.

---

## Phase 13 — Hardening & docs (≈ 3 days)

**Deliverable**: production-ready release.

**Tasks**:

1. Run `cargo audit`, fix or pin all advisories.
2. Run `cargo deny check`, prune any non-MIT dep.
3. Verify coverage ≥ 80 % across workspace.
4. Write `docs/deployment.md`, `docs/operations.md`, `docs/security.md`.
5. Generate `docs/api/openapi.yaml` from code annotations.
6. Final review against `docs/spec.md` acceptance criteria.
7. Tag `v1.0.0`.

**Verification**:

- All CI checks green.
- Spec success criteria §1.4 all satisfied.
- `git tag v1.0.0` + GitHub release drafted.

**Exit criteria**: `picroom v1.0.0` is releasable.

---

## Risk Register

| Risk | Probability | Impact | Mitigation |
|---|---|---|---|
| `ravif` build fails on musl / alpine | M | M | Pin glibc-based runtime image; provide alpine variant |
| AVIF encode too slow | M | H | Background queue, immediate low-quality preview |
| S3 SigV4 spec edge cases | M | M | Use AWS-provided test vectors; contract test against MinIO |
| Cargo workspace compile time | H | M | Shared target dir, incremental builds, `sccache` in CI |
| 80 % coverage gate too aggressive early | M | M | Phase-specific threshold (70 % for Phase 8+, 80 % from Phase 11) |
| OIDC provider quirks | M | L | Test against at least 2 providers (Keycloak + Authentik) |
| `object_store` crate changes API | L | M | Pin version; isolate behind our trait |

---

## Parallelizable Work Streams

After Phase 0 + 1 + 2 (must be sequential):

```
Phase 3 (storage)      ┐
Phase 4 (imaging)      │── all parallel
Phase 5 (auth)         │
Phase 6 (audit)        ┘

Then:

Phase 7 (service) needs 3, 4, 5, 6
Phase 8 (worker)  needs 3, 4, 6
Phase 9 (api)     needs 5, 6, 7
Phase 10 (s3)     needs 3, 5, 6
Phase 11 (admin)   needs 6
```

So: storage/imaging/auth/audit can be developed in parallel by separate
streams; service unlocks everything else.

---

## Estimated Calendar

| Phase | Duration | Cumulative |
|---|---|---|
| 0 | 1 d | 1 d |
| 1 | 2 d | 3 d |
| 2 | 2 d | 5 d |
| 3 | 4 d | 9 d |
| 4 | 3 d | 12 d |
| 5 | 3 d | 15 d |
| 6 | 1 d | 16 d |
| 7 | 3 d | 19 d |
| 8 | 2 d | 21 d |
| 9 | 3 d | 24 d |
| 10 | 3 d | 27 d |
| 11 | 1 d | 28 d |
| 12 | 1 d | 29 d |
| 13 | 3 d | 32 d |

With parallel streams (Phases 3-6), ~ 22 working days.

---

## Verification Checkpoints (review gates)

After each phase, the human must:

1. Run `cargo test --workspace` and confirm green.
2. Review the new code (≥ 200 lines or new public API).
3. Verify success criteria touched by this phase.
4. Approve before moving to next phase.

If a phase's exit criteria are not met, do not proceed.

---

_End of plan v1.0_