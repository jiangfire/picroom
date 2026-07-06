# Security Policy

## Supported Versions

| Version | Supported |
|---|---|
| 1.0.x | ✅ Active |
| 0.1.x (preview) | ⚠️ Best-effort |
| < 0.1 | ❌ |

## Reporting a Vulnerability

**Please do not open a public GitHub issue for security vulnerabilities.**

Email: **security@picroom.dev** (PGP key to be published).

We will acknowledge within **48 hours** and provide a fix timeline within
**7 days**. Critical issues are patched within **24 hours**.

## Threat Model

Picroom is a self-hosted service. The threat model assumes:

- The host OS, container runtime, and reverse proxy are **trusted**.
- The PostgreSQL database is **trusted** (use TLS, restrict network).
- The object storage backend is **trusted** but should use TLS in transit.
- All network inputs (HTTP, multipart uploads, OIDC callbacks) are
  **untrusted** and validated.
- All file contents (images) are **untrusted** and processed in sandboxed
  workers.

## Hardening Recommendations

1. **Run behind TLS-terminating reverse proxy** (nginx, Caddy, Traefik).
2. **Use managed PostgreSQL** with TLS + network ACLs.
3. **Use object storage with bucket policies** to deny public reads;
   Picroom always serves through signed URLs.
4. **Set `PICROOM_AUTH__JWT__SECRET`** to a high-entropy random value
   (e.g. `openssl rand -base64 48`).
5. **Enable audit log retention** to meet your compliance needs.
6. **Set quotas** to prevent single-tenant resource exhaustion.
7. **Use OIDC** instead of local passwords when possible.
8. **Rotate API tokens** periodically.
9. **Set up rate limits** at the reverse-proxy layer as well.
10. **Subscribe to GitHub Security Advisories** for this repository.

## Dependency Policy

- All dependencies must be MIT-compatible (`cargo deny check` enforces).
- High-severity advisories block CI (`cargo audit` enforces).
- Critical CVE fix release within 7 days; high within 30 days.

## Cryptographic Choices

| Concern | Algorithm | Library |
|---|---|---|
| Password hashing | Argon2id (default OWASP params) | `argon2` crate |
| JWT signing | HS256 / RS256 / ES256 | `jsonwebtoken` crate |
| AWS signing | SigV4 | `aws-sigv4` crate |
| Random IDs | UUIDv7 (time-ordered) | `uuid` crate |
| Session cookies | HttpOnly + Secure + SameSite=Lax | axum cookies |

## Acknowledgements

We credit reporters of valid vulnerabilities in the next release notes
unless anonymity is requested.