# ADR-0007: Security hardening plan

- **Status**: Accepted
- **Date**: 2026-07-06
- **Deciders**: Picroom maintainers
- **Supersedes**: None
- **References**: `docs/review-2025-07.md`

## Context

The 2025-07 code review identified that the authentication chain, S3
compatibility layer, and DB integration are implemented but not wired. The
binary runs in "dev mode" with no authentication, no audit persistence, and
hardcoded storage.

This ADR records the decision to fix these issues in priority order before
any public deployment.

## Decision

### Phase P0 — Minimum viable loop

1. **Replace `NoopAuditSink` with `DbAuditSink`** in the binary's
   `AppState` construction. Audit events must survive process restart.
2. **Wire `PgImageRepository`** (or `SqliteImageRepository` for dev) into
   `AppState.image_repo`, so upload/list/get/delete hit the DB.
3. **Binary reads storage config** — construct `LocalDriver` or `S3Driver`
   from `Config::storage` section, not hardcoded `data/`.
4. **Worker writes `image_variants` table** — `ProcessorDeps` gains a
   `VariantRepository` and `encode_variant` INSERTs after `storage.put()`.

### Phase P1 — Security & authentication

5. **`AuthUser` extractor** reads `Authorization: Bearer <jwt>` (or
   `picroom_session` cookie), calls `JwtService::verify`, and attaches
   the resolved `UserId` + roles to the request extension.
6. **`require_auth` middleware** is mounted as a `from_fn` layer on
   `/api/v1/*` (excluding `/auth/login` and `/auth/oidc/*`).
7. **`auth::login` handler** verifies password via `PasswordHasher`,
   issues JWT via `JwtService`, sets `HttpOnly + Secure + SameSite=Lax`
   cookie.
8. **Remove `eprintln!` DB URL leak**; replace with `tracing::debug!`
   that redacts credentials. **Validate `jwt_secret`** at startup —
   panic if it equals `"change-me"` in release builds.
9. **`RequestBodyLimitLayer`** mounted on the router with a configurable
   max (default: 100 MiB) to prevent OOM from oversized multipart bodies.
10. **IDOR protection** — `images::get` and `images::delete` compare
    `image.owner_id` against the authenticated user's ID (or require
    admin role).

### Phase P2 — S3 compatibility

11. **S3 object handlers** call the configured `Storage` driver
    (PUT → `storage.put()`, GET → `storage.get()`, etc.).
12. **SigV4 verification** — each S3 request calls `sigv4::verify()`
    with the parsed `Authorization` header. Use `subtle::ConstantTimeEq`
    for signature comparison. `S3Error::SignatureMismatch` must NOT
    include the expected signature in the response.
13. **XML error format** — S3 errors return `Content-Type: text/xml`
    with `<Error><Code>…</Code><Message>…</Message></Error>`.
14. **Fix `ListObjectsV2`** route — `GET /:bucket` should call a list
    handler, not `head_object`.

### Phase P3 — Quality

15. Replace all `Mutex::lock().unwrap()` in non-test code with
    `parking_lot::Mutex` (which has no `PoisonError`) or propagate the
    error.
16. Add MIT license headers to every `.rs` file (spec S15).
17. `/readyz` pings DB (`SELECT 1`) and storage (`storage.exists(dummy)`).
18. `/metrics` uses `metrics-exporter-prometheus` for real counters.
19. Worker retry applies `RetryPolicy::delay_secs()` as `sleep()`.
    (Implemented 2026-07 in `crates/worker/src/pool.rs`; see
    `docs/review-2026-07.md` §3.3 — previously this claim was aspirational.)

## Consequences

### Positive

- System becomes production-usable after P0 + P1.
- All existing tests continue to pass (they test the infrastructure layer,
  which is unchanged).
- Security posture matches SECURITY.md promises.

### Negative

- P0 requires touching `api_cmd.rs` and `worker_cmd.rs` significantly.
- P1 requires a breaking change: all `/api/v1/*` requests now require
  a valid JWT. Existing curl-based dev workflows must be updated.

### Neutral

- PostgreSQL becomes a hard dependency for production (SQLite remains
  for dev/tests).

## Verification

Each phase is verified by:

- P0: Upload → DB row exists → worker processes → `image_variants` row exists.
- P1: Unauthenticated request → 401. Authenticated request → 200.
- P2: `aws s3 cp` round-trip succeeds.
- P3: `cargo clippy -D warnings` clean; `/readyz` reflects DB health.
