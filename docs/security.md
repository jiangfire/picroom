# Security

Picroom's security model, the controls in place, and the known gaps. Read this
before exposing the service to untrusted networks.

## 1. Authentication

- **Login** (`POST /api/v1/auth/login`) verifies the password against the
  stored Argon2id hash (`PasswordHasher`). Unknown email, wrong password, and
  disabled account all return an identical `401` so valid emails cannot be
  enumerated by response shape or timing.
- On success a JWT is issued whose `sub` is the user id (a UUID) and whose
  `scopes` carry the user's role. Tokens are verified on every `/api/v1/*`
  request by the `require_auth` middleware — a forged or expired token is
  rejected with `401` at the gate.
- `GET /healthz`, `/readyz`, `/metrics`, and `/auth/*` are public. Everything
  else under `/api/v1/*` requires a valid bearer token.
- **OIDC** (`/auth/oidc/:provider/callback`) is unimplemented (returns `501`)
  and the routes are not mounted. Password login is the only auth path today.

**Required in production:** `PICROOM_AUTH__JWT_SECRET` must be changed from the
default `change-me`. Release builds of both `api` and `worker` refuse to start
with the default (`picroom_infra::require_strong_jwt_secret`); a warning is
logged in debug builds.

## 2. Authorization (RBAC)

Image handlers take a non-optional `AuthUser`, so the identity is always
established before any read/write:

- **Upload** attributes the image to the authenticated user (never a default).
- **GET / DELETE `/images/:id`** compare `image.owner_id` to the caller;
  non-owners receive `403` unless they hold the `admin` role.
- **GET `/images`** (list) scopes results to the caller; admins may pass an
  `owner` query param to list another user's images.

This closes the IDOR bypass present in the 2025-07 baseline, where handlers
took `Option<AuthUser>` and skipped the owner check when the token failed to
extract.

## 3. S3-compatible endpoint (`/s3/*`)

By default the S3 endpoint is **open** (no signature required) — appropriate
for trusted dev networks. To enforce AWS SigV4:

```bash
PICROOM_S3_ACCESS_KEY_ID=… PICROOM_S3_SECRET_ACCESS_KEY=… picroom api …
```

When both are set, every `/s3/*` request is run through `require_sigv4`, which:

- parses the `Authorization: AWS4-HMAC-SHA256 …` header,
- looks up the secret for the presented access key,
- recomputes the signature and compares it **in constant time** (`subtle::ConstantTimeEq`),
- rejects mismatches with a `403 SignatureDoesNotMatch` XML error that does
  **not** leak the expected signature.

The verifier is exercised by unit tests against the crate's own signing
primitives; interop testing against `aws-cli`/`rclone` end-to-end is the
remaining hardening step before relying on it in production.

**Multipart** is not supported; the handlers return an explicit
`501 NotImplemented` XML error so clients fall back to a single `PUT` rather
than silently losing data.

## 4. Request limits

`RequestBodyLimitLayer` caps multipart bodies at `PICROOM_SERVER__MAX_BODY_MB`
(default 100 MiB) to prevent memory-exhaustion DoS. The limit is applied in the
binary wiring (`api_cmd`), not the library router, so test harnesses that call
`build_router` directly are unbounded by design.

## 5. Secret handling

- The database URL is never logged in full; `api_cmd` logs only the scheme.
- Internal errors (`ApiError::internal`) are logged server-side at `error`
  level but the client always receives a generic `"internal server error"` —
  SQL errors, S3 response bodies, and filesystem paths are not leaked.
- No secrets are committed; configuration is sourced from environment/TOML.

## 6. Known limitations / accepted risk

These are documented gaps, not silent failures:

| Area | Status |
|---|---|
| **Quota enforcement** | `QuotaService` is a deferred stub — `remaining_*` returns `u64::MAX`, `charge_*` is a no-op. Per-user/team byte caps are **not** enforced. Set external limits (reverse proxy, bucket quotas) until implemented. |
| **DeleteService** | The HTTP `DELETE` handler performs real deletion (storage + DB row); `DeleteService` as a unit only emits audit and is not used by the live path. |
| **OIDC / SSO** | Not implemented (`501`). |
| **`admin audit tail`** | Returns an explicit "not implemented" error (events are still recorded via `DbAuditSink`). |
| **Rate limiting** | Not implemented at the application layer; rely on the reverse proxy. |

## 7. Vulnerability & license policy

CI runs `cargo audit` and `cargo deny check` on every change.

- Two RUSTSEC advisories (`rsa`, `tokio-tar`) are waived in `audit.toml` /
  `deny.toml` — they affect **only** the `testcontainers` dev-dependency and
  are not present in the release binary (proven via
  `cargo tree --edges normal`). The waiver carries a review date and is
  re-evaluated quarterly.
- `cargo deny` permits a documented set of permissive licenses; every non-MIT
  license is annotated with the crate that requires it (mostly the AVIF stack
  `ravif`/`rav1e`, mandated by spec §2.2). `version = "*"` wildcards are denied.

## 8. Reporting

File security issues via the project's private disclosure channel (see
`SECURITY.md`), not as public issues.
