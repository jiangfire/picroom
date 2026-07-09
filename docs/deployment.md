# Deployment

Picroom ships as a single Rust binary that runs in three modes — `api`,
`worker`, `admin` — over a PostgreSQL database and an S3-compatible object
store. The fastest path to a running system is `docker compose`.

## 1. Quick start (docker compose)

```bash
git clone … picroom && cd picroom
docker compose -f docker/docker-compose.yml up -d --build
```

The compose stack brings up:

| Service | Purpose | Port |
|---|---|---|
| `postgres` | PostgreSQL 16 (metadata, jobs, audit) | 127.0.0.1:5432 |
| `minio` | S3-compatible object storage | 127.0.0.1:9000 (API), :9001 (console) |
| `migrate` | One-shot: applies SQL migrations before app start | — |
| `api` | HTTP API + S3-compatible endpoint | 8080 |
| `worker` | Async image-processing consumer | — |

The `api` and `worker` services both wait on the `migrate` service completing
successfully (`service_completed_successfully`), so tables exist before the
first request. Reset all data with:

```bash
docker compose -f docker/docker-compose.yml down -v
```

Smoke test:

```bash
curl -fsS http://localhost:8080/healthz   # {"status":"ok"}
```

## 2. Configuration

Config is merged in priority order **env > TOML > defaults**, loaded by
`picroom_infra::config` (via `figment`). Environment variables use the
`PICROOM_` prefix with `__` separating nesting:

| Variable | Meaning | Example |
|---|---|---|
| `PICROOM_DATABASE__URL` | DB DSN | `postgres://picroom:secret@host/picroom` |
| `PICROOM_DATABASE__MAX_CONNECTIONS` | Pool size | `20` |
| `PICROOM_SERVER__BIND_ADDR` | Listen address | `0.0.0.0:8080` |
| `PICROOM_SERVER__MAX_BODY_MB` | Multipart body cap | `100` |
| `PICROOM_AUTH__JWT_SECRET` | JWT signing secret (**required in prod**) | random 32+ bytes |
| `PICROOM_STORAGE__POLICIES__MINIO__*` | MinIO/S3 backend (endpoint, bucket, region, keys) | see compose |
| `PICROOM_S3_ACCESS_KEY_ID` / `PICROOM_S3_SECRET_ACCESS_KEY` | When both set, the S3 endpoint enforces SigV4 | `minio` / `minio123` |
| `PICROOM_LOGGING__FORMAT` | `json` or `plain` | `json` |

A commented reference file lives at `config/example.toml`.

## 3. Running the binary directly

```bash
# Apply migrations (idempotent)
picroom admin migrate run --config ./config/prod.toml

# Create the first admin
picroom admin user create --email admin@example.com --role admin --password '…'

# Run API and worker (typically separate processes / containers)
picroom api    --config ./config/prod.toml
picroom worker --config ./config/prod.toml --concurrency 4
```

**Production guardrail:** in release builds both `api` and `worker` refuse to
start when `PICROOM_AUTH__JWT_SECRET` is still the default `change-me`. Set a
strong random secret before exposing the service.

## 4. Storage backends

- **Local** (default fallback): `LocalDriver` rooted at `./data`. Suitable for
  single-host dev only — not for horizontal scale.
- **S3 / MinIO**: enabled when
  `PICROOM_STORAGE__POLICIES__MINIO__ENDPOINT` + access keys are set. Path-style
  addressing is used (MinIO-compatible).

## 5. Production checklist

- [ ] `PICROOM_AUTH__JWT_SECRET` set to a random value (release builds enforce).
- [ ] `PICROOM_S3_ACCESS_KEY_ID` + `PICROOM_S3_SECRET_ACCESS_KEY` set so the S3
      endpoint verifies SigV4 signatures.
- [ ] PostgreSQL with backups; migrations applied (`admin migrate run`).
- [ ] Object storage (S3/MinIO) — not `LocalDriver` — for any multi-replica setup.
- [ ] `docker compose up` reaches `ready` on `/readyz`.
- [ ] Reverse proxy with TLS termination in front of port 8080.

## 6. Kubernetes (post-MVP)

A Helm chart is planned but not yet included. The recommended shape: a
`Deployment` × N for `api`, a `Deployment` for `worker` (with an init container
running `admin migrate run`), managed PostgreSQL or Cloud SQL, and an
S3-compatible bucket. See `docs/operations.md` for day-2 procedures.
