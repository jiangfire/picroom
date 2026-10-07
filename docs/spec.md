# Picroom — Specification (v1.0)

> **Status**: Draft for review · **Version**: 1.0.0 · **Last updated**: 2026-07-25

Picroom is a self-hosted image hosting service built for teams. It targets the gap
between consumer-grade PHP scripts (Lsky Pro, EasyImage) and heavyweight photo
platforms (Immich), combining native high performance, modern image formats, and
enterprise-grade permissions in a single MIT-licensed binary.

---

## 1. Objective

### 1.1 What we are building

A self-hostable image bed that:

- Serves as the **upload + transform + distribution** backend for product UGC,
  editorial CMS assets, documentation media, IM attachments, and CI/CD build
  artifacts.
- Runs as a **single Rust binary** with optional PostgreSQL and Redis, scaling
  from a 1-CPU VPS to a horizontally scaled K8s deployment.
- Speaks both a **REST/JSON API** and an **AWS S3-compatible endpoint**, so it
  is usable from PicGo, rclone, AWS CLI, and any tool that speaks SigV4.

### 1.2 Target users

| Persona | Use case |
|---|---|
| Solo developer | Markdown blog assets, 1k images / month |
| Engineering team (10–100) | Documentation media, CI/CD artifacts, internal CDN |
| SMB / startup | Product UGC, marketing CMS, 100k images / month |
| Mid-market | Multi-team media library, audit, RBAC, SSO |
| SaaS platform | Embed Picroom as a microservice behind their app |

### 1.3 Non-goals (v1)

- ❌ Video hosting / transcoding (out of scope; covered by separate products).
- ❌ AI / ML features (face recognition, object detection) — Immich's territory.
- ❌ Social-network style gallery / comments / likes.
- ❌ Photo timeline / map view / album browsing UI.
- ❌ Mobile apps (web UI is responsive; native apps are post-v1).
- ❌ End-user public sharing with social sign-in (post-v1).

### 1.4 Success criteria

Picroom v1.0 is considered **done** when **all** of the following hold:

| # | Criterion | Measurement |
|:-:|---|---|
| S1 | Single `picroom` binary < 40 MB (release, stripped) | `ls -l target/release/picroom` |
| S2 | Cold start < 500 ms to first byte | `time curl http://localhost:8080/healthz` |
| S3 | Upload throughput ≥ 200 MB/s on a 4-core / 8 GB box (single client, multipart) | `wrk` + `picroom-bench` |
| S4 | AVIF encode for a 4 MB JPEG ≤ 1.5 s on 4 cores | `picroom-bench image encode` |
| S5 | All unit + integration tests pass with ≥ 80 % line coverage | `cargo tarpaulin` |
| S6 | `cargo clippy --all-targets -- -D warnings` clean | CI |
| S7 | `cargo fmt --check` clean | CI |
| S8 | `cargo audit` clean | CI |
| S9 | `cargo deny check` (MIT-only deps) clean | CI |
| S10 | `docker compose up` brings up API + worker + PostgreSQL + MinIO in one command | Manual |
| S11 | `aws s3 cp foo.jpg s3://picroom-test/ --endpoint-url http://localhost:9000` works | Manual |
| S12 | PicGo can upload through the S3 endpoint | Manual |
| S13 | OIDC login (Authentik / Keycloak) succeeds and creates a session | Manual |
| S14 | Audit log records every auth, upload, delete, role change | Manual + test |
| S15 | License headers in every source file declare MIT | `reuse lint` |

---

## 2. Tech Stack

### 2.1 Languages & runtimes

| Layer | Choice | Version | Rationale |
|---|---|---|---|
| Backend | Rust | 1.79+ stable | Single binary, memory safety, async ecosystem |
| Async runtime | Tokio | 1.x | De facto Rust async runtime |
| HTTP framework | axum | 0.7 | Tower ecosystem, ergonomic, performant |
| DB driver | sqlx | 0.8 | Compile-time checked queries, async |
| Frontend | Vue 3 + Vite + TypeScript | 3.4+ / 5.x | Shipped as the Tauri admin client in `desktop/` (see `spec-admin-client.md`) |
| SQL | PostgreSQL | 16 | JSONB, RLS, generated columns, mature |
| Embedded SQL fallback | SQLite | 3.45+ | Zero-ops single-user mode |
| Object storage (dev) | MinIO | latest | S3-compatible, easy to test against |
| Image processing | `image` + `ravif` (AVIF) + `rgb` | 0.25 / 0.11 / 0.8 | Pure-Rust probe/resize/WebP + safe AVIF encoder (no libvips/cgo) |

### 2.2 Crate dependencies (locked to minor)

| Crate | Purpose |
|---|---|
| `tokio` | Async runtime |
| `axum`, `tower`, `tower-http` | HTTP server / middleware |
| `serde`, `serde_json` | (De)serialization |
| `sqlx` | DB driver w/ compile-time query checking |
| `tracing`, `tracing-subscriber` | Structured logging |
| `tracing-opentelemetry`, `opentelemetry` | Distributed tracing (post-MVP hook) |
| `prometheus` or `metrics-exporter-prometheus` | Metrics |
| `thiserror`, `anyhow` | Error handling |
| `figment` | Config loading (env > TOML > default) |
| `uuid` v7 | IDs |
| `time` or `chrono` | Timestamps |
| `jsonwebtoken` | JWT (HS/RS/ES) |
| `reqwest` | Outbound HTTP (OIDC, webhooks) |
| `quick-xml` + in-house SigV4 (`crates/s3compat`) | AWS SigV4 signing/verification (the `aws-sigv4` crates were declared but never imported and have been removed; SigV4 is implemented in-house) |
| `ravif` | AVIF encoder |
| `image` | Probe, resize, WebP, EXIF |
| `rgb` | Pixel buffer bridge to `ravif` |
| `mockall` | Mocking traits in tests |
| `proptest` | Property-based tests |
| `criterion` | Benchmarks |
| `wiremock` or `mockito` | HTTP mocking |
| `testcontainers` | E2E with real PG / MinIO |
| `rstest` | Parameterized tests |
| `insta` | Snapshot tests for OpenAPI / config |

### 2.3 Tooling

| Tool | Purpose |
|---|---|
| `cargo` | Build / test / lint |
| `rustfmt` | Formatting |
| `clippy` | Lints |
| `cargo-audit` | Vulnerability scanning |
| `cargo-deny` | License + advisory policy |
| `cargo-tarpaulin` | Code coverage |
| `cargo-mutants` | Mutation testing (post-MVP) |
| `sqlx-cli` | Migrations |
| `docker`, `docker compose` | Container build / local dev |
| `pre-commit` | Local hook (optional) |

---

## 3. Commands

All commands assume repo root.

### 3.1 Development

```bash
# Toolchain
rustup toolchain install stable
rustup component add rustfmt clippy rust-analyzer

# Run database (dev)
docker compose up -d postgres minio

# Run migrations
cargo run --bin picroom -- admin migrate run --config ./config/example.toml

# Build everything (debug)
cargo build --workspace

# Run API (dev mode)
cargo run --bin picroom -- api --config ./config/example.toml

# Run worker (dev mode)
cargo run --bin picroom -- worker --config ./config/example.toml

# Admin commands (also: `admin migrate status`, `admin config validate`,
# `admin storage-test --policy default`, `admin audit tail --follow`)
cargo run --bin picroom -- admin migrate run
cargo run --bin picroom -- admin user create --email admin@example.com --role admin
cargo run --bin picroom -- admin audit tail --follow

# Tests
cargo test --workspace                                 # all unit + integration
cargo test --doc                                       # doctests
cargo bench --no-run                                   # compile-check benchmarks (none wired yet)

# Lints / format
cargo fmt --all
cargo fmt --all -- --check
cargo clippy --all-targets --all-features -- -D warnings

# Coverage
cargo tarpaulin --workspace --out Html --output-dir target/coverage
```

### 3.2 Release

```bash
# Build release binary
cargo build --release --bin picroom

# Build Docker image (multi-stage)
docker build -t picroom:1.0.0 -f docker/Dockerfile .

# Build docker-compose bundle
docker compose -f docker/docker-compose.yml build

# Tag + push
docker tag picroom:1.0.0 ghcr.io/picroom/picroom:1.0.0
docker push ghcr.io/picroom/picroom:1.0.0

# Run release locally
docker compose -f docker/docker-compose.yml up -d
```

### 3.3 CI (must all be green to merge)

```bash
cargo fmt --all -- --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test --workspace
cargo test --doc
cargo audit
cargo deny check
cargo tarpaulin --workspace --fail-under 80
```

---

## 4. Project Structure

```
picroom/
├── Cargo.toml                       # workspace root
├── Cargo.lock                       # committed
├── rust-toolchain.toml              # pinned toolchain (1.79)
├── deny.toml                        # cargo-deny: license + advisory policy
├── tarpaulin.toml                   # coverage config
├── .cargo/
│   └── config.toml                  # build settings, target-dir
├── .github/
│   └── workflows/
│       ├── ci.yml                   # quality + test + coverage + audit
│       └── release.yml              # tag-driven release + image push
├── docker/
│   ├── Dockerfile                   # multi-stage build
│   ├── docker-compose.yml           # dev / demo stack (pg, minio, mailhog, migrate, api, worker)
│   └── config.example.toml          # compose-mounted config
├── desktop/                         # Tauri 2 admin client (standalone, NOT a workspace member — see spec-admin-client.md)
│   ├── package.json
│   ├── vite.config.ts
│   ├── tsconfig.json
│   ├── src/                         # Vue 3 + Pinia + Naive UI frontend
│   └── src-tauri/                   # Rust command layer (own Cargo project)
├── crates/
│   ├── api/                         # axum routes, handlers, middleware
│   │   ├── Cargo.toml
│   │   ├── src/
│   │   │   ├── lib.rs
│   │   │   ├── router.rs
│   │   │   ├── state.rs             # AppState (composition root for handlers)
│   │   │   ├── error.rs
│   │   │   ├── extractors/          # auth (AuthUser, require_auth)
│   │   │   ├── handlers/            # admin, auth, images, public, storage, system, teams
│   │   │   └── middleware/          # auth, trace
│   │   └── tests/
│   ├── service/                     # use cases
│   │   ├── Cargo.toml
│   │   └── src/
│   │       ├── lib.rs
│   │       ├── error.rs
│   │       ├── repo.rs              # repository traits (Image/User/Team)
│   │       ├── upload.rs
│   │       ├── query.rs
│   │       ├── delete.rs
│   │       ├── quota.rs
│   │       └── permission.rs
│   ├── domain/                      # entities, value objects, traits, errors
│   │   ├── Cargo.toml
│   │   └── src/
│   │       ├── lib.rs
│   │       ├── image.rs
│   │       ├── user.rs
│   │       ├── team.rs
│   │       ├── role.rs
│   │       ├── permission.rs
│   │       ├── storage_key.rs
│   │       ├── page.rs
│   │       ├── clock.rs
│   │       ├── id.rs
│   │       └── error.rs
│   ├── storage/                     # Storage trait + drivers
│   │   ├── Cargo.toml
│   │   ├── src/
│   │   │   ├── lib.rs
│   │   │   ├── any.rs               # AnyStorage enum (match-dispatch)
│   │   │   ├── driver/
│   │   │   │   ├── mod.rs           # StorageReader/Writer/Lister/Signer + Storage supertrait
│   │   │   │   ├── local.rs
│   │   │   │   ├── s3.rs
│   │   │   │   └── minio.rs         # (oss/cos/qiniu are planned, not yet implemented)
│   │   │   ├── signing.rs
│   │   │   ├── contract_test.rs
│   │   │   └── error.rs
│   │   └── tests/
│   ├── imaging/                     # Processor trait + pipeline
│   │   ├── Cargo.toml
│   │   └── src/
│   │       ├── lib.rs
│   │       └── processor/
│   │           ├── mod.rs
│   │           ├── probe.rs
│   │           ├── resize.rs
│   │           ├── avif.rs
│   │           ├── webp.rs
│   │           ├── thumbnail.rs
│   │           └── watermark.rs
│   ├── auth/                        # RBAC, JWT, OIDC, API token
│   │   ├── Cargo.toml
│   │   └── src/
│   │       ├── lib.rs
│   │       ├── jwt.rs
│   │       ├── oidc.rs
│   │       ├── password.rs          # Argon2id
│   │       ├── api_token.rs
│   │       └── rbac.rs
│   ├── audit/                       # audit log
│   │   ├── Cargo.toml
│   │   └── src/
│   │       ├── lib.rs
│   │       ├── event.rs
│   │       ├── sink.rs              # AuditSink trait + NoopAuditSink
│   │       ├── db_sink.rs           # DbAuditSink (PostgreSQL)
│   │       └── reader.rs
│   ├── s3compat/                    # AWS S3-compatible endpoint
│   │   ├── Cargo.toml
│   │   └── src/
│   │       ├── lib.rs
│   │       ├── sigv4.rs             # in-house SigV4 verification (constant-time compare)
│   │       ├── routes.rs
│   │       ├── bucket.rs
│   │       ├── object.rs
│   │       ├── list.rs              # ListObjectsV2
│   │       ├── multipart.rs         # stubbed — returns 501 NotImplemented (post-MVP)
│   │       ├── middleware.rs
│   │       └── error.rs
│   ├── worker/                      # async job consumer
│   │   ├── Cargo.toml
│   │   └── src/
│   │       ├── lib.rs
│   │       ├── job.rs
│   │       ├── db_queue.rs          # SELECT ... FOR UPDATE SKIP LOCKED
│   │       ├── pool.rs              # worker pool + retry sleep (implemented 2026-07)
│   │       ├── processor.rs
│   │       ├── retry.rs
│   │       └── dlq.rs
│   ├── infra/                       # db, cache, config, logging, telemetry
│   │   ├── Cargo.toml
│   │   └── src/
│   │       ├── lib.rs
│   │       ├── db.rs                # Database enum (Postgres | Sqlite)
│   │       ├── cache.rs
│   │       ├── config.rs            # figment: env > TOML > defaults
│   │       ├── clock.rs
│   │       ├── id.rs
│   │       ├── logging.rs
│   │       └── telemetry.rs         # metrics-exporter-prometheus
│   └── admin/                       # CLI subcommands (used by bin/picroom)
│       ├── Cargo.toml
│       └── src/
│           ├── lib.rs
│           ├── migrate.rs           # run + status (revert unsupported)
│           ├── user.rs
│           ├── team.rs
│           ├── audit_cmd.rs
│           ├── config_cmd.rs
│           └── storage_test.rs
├── bin/
│   └── picroom/                     # single binary entry point (clap)
│       ├── Cargo.toml
│       └── src/
│           ├── main.rs              # api | worker | admin | version
│           ├── api_cmd.rs
│           ├── worker_cmd.rs
│           ├── app.rs               # AppState + storage construction
│           ├── banner.rs
│           └── shutdown.rs          # SIGTERM/SIGINT graceful drain
├── desktop/                         # (described above)
├── migrations/                      # sqlx migrations (embedded via sqlx::migrate!)
│   ├── 0001_init.sql
│   ├── 0002_storage_and_images.sql
│   ├── 0003_audit_jobs_tokens.sql
│   ├── 0004_sessions_and_oidc.sql
│   ├── 0005_sqlite_init.sql
│   ├── 0006_seed_default_storage_policy.sql
│   ├── 0007_quotas.sql
│   └── 0008_oidc_password_nullable.sql
├── tests/                           # shared test fixtures (E2E suite not yet wired)
│   └── fixtures/images/sample.png
├── benches/                         # reserved for criterion benchmarks (currently empty)
├── config/
│   └── example.toml                 # reference config (no dev.toml — use example.toml)
├── scripts/                         # reserved for helper scripts (currently empty)
├── data/                            # LocalDriver runtime data (gitignored in prod)
├── docs/
│   ├── spec.md                      # this file
│   ├── spec-admin-client.md         # Tauri admin client spec
│   ├── coverage-plan.md             # interim coverage floor + remediation
│   ├── deployment.md
│   ├── operations.md
│   ├── security.md
│   ├── adr/                         # architecture decision records
│   │   ├── 0001-rust-and-axum.md
│   │   ├── 0002-cargo-workspace.md
│   │   ├── 0003-storage-trait-isp.md
│   │   ├── 0004-s3-compatibility.md
│   │   ├── 0005-rbac-model.md
│   │   ├── 0006-image-pipeline.md
│   │   ├── 0007-security-hardening.md
│   │   └── 0008-tauri-admin-client.md
│   └── api/
│       └── openapi.yaml             # hand-authored (no utoipa/codegen)
├── .gitignore
├── .dockerignore
├── README.md
├── CHANGELOG.md
├── LICENSE                          # MIT
├── CONTRIBUTING.md
└── SECURITY.md
```

### 4.1 Dependency rules

The actual internal dependency graph (verified from each crate's `Cargo.toml`):

```
domain      ← (depends on nothing except std + thiserror + optional serde)
storage     ← domain
imaging     ← domain
auth        ← domain
audit       ← domain
infra       ← domain
worker      ← domain, storage, imaging, audit, infra
service     ← domain, storage, imaging, auth, audit, worker
s3compat    ← domain, storage, auth, audit, service
admin       ← domain, infra, storage, auth, audit
api         ← domain, service, auth, audit, infra, storage, s3compat, worker
picroom     ← all eleven internal crates (composition root)
```

Notes / deviations from the original plan:

- `service` depends on `worker` (it composes the worker's job-enqueue surface
  into its upload use case). It deliberately does **not** depend on `infra` —
  the service layer reaches persistence through repository traits + `sqlx`
  rather than through `infra`. This is an accepted refinement of the v1.0
  layering; the earlier "service must not depend on worker" rule is no longer
  in force.
- `admin` depends on `storage`/`auth`/`audit` (not just `domain` + `infra`) so
  the CLI can run `storage-test`, role management, and audit tail.
- `sqlx` is depended on directly by `service`, `audit`, `worker`, and `admin`.
  The intended long-term shape is to funnel DB access through `infra`'s ports;
  for v1 this leakage is accepted and tracked.

Still forbidden:

- `domain` depending on anything except `std`, `thiserror`, optional serde.
- `service` depending on `api`.
- `storage` driver depending on `api`.
- Any circular dependency between crates.

---

## 5. Code Style

### 5.1 Tooling

- `cargo fmt` defaults.
- `clippy::all`, `clippy::pedantic`, `clippy::nursery` with project-specific allow list.
- `cargo deny` rejects any non-MIT dependency unless explicitly waived in `deny.toml`.

### 5.2 Lint policy

```toml
# clippy.toml
avoid-breaking-exported-api = false
cognitive-complexity-threshold = 25

# Cargo.toml (workspace)
[lints.clippy]
all = { level = "warn", priority = -1 }
pedantic = { level = "warn", priority = -1 }
nursery = { level = "warn", priority = -1 }

# Allow list (project-specific)
module_name_repetitions = "allow"
must_use_candidate = "allow"
missing_errors_doc = "allow"
missing_panics_doc = "allow"
```

### 5.3 Conventions

- Naming: `snake_case` for functions/variables, `PascalCase` for types/traits,
  `SCREAMING_SNAKE_CASE` for consts, lowercase module names.
- Errors: every public function returns `Result<T, Error>`; no `unwrap()` in
  non-test code.
- Async: use `tokio` runtime; no `async_std` or `smol`.
- Types: prefer `&str` over `String`, `Cow<'_, str>` only at API boundaries.
- Collections: prefer `Vec<T>` over `Vec<Box<T>>`; use `SmallVec` only when
  profiled.
- Concurrency: `tokio::sync::Mutex` for async, `std::sync::Mutex` for short
  sync critical sections.

### 5.4 Example

```rust
//! Image entity — central domain type.

use crate::error::DomainError;
use crate::storage_key::StorageKey;
use crate::user::UserId;
use serde::{Deserialize, Serialize};
use time::OffsetDateTime;
use uuid::Uuid;

/// A single image record stored in Picroom.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Image {
    pub id: Uuid,
    pub owner: UserId,
    pub key: StorageKey,
    pub content_type: String,
    pub bytes: u64,
    pub width: u32,
    pub height: u32,
    pub created_at: OffsetDateTime,
}

impl Image {
    /// Aspect ratio as a float. Returns `None` if height is zero.
    pub fn aspect_ratio(&self) -> Option<f32> {
        if self.height == 0 {
            None
        } else {
            Some(self.width as f32 / self.height as f32)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn aspect_ratio_returns_none_when_height_is_zero() {
        let img = Image {
            id: Uuid::nil(),
            owner: UserId::nil(),
            key: StorageKey::parse("test/x.jpg").unwrap(),
            content_type: "image/jpeg".into(),
            bytes: 1,
            width: 100,
            height: 0,
            created_at: OffsetDateTime::UNIX_EPOCH,
        };
        assert_eq!(img.aspect_ratio(), None);
    }

    #[test]
    fn aspect_ratio_computes_width_over_height() {
        let img = Image {
            id: Uuid::nil(),
            owner: UserId::nil(),
            key: StorageKey::parse("test/x.jpg").unwrap(),
            content_type: "image/jpeg".into(),
            bytes: 1,
            width: 1920,
            height: 1080,
            created_at: OffsetDateTime::UNIX_EPOCH,
        };
        assert_eq!(img.aspect_ratio(), Some(1920.0 / 1080.0));
    }
}
```

---

## 6. Testing Strategy

### 6.1 Test pyramid

| Level | Coverage target | Tooling |
|---|---|---|
| Unit | 70 % of total tests, ≥ 85 % line coverage per crate | `cargo test`, `mockall`, `proptest` |
| Integration | 20 % | `cargo test` (each crate's `tests/`), real PG / MinIO |
| E2E | 10 % | `tests/*.rs`, `testcontainers`, `reqwest` |

### 6.2 Coverage thresholds

- Per crate: ≥ 80 % lines, ≥ 70 % branches.
- Domain crate: 100 % required (it's pure logic).
- Storage drivers: ≥ 80 %; mandatory contract-test pass.

### 6.3 Test locations

- Unit tests: in `mod tests` at the bottom of each file.
- Integration tests: `<crate>/tests/*.rs` (e.g. `crates/worker/tests/db_queue.rs`,
  `crates/api/tests/api.rs`).
- E2E tests: a top-level `tests/` tree is reserved (currently holds only
  `fixtures/`); the testcontainers-driven E2E suite is a tracked follow-up, not
  yet wired — do not gate on `--features e2e`.
- Benchmarks: `benches/*.rs` — directory reserved; no criterion targets wired yet.

### 6.4 Required test types

For every public trait implementation:

| Trait | Required test |
|---|---|
| `Storage` (any driver) | contract test (put/get/delete/roundtrip) |
| `Processor` (any) | golden test against reference output |
| `AuthProvider` | valid + expired + forged tokens |
| `AuditSink` | event ordering + idempotency |
| Repository (sqlx) | round-trip + unique constraint + index usage |

### 6.5 Contract test pattern (Storage)

```rust
// crates/storage/tests/contract.rs
#[async_trait]
async fn contract_put_get_delete<D: Storage>(driver: &D) {
    let key = StorageKey::parse("test/roundtrip.bin").unwrap();
    let payload = Bytes::from_static(b"hello world");
    driver.put(&key, payload.clone()).await.unwrap();
    let got = driver.get(&key).await.unwrap();
    assert_eq!(got, payload);
    driver.delete(&key).await.unwrap();
    assert!(matches!(
        driver.get(&key).await,
        Err(StorageError::NotFound)
    ));
}
```

Every driver test invokes `contract_put_get_delete(&self_driver)`.

### 6.6 E2E environment

- `testcontainers` spins up PostgreSQL 16 + MinIO.
- Bind to ephemeral ports, isolated network.
- Tests must clean up after themselves.

### 6.7 Performance / load testing

- `criterion` benchmarks for hot paths.
- `wrk` + `picroom-bench` for upload throughput.
- Target thresholds: see Success criteria §1.4.

### 6.8 Snapshot tests

- OpenAPI spec (golden file in `docs/api/openapi.yaml`).
- Example config files.
- Audit event payloads.

---

## 7. Boundaries

### 7.1 Always do

- Run `cargo fmt`, `cargo clippy`, `cargo test` before committing.
- Use `Result` everywhere; never `unwrap()` outside tests.
- Add or update tests alongside any behavior change.
- Update `docs/spec.md` before implementing a spec-changing feature.
- Add an ADR when introducing or replacing a major dependency, a new
  abstraction, or a non-obvious design choice.
- Reference the spec section / ADR in every PR description.
- Pin every dependency to a major.minor (or exact) version in `Cargo.toml`.

### 7.2 Ask first (require explicit approval)

- Adding a new crate to the workspace.
- Changing the database schema in a backwards-incompatible way.
- Changing the public API surface (`/api/v1/*`).
- Changing the storage driver trait surface.
- Changing RBAC semantics or role hierarchy.
- Adding a non-MIT dependency.
- Changing CI configuration.
- Modifying the Dockerfile or Helm chart.

### 7.3 Never do

- Commit secrets, API keys, or tokens.
- Bypass CI checks (`--no-verify`, force-push to protected branches).
- Edit `Cargo.lock` by hand (use `cargo add` / `cargo update`).
- Disable a failing test without an issue explaining why.
- Re-license code away from MIT.
- Use `unwrap()` in non-test code.
- Block the async runtime on synchronous I/O.
- Introduce a circular dependency between crates.

---

## 8. API Contract (high-level)

Full OpenAPI document lives at `docs/api/openapi.yaml`. Key endpoints:

### 8.1 REST API (`/api/v1/`)

```
# System (public)
GET    /healthz                                 # liveness
GET    /readyz                                  # readiness (DB + storage probes)
GET    /metrics                                 # Prometheus

# Public image bytes ("公链" — unauthenticated; see ADR-0008)
GET    /i/*key                                  # serve raw object bytes (Content-Type sniffed)

# Auth (public)
POST   /api/v1/auth/login                       # password login
POST   /api/v1/auth/logout
GET    /api/v1/auth/oidc/:provider/login         # begin OIDC login (redirect to IdP)
GET    /api/v1/auth/oidc/:provider/callback      # OIDC callback (issues JWT)

# Images (auth required; RBAC-enforced)
GET    /api/v1/images                            # list images (filter, page)
POST   /api/v1/images                            # upload (multipart)
GET    /api/v1/images/:id
GET    /api/v1/images/:id/link                   # absolute public / presigned URL
GET    /api/v1/images/:id/file                   # redirect to public / signed URL
DELETE /api/v1/images/:id

# Teams (auth required)
POST   /api/v1/teams                             # create team
GET    /api/v1/teams                             # list teams
GET    /api/v1/teams/:id
POST   /api/v1/teams/:id/members
GET    /api/v1/teams/:id/members                 # list members

# Admin (auth required; admin role)
POST   /api/v1/admin/users                       # create user
GET    /api/v1/admin/users                       # list users (paginated)
PATCH  /api/v1/admin/users/:id/role
POST   /api/v1/admin/users/:id/disable
POST   /api/v1/admin/users/:id/enable
GET    /api/v1/audit                             # audit log
GET    /api/v1/admin/storage/policies            # list storage policies
POST   /api/v1/admin/storage/policies            # create storage policy
```

> Variant endpoints (`/avif`, `/webp`, `/thumbnail`) are **not** exposed as
> separate routes; variants are served through the same unauthenticated
> `GET /i/<variant-key>` route (see ADR-0008). A `GET /api/v1/me` endpoint is
> not implemented.

### 8.2 S3-compatible API (`/s3/`)

```
PUT    /s3/:bucket/:key
GET    /s3/:bucket/:key
HEAD   /s3/:bucket/:key
DELETE /s3/:bucket/:key
POST   /s3/:bucket/:key?uploads                 # multipart init      → 501 (stubbed)
PUT    /s3/:bucket/:key?partNumber=N&uploadId=U # multipart part      → 501 (stubbed)
POST   /s3/:bucket/:key?uploadId=U              # multipart complete  → 501 (stubbed)
DELETE /s3/:bucket/:key?uploadId=U              # multipart abort     → 501 (stubbed)
GET    /s3/:bucket                               # list (v2)
```

SigV4 signing; path-style addressing. Multipart handlers exist but return an
explicit `501 NotImplemented` XML error so well-behaved clients fall back to a
single `PUT` rather than silently losing data (post-MVP, see ADR-0004).

### 8.3 Health and metrics

```
GET    /healthz                                  # liveness
GET    /readyz                                   # readiness (DB, storage)
GET    /metrics                                  # Prometheus
```

---

## 9. Data Model (high-level)

See `migrations/*.sql` for exact DDL.

| Table | Purpose |
|---|---|
| `users` | account identity (email, name, password_hash) |
| `teams` | tenancy container |
| `team_members` | user ↔ team with role |
| `roles` | role definition per team |
| `permissions` | role × action × resource_type |
| `storage_policies` | named storage configs (local / S3; MinIO via S3 driver). OSS / COS / Qiniu planned but not implemented (ADR-0003) |
| `images` | image metadata, owner, key, dims, hashes |
| `image_variants` | derived variants (avif, webp, thumb) |
| `api_tokens` | long-lived bearer tokens for scripts |
| `audit_events` | append-only audit log |
| `jobs` | async job queue (encode, thumbnail, replicate) |
| `quotas` | per-user / per-team storage and bandwidth caps |

---

## 10. RBAC Model

### 10.1 Roles (built-in)

| Role | Permissions |
|---|---|
| `viewer` | `image.read` |
| `uploader` | `image.read`, `image.create` |
| `manager` | all `image.*`, `team.read`, `team.invite` |
| `admin` | everything + `user.*`, `audit.read`, `system.*` |

Custom roles can be created per-team with arbitrary permission sets.

### 10.2 Resources

| Resource | Scope |
|---|---|
| `image` | `personal` (owner-only) or `team` (shared) |
| `team` | the team itself |
| `user` | system-wide |
| `audit` | system-wide |
| `storage_policy` | system-wide |

### 10.3 Evaluation order

1. Explicit deny rule (highest priority — **overrides the `admin` role**).
2. Resource ownership (`owner_id` match).
3. Team membership role (for the resource's team scope).
4. Resource-level ACL allow (e.g., shared with a specific user).
5. Global role default permissions.
6. Default deny.

Global `uploader`/`manager` roles do not apply inside another team's scope;
team-shared images require team membership (or an ACL grant, or admin).
Enforcement lives in the service layer (`UploadService`, `DeleteService`);
route handlers authenticate only. ACL management endpoints exist for images
(`GET/PUT /api/v1/images/{id}/acl`, `DELETE .../acl/{subjectType}/{subjectId}`),
replace-semantics on `PUT`; the `resource_acls` table carries an `effect`
column (`allow`/`deny`) and is resource-agnostic.

---

## 11. Image Processing Pipeline

```
Upload → Validate → Probe → Persist (original) → Enqueue job
                                                  ↓
                                    Worker picks up job
                                                  ↓
                              ┌───────────────────┴───────────────────┐
                              ▼                                       ▼
                        AVIF encode                             WebP encode
                              │                                       │
                              └───────────────────┬───────────────────┘
                                                  ▼
                                         Generate thumbnail
                                                  ▼
                                  Persist variants to storage
                                                  ▼
                                Update image_variants table
                                                  ▼
                                   Emit audit event
```

Pipeline is configurable:

```toml
[pipeline]
encode_avif = true
encode_webp = true
generate_thumbnail = true
strip_exif = true
max_dimension = 8192

[pipeline.quality]
avif = 60   # applied to the AVIF encoder (0-100)
webp = 80   # reserved: WebP encoding is lossless-only (image crate), no effect
jpeg = 85   # applied to generated thumbnails (1-100)
```

- `max_dimension` bounds every generated variant (aspect-preserving downscale).
- Generated variants are re-encoded from decoded pixels and never carry EXIF;
  `strip_exif` therefore applies to variants. **Originals are stored
  byte-exact** — stripping EXIF from uploaded originals is not implemented.
- Variant jobs are enqueued only after the `images` row commits; jobs claim a
  lease and dead-lease rows are reclaimed by live workers.

---

## 12. Storage Abstraction

```rust
// crates/storage/src/driver/mod.rs

#[async_trait::async_trait]
pub trait StorageReader: Send + Sync {
    async fn get(&self, key: &StorageKey) -> Result<Bytes, StorageError>;
    async fn head(&self, key: &StorageKey) -> Result<ObjectMeta, StorageError>;
    async fn exists(&self, key: &StorageKey) -> Result<bool, StorageError>;
}

#[async_trait::async_trait]
pub trait StorageWriter: Send + Sync {
    async fn put(&self, key: &StorageKey, bytes: Bytes) -> Result<(), StorageError>;
    async fn delete(&self, key: &StorageKey) -> Result<(), StorageError>;
}

#[async_trait::async_trait]
pub trait StorageLister: Send + Sync {
    async fn list(&self, prefix: Option<&StorageKey>) -> Result<Page<ObjectMeta>, StorageError>;
}

#[async_trait::async_trait]
pub trait StorageSigner: Send + Sync {
    async fn sign_get_url(&self, key: &StorageKey, ttl: Duration) -> Result<Url, StorageError>;
    async fn sign_put_url(&self, key: &StorageKey, ttl: Duration) -> Result<Url, StorageError>;
}

pub trait Storage: StorageReader + StorageWriter + StorageLister + StorageSigner {}

pub enum AnyStorage {
    Local(LocalDriver),
    S3(S3Driver),
    Minio(MinioDriver),
}

impl Storage for AnyStorage { /* dispatch via match */ }
```

> `Oss` / `Cos` / `Qiniu` variants are planned (ADR-0003) but not yet
> implemented; only `Local`, `S3`, and `Minio` ship today. `MinioDriver` is a
> thin specialization of the S3 path-style client.

---

## 13. Deployment

### 13.1 Minimal (single host)

```bash
docker compose -f docker/docker-compose.yml up -d
```

Brings up: API, worker, PostgreSQL, MinIO. Single port (8080) exposed.

### 13.2 Production (K8s, post-MVP)

- Deployment × 3 replicas for `picroom-api`.
- Deployment × 2 replicas for `picroom-worker`.
- Managed PostgreSQL (or self-hosted with HA).
- S3-compatible object storage (AWS S3, MinIO, or any SigV4 endpoint).
- Redis (optional) for caching.
- Ingress (nginx / Traefik) with TLS termination.

### 13.3 Configuration

Loaded from environment variables (prefix `PICROOM_`, `__` separates nesting)
with optional TOML override. A commented reference file lives at
`config/example.toml`; the salient sections:

```toml
[server]
bind_addr = "0.0.0.0:8080"
request_timeout_secs = 30
graceful_shutdown_secs = 30
max_body_mb = 100
# public_url_base = "https://cdn.example.com"   # absolute base for /link & /file (ADR-0008)

[database]
url = "postgres://picroom:secret@localhost/picroom"
max_connections = 20
min_connections = 2

[storage]
default = "primary"

[storage.policies.primary]
driver = "s3"                                   # local | s3 | minio
bucket = "picroom-prod"
endpoint = "https://s3.amazonaws.com"
region = "us-east-1"
access_key_id = "${AWS_ACCESS_KEY_ID}"
secret_access_key = "${AWS_SECRET_ACCESS_KEY}"

[pipeline]
encode_avif = true
encode_webp = true
generate_thumbnail = true
strip_exif = true
max_dimension = 8192

[auth]
allow_signup = false
password_min_length = 12
jwt_secret = "change-me"                        # MUST be overridden in prod (release builds refuse to start)
jwt_issuer = "picroom"
jwt_audience = "picroom-api"
jwt_ttl_secs = 3600

[auth.oidc]
admin_emails = ["admin@example.com"]            # emails promoted to `admin` on first OIDC login
secure_cookies = true                           # set false only for local HTTP dev (no TLS)

[auth.oidc.providers.google]
issuer = "https://accounts.google.com"
client_id = "${OIDC_GOOGLE_CLIENT_ID}"
client_secret = "${OIDC_GOOGLE_CLIENT_SECRET}"
redirect_uri = "https://picroom.example.com/api/v1/auth/oidc/google/callback"
scopes = ["openid", "email", "profile"]
# admin_emails = ["admin@example.com"]   # emails promoted to `admin` on first OIDC login

[quota]
default_user_bytes = 10737418240                # 10 GiB
default_team_bytes = 1099511627776              # 1 TiB
soft_limit_warning = 0.9
hard_limit_enforce = true

[rate_limit]
per_user_rps = 10
per_user_burst = 20
per_ip_rps = 50
per_ip_burst = 100

[audit]
retention_days = 365

[logging]
level = "info"
format = "json"                                 # "json" or "pretty"

[telemetry]
metrics_enabled = true
# otlp_endpoint = "http://localhost:4317"
```

Two additional env vars govern the S3-compat surface: `PICROOM_S3_ACCESS_KEY_ID`
and `PICROOM_S3_SECRET_ACCESS_KEY` — when both are set, every `/s3/*` request is
run through SigV4 verification; otherwise the endpoint is open (dev only).

Environment variables win over TOML; TOML wins over defaults.

---

## 14. Open Questions

Items that remain unresolved and require decision before implementation:

1. **Frontend deployment**: bundle into binary via `include_str!` + axum
   static handler, or separate SPA served by nginx? **Recommended**: include
   in binary for single-binary deployment. **Resolved (post-MVP, 2026-07-12)**:
   the public-link surface (`GET /i/:key`, `GET /api/v1/images/:id/link`,
   `GET /api/v1/images/:id/file`) replaces any bundled SPA; the admin GUI
   lives in the standalone Tauri 2 client under `desktop/` (see §17 and
   [`docs/spec-admin-client.md`](spec-admin-client.md)).
2. **Image variant storage path layout**: by-image-id (`/img/<id>/avif`) or by
   hash (`/img/<sha256[:2]>/<sha256>.avif`)? **Recommended**: by ID for human
   debugging, hash for deduplication (post-MVP).
3. **Default DB**: ship `sqlite` mode by default, or always require PostgreSQL?
   **Recommended**: dual-mode with env switch. **Resolved (2026-07-11)**:
   `Database` enum (`crates/infra/src/db.rs`) selects Postgres or SQLite
   from `database.url`; both backends are wired through `admin migrate run`
   and `admin user …`.
4. **Quota enforcement**: hard cap (reject) vs. soft cap (allow + warn)?
   **Recommended**: hard cap by default, soft cap configurable.
   **Resolved (2026-07-25)**: `QuotaService` rejects uploads when
   `quotas.max_bytes - SUM(bytes) < payload` (PG path); `soft_limit_warning`
   and `hard_limit_enforce` are config knobs (see `docs/security.md` §6).
5. **Audit retention**: 30 / 90 / 365 days? **Recommended**: configurable,
   default 365 days. **Resolved (2026-07-11)**: `audit.retention_days`
   defaults to 365; no automated sweep ships in v1 (manual pruning only).
6. **Rate limiting**: per-IP, per-user, or both? **Recommended**: per-user
   primary, per-IP secondary. **Resolved (2026-07-11)**: not implemented at
   the application layer; rely on the reverse proxy (see `docs/security.md`
   §6).
7. **Branding**: project name confirmed as `Picroom`? Logo? **Recommended**:
   ship without logo in v1. **Resolved**: shipped without logo; client
   window title is "Picroom Admin".

---

## 15. Glossary

| Term | Definition |
|---|---|
| Bucket | S3-compatible container for objects |
| Driver | Implementation of `Storage` trait |
| Job | Async task in the worker queue |
| Policy | Named configuration (storage, pipeline, quota) |
| Resource | Anything subject to permission checks (image, team, …) |
| Role | Named bundle of permissions |
| Tenant | Synonym for team in multi-tenancy context |
| Variant | Derived image (AVIF / WebP / thumbnail) |

---

## 16. References

- 竞品分析: internal competitor analysis (produced during scoping; preserved in git history, not in-tree).
- Architecture review: covered in §1 of this document + the ADR series (`docs/adr/`).
- Immich architecture (for reference): https://github.com/immich-app/immich
- Lsky Pro (for reference): https://github.com/lsky-org/lsky-pro
- AWS SigV4 reference: https://docs.aws.amazon.com/IAM/latest/UserGuide/reference_sigv-create-signed-request.html
- 12-Factor App: https://12factor.net/

## 17. Desktop admin client

A standalone **Tauri 2 + Vue 3** desktop application (`desktop/`) ships
alongside the server to give administrators a GUI for day-to-day operations
(image upload/manage, public-link generation, user/team/storage admin, audit
review). It is a **thin HTTP client** over the `/api/v1/*` REST surface; it
does **not** embed the server and does not talk to PostgreSQL directly. RBAC
stays enforced server-side.

The full design, command-layer Rust crate shape, and Phase 1/2/3 progress
live in [`docs/spec-admin-client.md`](spec-admin-client.md),
[`docs/plan-admin-client.md`](plan-admin-client.md),
[`docs/tasks-admin-client.md`](tasks-admin-client.md), and
[`docs/test-plan-admin-client.md`](test-plan-admin-client.md). The
architectural decision is recorded in [ADR-0008](adr/0008-tauri-admin-client.md).

Key facts:

- `desktop/src-tauri/` is a **standalone Cargo project**, NOT a member of the
  picroom workspace (see §4.1 and ADR-0008). CI builds it with its own
  `cargo` invocation.
- Public-link capability ("公链") is exposed by the server at
  `GET /i/{key}` (no auth) plus the authenticated
  `GET /api/v1/images/:id/link` / `GET /api/v1/images/:id/file` endpoints.
  Content-Type is detected by a dependency-free magic-byte sniffer so the
  `image` crate does not have to enter the API production deps (ADR-0008).
- Build target priority is **Windows first** (NSIS + MSI installers). Linux
  and macOS bundles are not on the v1 path.

---

_End of spec v1.0_