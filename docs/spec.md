# Picroom — Specification (v1.0)

> **Status**: Draft for review · **Version**: 1.0.0 · **Last updated**: 2026-07-05

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
| Backend | Rust | 1.75+ stable | Single binary, memory safety, async ecosystem |
| Async runtime | Tokio | 1.x | De facto Rust async runtime |
| HTTP framework | axum | 0.7+ | Tower ecosystem, ergonomic, performant |
| DB driver | sqlx | 0.7+ | Compile-time checked queries, async |
| Frontend | Vue 3 + Vite + TypeScript | 3.4+ / 5.x | Modern, lightweight, mature |
| SQL | PostgreSQL | 16 | JSONB, RLS, generated columns, mature |
| Embedded SQL fallback | SQLite | 3.45+ | Zero-ops single-user mode |
| Object storage (dev) | MinIO | latest | S3-compatible, easy to test against |
| Image processing | libvips (via `bimg` or `image` crate) + `ravif` (AVIF) | latest | Fastest safe encoder for VIPS-family |

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
| `aws-sigv4` + custom S3 dispatch | AWS SigV4 signing |
| `object_store` (optional internal) | Reference S3 client impls |
| `ravif` | AVIF encoder |
| `image` | Probe, resize, WebP, EXIF |
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
cargo sqlx migrate run

# Build everything (debug)
cargo build --workspace

# Run API (dev mode)
cargo run --bin picroom -- api --config ./config/dev.toml

# Run worker (dev mode)
cargo run --bin picroom -- worker --config ./config/dev.toml

# Admin commands
cargo run --bin picroom -- admin migrate
cargo run --bin picroom -- admin user create --email admin@example.com --role admin
cargo run --bin picroom -- admin audit tail --follow

# Tests
cargo test --workspace                                 # all unit + integration
cargo test --workspace --features e2e                  # include E2E
RUN_E2E=1 cargo test --test e2e --features e2e         # explicitly run E2E
cargo test --doc                                       # doctests
cargo bench --no-run                                   # compile-check benchmarks

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
├── rust-toolchain.toml              # pinned toolchain
├── .cargo/
│   └── config.toml                  # build settings, target-dir
├── .github/
│   └── workflows/
│       ├── ci.yml                   # quality + test + coverage + audit
│       └── release.yml              # tag-driven release + image push
├── docker/
│   ├── Dockerfile                   # multi-stage build
│   └── docker-compose.yml           # dev / demo stack
├── helm/                            # K8s chart (post-MVP)
│   ├── Chart.yaml
│   ├── values.yaml
│   └── templates/
├── crates/
│   ├── api/                         # axum routes, handlers, middleware
│   │   ├── Cargo.toml
│   │   ├── src/
│   │   │   ├── lib.rs
│   │   │   ├── router.rs
│   │   │   ├── handlers/
│   │   │   ├── extractors/
│   │   │   ├── middleware/
│   │   │   └── error.rs
│   │   └── tests/
│   ├── service/                     # use cases
│   │   ├── Cargo.toml
│   │   └── src/
│   │       ├── lib.rs
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
│   │       └── error.rs
│   ├── storage/                     # Storage trait + drivers
│   │   ├── Cargo.toml
│   │   ├── src/
│   │   │   ├── lib.rs
│   │   │   ├── driver/
│   │   │   │   ├── mod.rs
│   │   │   │   ├── local.rs
│   │   │   │   ├── s3.rs
│   │   │   │   ├── oss.rs
│   │   │   │   ├── cos.rs
│   │   │   │   └── qiniu.rs
│   │   │   ├── signing.rs
│   │   │   ├── contract_test.rs
│   │   │   └── error.rs
│   │   └── tests/
│   ├── imaging/                     # Processor trait + pipeline
│   │   ├── Cargo.toml
│   │   └── src/
│   │       ├── lib.rs
│   │       ├── processor/
│   │       │   ├── mod.rs
│   │       │   ├── probe.rs
│   │       │   ├── resize.rs
│   │       │   ├── avif.rs
│   │       │   ├── webp.rs
│   │       │   ├── thumbnail.rs
│   │       │   └── watermark.rs
│   │       └── pipeline.rs
│   ├── auth/                        # RBAC, JWT, OIDC, API token
│   │   ├── Cargo.toml
│   │   └── src/
│   │       ├── lib.rs
│   │       ├── jwt.rs
│   │       ├── oidc.rs
│   │       ├── password.rs
│   │       ├── api_token.rs
│   │       └── rbac.rs
│   ├── audit/                       # audit log
│   │   ├── Cargo.toml
│   │   └── src/
│   │       ├── lib.rs
│   │       ├── event.rs
│   │       └── sink.rs
│   ├── s3compat/                    # AWS S3-compatible endpoint
│   │   ├── Cargo.toml
│   │   └── src/
│   │       ├── lib.rs
│   │       ├── sigv4.rs
│   │       ├── routes.rs
│   │       ├── bucket.rs
│   │       └── object.rs
│   ├── worker/                      # async job consumer
│   │   ├── Cargo.toml
│   │   └── src/
│   │       ├── lib.rs
│   │       ├── job.rs
│   │       ├── retry.rs
│   │       └── dlq.rs
│   ├── infra/                       # db, cache, config, logging, telemetry
│   │   ├── Cargo.toml
│   │   └── src/
│   │       ├── lib.rs
│   │       ├── db.rs
│   │       ├── cache.rs
│   │       ├── config.rs
│   │       ├── clock.rs
│   │       ├── id.rs
│   │       └── logging.rs
│   └── admin/                       # CLI subcommands
│       ├── Cargo.toml
│       └── src/
│           ├── lib.rs
│           ├── migrate.rs
│           ├── user.rs
│           └── audit.rs
├── bin/
│   └── picroom/                     # single binary entry point
│       ├── Cargo.toml
│       └── src/
│           └── main.rs
├── web/                             # Vue 3 frontend (optional, can be CDN)
│   ├── package.json
│   ├── vite.config.ts
│   ├── index.html
│   └── src/
├── migrations/                      # sqlx migrations
│   ├── 0001_init.sql
│   ├── 0002_teams.sql
│   ├── 0003_rbac.sql
│   ├── 0004_storage_policies.sql
│   ├── 0005_images.sql
│   ├── 0006_audit.sql
│   └── 0007_jobs.sql
├── tests/                           # E2E tests (testcontainers)
│   ├── e2e_upload.rs
│   ├── e2e_s3_compat.rs
│   ├── e2e_auth.rs
│   └── e2e_quota.rs
├── benches/                         # criterion benchmarks
│   ├── image_encode.rs
│   └── upload_throughput.rs
├── docs/
│   ├── spec.md                      # this file
│   ├── plan.md                      # implementation plan
│   ├── tasks.md                     # task breakdown
│   ├── adr/                         # architecture decision records
│   │   ├── 0001-rust-and-axum.md
│   │   ├── 0002-cargo-workspace.md
│   │   ├── 0003-storage-trait-isp.md
│   │   ├── 0004-s3-compatibility.md
│   │   ├── 0005-rbac-model.md
│   │   └── 0006-image-pipeline.md
│   └── api/
│       └── openapi.yaml
├── config/
│   ├── dev.toml
│   └── example.toml
├── .gitignore
├── .dockerignore
├── README.md
├── LICENSE                          # MIT
├── CONTRIBUTING.md
└── SECURITY.md
```

### 4.1 Dependency rules (enforced by `cargo metadata` test)

```
domain      ← (depends on nothing except std + thiserror)
storage     ← domain
imaging     ← domain
auth        ← domain
audit       ← domain
infra       ← domain
service     ← domain, storage, imaging, auth, audit, infra
worker      ← domain, storage, imaging, audit, infra
s3compat    ← domain, storage, service, auth, audit
api         ← service, auth, audit, infra, s3compat
admin       ← domain, infra
picroom     ← api, worker, admin
```

Forbidden:

- `domain` depending on anything except `std`, `thiserror`, optional serde.
- `service` depending on `api` or `worker`.
- `storage` driver depending on `api`.

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
- Integration tests: `<crate>/tests/*.rs`.
- E2E tests: top-level `tests/*.rs`, gated by `--features e2e`.
- Benchmarks: `benches/*.rs`, compiled but not run by default.

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
POST   /api/v1/auth/login                       # password login
POST   /api/v1/auth/oidc/:provider/callback     # OIDC callback
POST   /api/v1/auth/logout
GET    /api/v1/me                                # current user
POST   /api/v1/teams                             # create team
GET    /api/v1/teams/:id
POST   /api/v1/teams/:id/members
GET    /api/v1/images                            # list images (filter, page)
POST   /api/v1/images                            # upload (multipart or json)
GET    /api/v1/images/:id
GET    /api/v1/images/:id/file                   # redirect to storage URL
GET    /api/v1/images/:id/thumbnail              # auto-generated thumbnail
GET    /api/v1/images/:id/avif                   # AVIF variant
GET    /api/v1/images/:id/webp                   # WebP variant
DELETE /api/v1/images/:id
GET    /api/v1/audit                             # admin: audit log
POST   /api/v1/admin/users                       # admin: create user
PATCH  /api/v1/admin/users/:id/role
POST   /api/v1/admin/storage/policies
```

### 8.2 S3-compatible API (`/s3/`)

```
PUT    /s3/:bucket/:key
GET    /s3/:bucket/:key
HEAD   /s3/:bucket/:key
DELETE /s3/:bucket/:key
POST   /s3/:bucket/:key?uploads                 # multipart init
PUT    /s3/:bucket/:key?partNumber=N&uploadId=U # multipart part
POST   /s3/:bucket/:key?uploadId=U              # multipart complete
GET    /s3/:bucket                               # list (v2)
```

SigV4 signing; path-style addressing.

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
| `storage_policies` | named storage configs (local / S3 / OSS / …) |
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

1. Explicit deny rule (highest priority).
2. Team membership role.
3. Resource-level ACL (e.g., shared with specific user).
4. Default deny.

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

Pipeline is configurable per-storage-policy:

```toml
[pipeline]
encode_avif = true
encode_webp = true
generate_thumbnail = true
strip_exif = true
max_dimension = 8192
quality = { avif = 60, webp = 80, jpeg = 85 }
```

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
    async fn list(&self, prefix: &StorageKey) -> Result<Page<ObjectMeta>, StorageError>;
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
    Oss(OssDriver),
    Cos(CosDriver),
    Qiniu(QiniuDriver),
    Minio(MinioDriver),
}

impl Storage for AnyStorage { /* dispatch via match */ }
```

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
- S3 / OSS / MinIO for object storage.
- Redis (optional) for caching.
- Ingress (nginx / Traefik) with TLS termination.

### 13.3 Configuration

Loaded from environment variables (prefix `PICROOM_`) with optional TOML override:

```toml
# config/example.toml
[server]
bind_addr = "0.0.0.0:8080"
request_timeout = "30s"

[database]
url = "postgres://picroom:secret@localhost/picroom"
max_connections = 20

[storage]
default = "primary"

[storage.policies.primary]
driver = "s3"
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

[auth.oidc.providers.google]
issuer = "https://accounts.google.com"
client_id = "${OIDC_GOOGLE_CLIENT_ID}"
client_secret = "${OIDC_GOOGLE_CLIENT_SECRET}"

[quota]
default_user_bytes = 10737418240       # 10 GiB
default_team_bytes = 1099511627776     # 1 TiB
```

Environment variables win over TOML; TOML wins over defaults.

---

## 14. Open Questions

Items that remain unresolved and require decision before implementation:

1. **Frontend deployment**: bundle into binary via `include_str!` + axum
   static handler, or separate SPA served by nginx? **Recommended**: include
   in binary for single-binary deployment.
2. **Image variant storage path layout**: by-image-id (`/img/<id>/avif`) or by
   hash (`/img/<sha256[:2]>/<sha256>.avif`)? **Recommended**: by ID for human
   debugging, hash for deduplication (post-MVP).
3. **Default DB**: ship `sqlite` mode by default, or always require PostgreSQL?
   **Recommended**: dual-mode with env switch.
4. **Quota enforcement**: hard cap (reject) vs. soft cap (allow + warn)?
   **Recommended**: hard cap by default, soft cap configurable.
5. **Audit retention**: 30 / 90 / 365 days? **Recommended**: configurable,
   default 365 days.
6. **Rate limiting**: per-IP, per-user, or both? **Recommended**: per-user
   primary, per-IP secondary.
7. **Branding**: project name confirmed as `Picroom`? Logo? **Recommended**:
   ship without logo in v1.

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

- 竞品分析: internal `docs/competitor-analysis.md` (in repo root, generated
  before this spec).
- Architecture review: covered in §1 of this document + ADR series.
- Immich architecture (for reference): https://github.com/immich-app/immich
- Lsky Pro (for reference): https://github.com/lsky-org/lsky-pro
- AWS SigV4 reference: https://docs.aws.amazon.com/IAM/latest/UserGuide/reference_sigv-create-signed-request.html
- 12-Factor App: https://12factor.net/

---

_End of spec v1.0_