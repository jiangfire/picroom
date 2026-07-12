# ADR-0008: Tauri admin client + public-link model

- **Status**: Accepted
- **Date**: 2026-07-12
- **Deciders**: Picroom maintainers

## Context

Picroom ships an `admin` CLI (`crates/admin`) for user/team/audit operations
and a REST API for image upload/management. Two needs emerge for v1:

1. A GUI for administrators — browsing images, copying public links,
   uploading via drag-drop, and managing users/teams without a terminal.
2. A "公链" (public link) capability — the core image-bed value of "upload →
   get a shareable URL". The server had `StorageSigner` but no unauthenticated
   serving route, so no true public URL existed.

### Options considered — admin client transport

| Option | Pros | Cons |
|---|---|---|
| Tauri HTTP client (chosen) | Reuses axum API + JWT/RBAC; thin client; native file streaming | Requires backend API gaps filled first |
| Embedded (Tauri bundles `picroom-admin`/DB) | Self-contained, no server needed | Couples client to backend; bypasses API auth; heavy |

### Options considered — public link

| Option | Pros | Cons |
|---|---|---|
| Self-hosted `/i/{key}` route (chosen) | Works for every storage driver; permanent links; simple | Picroom serves public bytes |
| Presigned URLs only | No public route | Time-limited; not the classic image-bed "permanent link" |
| External CDN/public bucket | Offloads traffic | Requires pre-existing CDN; driver-specific |

## Decision

1. **Admin client = Tauri 2 + Vue 3 HTTP client.** The client connects to a
   running Picroom server over REST + JWT. It does **not** embed the server or
   talk to the DB. JSON CRUD goes through `tauri-plugin-http`; large file
   upload/download streams through a Rust command layer (`reqwest`) to keep
   big files out of the JS heap. See `docs/spec-admin-client.md`.

2. **Public link = self-hosted unauthenticated `GET /i/{key}`.** Anyone who
   knows a storage key may read its bytes. Keys are `img/{uuid_v7}.bin`
   (unguessable), so "URL = capability" — the Lsky/EasyImage model. The
   `GET /api/v1/images/{id}/link` endpoint returns `{ public_url }` built
   from `server.public_url_base` (absolute) or a path-relative `/i/{key}`
   (resolved by the caller) when unset.

3. **Content-Type sniffing is dependency-free.** The `/i/{key}` handler sniffs
   Content-Type from leading magic bytes (JPEG/PNG/GIF/WebP/AVIF) rather than
   using the `image` crate, to keep the API crate free of an extra production
   dependency and because keys carry no extension (`.bin`).

4. **`desktop/` lives outside the Rust workspace.** Tauri and its WebView
   dependencies conflict with the workspace's `unsafe_code = "forbid"` lint
   and MIT-only `deny.toml`. Keeping `desktop/src-tauri` as a standalone
   Cargo project isolates those constraints from the server crates.

## Consequences

### Positive

- The server remains the single enforcement point for RBAC; the client can
  never bypass it.
- Public links are permanent, driver-agnostic, and work behind any Picroom
  deploy — including single-binary LocalDriver setups with no CDN.
- The admin client is small (Tauri ~webview + Vue bundle) and streams files
  natively, so browser memory limits don't apply.

### Negative

- `/i/{key}` is unauthenticated: knowing a leaked key grants permanent read.
  Mitigation: keys are UUID v7 (122 bits of entropy, unguessable); no
  directory listing or enumeration is exposed; reads are cached immutably.
- No per-image `is_public` toggle in v1 — an image is "public" iff its key is
  known. A `published` flag is a documented follow-up.
- Public-read audit logging is deferred (volume); revisit if abuse detection
  is needed.
- Two Cargo graphs (workspace + `desktop/`) must be maintained separately.

### Neutral

- JWT has no refresh token in v1; the client re-logs-in on `401`. A refresh
  flow is post-MVP.
- `server.public_url_base` is optional; when unset, link responses are
  path-relative and the client resolves them against the server it knows.

## Alternatives revisited

- **Embedded client**: rejected because the whole point of a client is to
  operate a *remote* deployment, and embedding would duplicate the DB layer
  and bypass RBAC.
- **`image::guess_format` for sniffing**: functionally equivalent but pulls
  the `image` crate into the API production deps for a 15-line job.

## References

- Internal: `docs/spec-admin-client.md`, `docs/spec.md` §8, §10
- ADR-0003 (storage trait ISP), ADR-0005 (RBAC model)
- AWS SigV4 / S3-compat: ADR-0004
