# Picroom

> **Self-hosted image hosting service for teams. Single Rust binary. MIT-licensed.**

[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](LICENSE)
[![Build](https://img.shields.io/badge/build-passing-brightgreen)](#)
[![Coverage](https://img.shields.io/badge/coverage-%E2%89%A580%25-brightgreen)](#)
[![Rust](https://img.shields.io/badge/rust-1.75%2B-orange.svg)](https://www.rust-lang.org)

Picroom is a self-hosted image bed built for engineering teams, content
platforms, and SMBs who need more than a hobbyist PHP script but less than
a full-blown photo platform like Immich. It combines native high
performance, modern image formats (AVIF + WebP), enterprise-grade RBAC,
and an AWS S3-compatible endpoint in a single statically-linked binary.

## Features

- **Single binary**: One ~30 MB `picroom` binary, no JVM, no Node, no PHP.
- **Native image pipeline**: AVIF + WebP + thumbnails, generated in a
  background worker queue.
- **S3-compatible**: Speak to Picroom from PicGo, rclone, AWS CLI, or any
  SigV4 client.
- **Enterprise RBAC**: roles + resource-level ACLs + audit log + OIDC SSO.
- **Multi-tenancy**: teams, per-team storage policies, per-team quotas.
- **MIT licensed**: Use it commercially without disclosure.
- **Easy deploy**: One `docker compose up` brings up the whole stack.

## Quick start (Docker Compose)

```bash
git clone https://github.com/picroom/picroom.git
cd picroom
docker compose -f docker/docker-compose.yml up -d
# wait ~10 seconds for migrations
curl http://localhost:8080/healthz
# {"status":"ok"}
```

Open `http://localhost:8080` for the web UI (or use the REST API at
`/api/v1/`).

## Quick start (single binary)

```bash
# 1. Install PostgreSQL 16
# 2. Build
cargo build --release --bin picroom
# 3. Run
./target/release/picroom admin migrate
./target/release/picroom api --config ./config/example.toml
```

## Usage example (S3-compatible)

```bash
# Configure AWS CLI to point at Picroom
export AWS_ACCESS_KEY_ID=<your-token>
export AWS_SECRET_ACCESS_KEY=<your-token>
export AWS_ENDPOINT_URL=http://localhost:8080/s3

# Upload
aws s3 cp ./photo.jpg s3://my-bucket/photo.jpg

# Get
aws s3 presign s3://my-bucket/photo.jpg --expires-in 3600
```

## Usage example (REST)

```bash
# Login
curl -X POST http://localhost:8080/api/v1/auth/login \
  -H "Content-Type: application/json" \
  -d '{"email":"admin@example.com","password":"changeme"}' \
  -c cookies.txt

# Upload
curl -X POST http://localhost:8080/api/v1/images \
  -b cookies.txt \
  -F file=@./photo.jpg \
  -F team_id=00000000-0000-0000-0000-000000000000
```

## Architecture

Picroom is a Cargo workspace with 11 crates + 1 binary crate. The key
abstractions are:

- **`domain`**: pure entities, value objects, traits, errors. No I/O.
- **`storage`**: `Storage` trait (split by capability) + drivers for
  Local, S3, OSS, COS, Qiniu, MinIO.
- **`imaging`**: `Processor` trait + AVIF/WebP/resize/thumbnail/watermark.
- **`auth`**: JWT + OIDC + API tokens + RBAC.
- **`service`**: use cases (upload, query, delete, quota).
- **`api`**: axum REST + middleware + extractors.
- **`s3compat`**: AWS SigV4 verification + S3 protocol handlers.
- **`worker`**: async job consumer with retry + DLQ.
- **`admin`**: CLI subcommands (migrate, user, team, audit).

See [`docs/spec.md`](docs/spec.md) for full design, [`docs/plan.md`](docs/plan.md)
for implementation phases, and [`docs/adr/`](docs/adr/) for architectural
decision records.

## Development

```bash
# Run tests
cargo test --workspace

# Run lints
cargo fmt --all -- --check
cargo clippy --all-targets --all-features -- -D warnings

# Run coverage
cargo tarpaulin --workspace

# Run E2E (requires Docker)
RUN_E2E=1 cargo test --test e2e --features e2e
```

See [`CONTRIBUTING.md`](CONTRIBUTING.md) for the development workflow.

## License

Picroom is released under the [MIT License](LICENSE).

## Acknowledgements

Inspired by:

- [Lsky Pro](https://github.com/lsky-org/lsky-pro) — feature-rich PHP image
  bed.
- [EasyImage](https://github.com/icret/EasyImages2.0) — minimal PHP image
  bed.
- [Immich](https://github.com/immich-app/immich) — self-hosted photo platform.
- [MinIO](https://github.com/minio/minio) — S3-compatible object storage.
- [PicGo](https://github.com/Molunerfinn/PicGo) — image upload client.