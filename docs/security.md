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
- **OIDC / SSO** is implemented. `GET /api/v1/auth/oidc/:provider/login` starts the
  flow (redirects to the IdP after issuing an `HttpOnly` `state` cookie) and
  `GET /api/v1/auth/oidc/:provider/callback` completes it: the `state` cookie is
  verified for CSRF, the `code` is exchanged, and the id_token signature is verified
  against the provider's published **JWKS** (RS256/ES256). The id_token `nonce` is
  bound to the `state` cookie. Accounts are auto-provisioned as `viewer`, or `admin`
  when the email is in `auth.oidc.admin_emails`. Unknown providers return `404`;
  unverified tokens are rejected with `400`. Password login remains available.

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
| **Quota enforcement** | Enforced in production. The PG-backed `QuotaService` (wired in `bin/picroom/src/api_cmd.rs`) rejects uploads once `remaining_user` drops below the payload size. `remaining_user` = `quotas.max_bytes` − `SUM(bytes)` over non-deleted `images`, defaulting to `QuotaConfig.default_user_bytes` (10 GiB). Team-level quotas are still unlimited. `charge_user` remains a no-op because usage is computed live from the `images` table. |
| **DeleteService** | Wired. The HTTP `DELETE` handler routes through the unified `DeleteService` (storage removal + DB soft-delete + audit event). |
| **OIDC / SSO** | Implemented — `GET /auth/oidc/:provider/{login,callback}`; id_token verified against the provider JWKS (RS256/ES256), `state`+`nonce` CSRF binding, accounts auto-provisioned as `viewer` (or `admin` via `auth.oidc.admin_emails` allowlist). |
| **`admin audit tail`** | Implemented — reads `audit_events` for both PostgreSQL and SQLite (`admin/src/audit_cmd.rs`); `--follow` streams new events. |
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
