# Spec: Picroom Tauri Admin Client

> **Status**: Draft · **Feature**: desktop management client + supporting backend API
> **Parent spec**: [`docs/spec.md`](spec.md) v1.0 · **OpenAPI**: [`docs/api/openapi.yaml`](api/openapi.yaml)
> **Last updated**: 2026-07-12

## 1. Objective

### 1.1 What we are building

A cross-platform **desktop management client** for Picroom administrators, built
with Tauri 2 + Vue 3. It replaces the `picroom admin` CLI (`crates/admin`) with a
GUI for day-to-day operations against a running Picroom server: image
upload/management, public-link ("公链") generation, download, user/team/storage
administration, and audit-log review.

The client is a **thin HTTP client** over the existing axum REST API. It does
**not** embed the server or talk to the database directly. The server remains
the single source of truth and the enforcement point for RBAC.

### 1.2 Why

- The CLI is ergonomic for automation but not for browsing images, copying
  links, or onboarding new admins.
- An image bed's primary value is "upload → get a public URL"; a dedicated
  client makes that flow fast (drag-drop, one-click link copy).
- A native client streams large files through Rust without browser memory
  limits, and stores credentials out of the browser.

### 1.3 Personas

| Persona | What they do in the client |
|---|---|
| Admin | manage users/teams/storage, review audit log, upload/manage images |
| Manager | upload/manage images, manage team members |
| (Viewer) | not a target — the client is admin-oriented |

### 1.4 Non-goals (v1 of this feature)

- ❌ Embedding the picroom server or DB driver inside the client.
- ❌ Public/anonymous galleries, social features.
- ❌ Multi-account switching UI for end-users (single admin profile at a time;
  multiple server *profiles* are supported).
- ❌ Offline mode; an reachable server is required.
- ❌ Refresh-token rotation (MVP: 401 → re-login).
- ❌ Mobile (iOS/Android) builds — desktop (Windows first) only.
- ❌ An `is_public` per-image toggle (public-link model is "URL = capability").

---

## 2. Tech Stack

### 2.1 Client (`desktop/`)

| Layer | Choice | Version | Rationale |
|---|---|---|---|
| Shell | Tauri | 2.x stable | tiny binaries, Rust command layer, webview |
| Frontend framework | Vue | 3.4+ | matches parent spec §2.1 |
| Build tool | Vite | 5.x | fast HMR, Tauri's recommended bundler |
| Language | TypeScript | 5.x | type safety at the IPC + HTTP boundary |
| UI components | Naive UI | latest (2.x) | Vue 3 native, tree-shakeable, MIT |
| Router | vue-router | 4.x | |
| State | Pinia | 2.x | official Vue store |
| Tauri plugins | http, store, dialog, fs, clipboard-manager, notification | bundled w/ Tauri 2 | native HTTP (no CORS), config, file pickers |

### 2.2 Rust command layer (`desktop/src-tauri/`)

A **standalone** Cargo project (NOT a member of the picroom workspace) depending on:

| Crate | Purpose |
|---|---|
| `tauri` | command/IPC + window |
| `tauri-plugin-http` | native HTTP (also usable from frontend) |
| `tauri-plugin-store` | encrypted server-profile + token persistence |
| `tauri-plugin-dialog`, `tauri-plugin-fs` | file pickers, save-to-disk |
| `tauri-plugin-clipboard-manager` | copy public links |
| `tauri-plugin-notification` | upload-complete toasts |
| `reqwest` (rustls) | streaming multipart upload / download |
| `serde`, `serde_json` | DTOs |
| `tokio` | async streaming |
| `anyhow`, `thiserror` | errors |

### 2.3 Server-side (no new deps)

All backend changes reuse the existing stack (axum, sqlx, the `Storage` trait,
`ApiError`, `AuthUser` extractor). No new workspace crate is created.

---

## 3. Architecture

```
┌─────────────────────────────┐    REST/JSON + JWT Bearer   ┌──────────────────────────┐
│  desktop/  Tauri client     │  ─────────────────────────► │  Picroom server (axum)    │
│                             │                              │  crates/api              │
│  Vue 3 UI (Naive UI)        │                              │                          │
│   │ invoke ─┐               │   file upload/download      │  existing + NEW endpoints│
│   ▼         │ Rust commands │  ◄─────────────────────────►│  /api/v1/admin/*         │
│  tauri-     │ (reqwest,     │                              │  /api/v1/teams (list)    │
│  plugin-http│ streaming)    │   public link (no auth)      │  /api/v1/images/:id/link │
│  (JSON CRUD)│               │  ◄─────────────────────────►│  /i/:key  (public bytes) │
└─────────────┴───────────────┘                              └──────────────────────────┘
```

### 3.1 Request routing rules (client side)

| Operation | Path | Why |
|---|---|---|
| JSON CRUD (list/get/create/patch/delete) | frontend → `tauri-plugin-http` `fetch` → server | native HTTP, no CORS, no Rust glue |
| Upload (file → server) | frontend `invoke('upload_file')` → Rust `reqwest` multipart stream | keep big files out of JS heap; emit progress events |
| Download (server → file) | frontend `invoke('download_image')` → Rust stream-to-disk | same reason |
| Public-link open/copy | frontend reads `{public_url}` and copies/opens it | no auth needed |

### 3.2 Auth model

1. Login page collects `{ server_url, email, password }`.
2. Frontend POSTs `/api/v1/auth/login` → receives `{ access_token }`.
3. Rust stores `{ server_url, email, token }` in `tauri-plugin-store`
   (app-data dir, one entry per profile).
4. A managed `AppState` holds the active profile + a long-lived `reqwest::Client`.
5. Every request injects `Authorization: Bearer <token>`.
6. On `401`: clear token, redirect to login. (No refresh token in MVP.)

### 3.3 Public-link model ("公链")

- **Capability = URL**: an uploaded image's key is `img/{uuid_v7}.bin`
  (`service/src/upload.rs:167`). UUID v7 is unguessable, so knowing the URL is
  the access grant — same model as Lsky/EasyImage.
- The server serves raw bytes at `GET /i/:key` **without** auth.
- Content-Type is determined by **magic-byte sniffing** (`image::guess_format`
  on the leading bytes), because keys carry no extension (`.bin`) and the route
  must work for variants (avif/webp/thumb) without per-object DB lookups.
- `GET /api/v1/images/:id/link` returns the absolute public URL built from the
  server's configured `server.public_url_base` (falls back to the request's
  `Host`). For S3-backed storage, the same endpoint returns a presigned URL
  with `expires_at`.
- No directory listing; no enumeration. Audit logging of public reads is
  deferred (high volume).

---

## 4. Backend API surface (Phase 1)

All new endpoints are added to `crates/api`. Each is JWT-guarded (mounted under
`/api/v1/`) unless noted, and uses the existing `AuthUser` extractor + RBAC
`PermissionService` checks. The public `/i/*` route is the only unauthenticated
addition.

### 4.1 New routes

| Method | Path | Auth | RBAC | Notes |
|---|---|---|---|---|
| GET | `/i/:key` | **none** | — | raw bytes; sniff Content-Type |
| GET | `/api/v1/images/:id/link` | Bearer | `Image/Read` | `{ public_url, expires_at? }` |
| GET | `/api/v1/images/:id/file` | Bearer | `Image/Read` | 302 to public/signed URL |
| GET | `/api/v1/admin/users` | Bearer | `User/Admin` | paginated list |
| POST | `/api/v1/admin/users/:id/disable` | Bearer | `User/Admin` | set `disabled=true` |
| GET | `/api/v1/teams` | Bearer | `Team/Read` | list teams |
| GET | `/api/v1/teams/:id/members` | Bearer | `Team/Read` | list members |
| GET | `/api/v1/admin/storage/policies` | Bearer | `System/Admin` | from config + DB |
| POST | `/api/v1/admin/storage/policies` | Bearer | `System/Admin` | create (DB row) |

### 4.2 Repository trait additions (`service/src/repo.rs`)

```rust
trait ImageRepository {
    // existing: insert, get, list_for_owner, delete, ping
    async fn find_by_key(&self, key: &StorageKey) -> Result<Option<Image>, ServiceError>;
}

trait UserRepository {
    // existing: find_by_email, create_user, set_role
    async fn list(&self, page: PageReq) -> Result<Page<User>, ServiceError>;
    async fn find_by_id(&self, id: UserId) -> Result<Option<User>, ServiceError>;
    async fn disable(&self, id: UserId, disabled: bool) -> Result<(), ServiceError>;
}

trait TeamRepository {
    // existing: create, get, list, add_member
    async fn list_members(&self, team_id: TeamId) -> Result<Vec<TeamMember>, ServiceError>;
}
```

Each new method gets a PostgreSQL implementation. (SQLite admin paths remain
CLI-only; the client targets a server backed by PostgreSQL, which is the
production DB.)

### 4.3 Configuration addition (`infra/src/config.rs`)

```toml
[server]
bind_addr = "0.0.0.0:8080"
public_url_base = "https://cdn.example.com"   # NEW — base for /i/* public links
```

When unset, the link handler falls back to constructing the URL from the
incoming request's scheme + host.

### 4.4 Public-route wiring detail

`require_auth` (`api/src/extractors/auth.rs:90`) only enforces on `/api/v1/*`,
so `GET /i/:key` passes through regardless of registration order. The handler:

1. Parses `StorageKey` from the path (reject path-escape attempts — reuse
   `StorageKey::parse`).
2. `storage.get(&key)` → on `NotFound` return 404.
3. Sniff Content-Type from the first bytes; set `Cache-Control: public, max-age=31536000, immutable`.
4. Return bytes with 200.

---

## 5. Project Structure

```
picroom/
├── desktop/                          # NEW — Tauri client (standalone)
│   ├── package.json
│   ├── vite.config.ts
│   ├── tsconfig.json
│   ├── index.html
│   ├── .gitignore                    # node_modules, dist, src-tauri/target
│   ├── README.md
│   ├── src/                          # Vue 3 frontend
│   │   ├── main.ts
│   │   ├── App.vue
│   │   ├── router/index.ts           # routes + auth guard
│   │   ├── stores/auth.ts            # Pinia: server/email/token
│   │   ├── api/
│   │   │   ├── client.ts             # tauri-plugin-http wrapper + Bearer inject
│   │   │   ├── images.ts
│   │   │   ├── users.ts
│   │   │   ├── teams.ts
│   │   │   ├── audit.ts
│   │   │   └── storage.ts
│   │   ├── views/
│   │   │   ├── LoginView.vue
│   │   │   ├── ImagesView.vue
│   │   │   ├── UsersView.vue
│   │   │   ├── TeamsView.vue
│   │   │   ├── AuditView.vue
│   │   │   ├── StorageView.vue
│   │   │   └── SettingsView.vue
│   │   └── components/
│   │       ├── UploadDropzone.vue
│   │       ├── ImageGrid.vue
│   │       └── CopyLinkButton.vue
│   └── src-tauri/                    # Rust command layer (NOT in workspace)
│       ├── Cargo.toml
│       ├── build.rs
│       ├── tauri.conf.json
│       ├── icons/
│       └── src/
│           ├── main.rs
│           ├── lib.rs                # Tauri builder + plugin registration
│           ├── state.rs              # managed state: reqwest client + active profile
│           ├── store.rs              # tauri-plugin-store read/write helpers
│           ├── error.rs              # serializable command error
│           └── commands/
│               ├── mod.rs
│               ├── auth.rs           # login, logout, get_session, list_profiles
│               ├── upload.rs         # upload_file(path, team_id?) with progress events
│               └── download.rs       # download_image(id, save_dir)
├── crates/api/src/                   # + public handler, +admin/user/team/storage handlers
├── crates/service/src/repo.rs        # +trait methods +PG impls
├── crates/infra/src/config.rs        # +public_url_base
└── docs/
    ├── spec-admin-client.md          # this file
    ├── adr/0007-tauri-admin-client.md # NEW — records architecture decisions
    └── api/openapi.yaml               # updated
```

---

## 6. Commands

### 6.1 Backend (existing workspace)

```bash
# from repo root
cargo fmt --all -- --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test --workspace
cargo run --bin picroom -- api --config ./config/example.toml
```

### 6.2 Client (`desktop/`)

```bash
cd desktop
npm install
npm run dev            # Vite dev (frontend only)
npm run tauri dev      # full Tauri dev (Rust + webview)
npm run tauri build    # produce Windows installer (MSI/NSIS)
npm run lint           # eslint + vue-tsc (typecheck)
npm run test           # vitest (frontend unit)
cargo test --manifest-path src-tauri/Cargo.toml   # Rust command-layer unit tests
```

### 6.3 End-to-end smoke (manual, for Definition of Done)

```bash
# 1. server up (docker compose)
docker compose -f docker/docker-compose.yml up -d
# 2. migrate + seed an admin
./target/release/picroom admin migrate
./target/release/picroom admin user create --email admin@example.com --role admin
# 3. launch client
cd desktop && npm run tauri dev
# 4. log in, upload an image, copy its public link, open it in a private window (no auth) → image renders
```

---

## 7. Code Style

### 7.1 Rust (server + command layer)

Follows parent spec §5: `cargo fmt` defaults, `clippy::pedantic`, `Result`
everywhere, no `unwrap()` outside tests, license header in every file.

```rust
// crates/api/src/handlers/public.rs
//! Public (unauthenticated) image-byte serving.

use crate::error::ApiError;
use crate::state::AppState;
use axum::extract::{Path, State};
use axum::response::{IntoResponse, Response};
use axum::http::{header, StatusCode};
use picroom_domain::StorageKey;
use std::sync::Arc;

/// `GET /i/:key` — serve raw object bytes with no auth.
///
/// Content-Type is sniffed from the leading bytes via `image::guess_format`,
/// because upload keys are `img/{uuid}.bin` and carry no extension.
pub async fn serve_object(
    State(state): State<Arc<AppState>>,
    Path(key_raw): Path<String>,
) -> Result<Response, ApiError> {
    let key = StorageKey::parse(&key_raw)
        .map_err(|e| ApiError::bad_request(format!("invalid key: {e}")))?;
    let bytes = state.storage.get(&key).await.map_err(|e| match e {
        picroom_storage::StorageError::NotFound(_) => {
            ApiError::new(StatusCode::NOT_FOUND, "not_found", "no such object")
        }
        other => ApiError::internal(other.to_string()),
    })?;
    let content_type = sniff_content_type(&bytes);
    Ok((
        StatusCode::OK,
        [
            (header::CONTENT_TYPE, content_type),
            (header::CACHE_CONTROL, "public, max-age=31536000, immutable"),
        ],
        bytes,
    )
        .into_response())
}

fn sniff_content_type(b: &[u8]) -> &'static str {
    match image::guess_format(b).ok() {
        Some(image::ImageFormat::Jpeg) => "image/jpeg",
        Some(image::ImageFormat::Png) => "image/png",
        Some(image::ImageFormat::WebP) => "image/webp",
        Some(image::ImageFormat::Avif) => "image/avif",
        Some(image::ImageFormat::Gif) => "image/gif",
        _ => "application/octet-stream",
    }
}
```

### 7.2 TypeScript / Vue

- `<script setup lang="ts">` SFCs, Composition API only.
- Strict `tsconfig` (`"strict": true`); no `any` without an inline justification.
- API DTOs mirrored from OpenAPI in `src/api/types.ts`.
- Files: `PascalCase.vue` for components/views, `camelCase.ts` for modules.

```ts
// desktop/src/api/client.ts
import { fetch } from '@tauri-apps/plugin-http';
import { useAuthStore } from '@/stores/auth';

export class ApiError extends Error {
  constructor(public status: number, public code: string, message: string) {
    super(message);
  }
}

export async function request<T>(path: string, init: RequestInit = {}): Promise<T> {
  const auth = useAuthStore();
  const res = await fetch(`${auth.serverUrl}${path}`, {
    ...init,
    headers: { 'Content-Type': 'application/json', Authorization: `Bearer ${auth.token}`, ...(init.headers ?? {}) },
  });
  if (res.status === 401) { auth.clear(); throw new ApiError(401, 'unauthorized', 'session expired'); }
  if (!res.ok) throw new ApiError(res.status, 'error', await res.text());
  return res.status === 204 ? (undefined as T) : ((await res.json()) as T);
}
```

---

## 8. Testing Strategy

### 8.1 Server-side

- **Unit / integration** (`cargo test`): every new handler gets an axum
  `oneshot` integration test using `AppState::for_dev` + an in-memory/local
  `Storage` (see existing pattern in `crates/api/src/router.rs:62`). Cover:
  - public route: 200 + correct content-type, 404 on missing, 400 on bad key,
    200 on `/api/v1/*` still requires auth (regression).
  - link endpoint: returns absolute URL; honors `public_url_base`.
  - admin user/team/storage endpoints: RBAC deny for non-admin, happy path,
    pagination.
- Repository methods: round-trip tests against the existing PG test setup;
  feature-gated like the current repo tests.
- **OpenAPI snapshot**: update `docs/api/openapi.yaml` and any insta snapshot.

### 8.2 Client-side

- **Frontend unit** (vitest): Pinia stores + `api/*` modules mocked against a
  `msw`-style or hand-rolled fake server. Cover: 401 → token cleared, Bearer
  header injected, pagination cursors threaded.
- **Rust command-layer unit** (`cargo test --manifest-path src-tauri/Cargo.toml`):
  `upload`/`download` commands tested against a `wiremock` multipart endpoint
  with a tmpfile; progress events asserted via a test event channel.
- **E2E** (manual smoke, §6.3): login → upload → copy public link → open
  unauthenticated → image renders. Plus admin CRUD happy paths.

### 8.3 Coverage expectations

- New server handlers: ≥ 80 % lines (parent spec §6.2).
- Client: no hard threshold for v1, but each `api/*` module and each Rust
  command must have at least one passing test.

---

## 9. Boundaries

### 9.1 Always do

- Run `cargo fmt && cargo clippy && cargo test` before committing server changes.
- Add an axum integration test for every new route.
- Inject `Authorization: Bearer` on every authenticated client request; never
  send tokens in URLs.
- Stream uploads/downloads > 1 MiB through the Rust command layer.
- Keep `desktop/src-tauri` out of the picroom workspace `Cargo.toml`.
- Mirror every DTO between OpenAPI, the Rust command layer, and `src/api/types.ts`.

### 9.2 Ask first

- Changing the public-link model (e.g. adding an `is_public` flag, auth on
  `/i/*`, or directory semantics).
- Adding a non-MIT dependency to either the workspace or the client.
- Changing the RBAC role hierarchy or the `users`/`teams` schema in a
  backwards-incompatible way.
- Introducing a refresh-token / SSO flow.
- Bundling the client into the single `picroom` binary.

### 9.3 Never do

- Ship the JWT in a URL query parameter or log it.
- Serve `/i/*` with directory listing or object enumeration.
- Add `desktop/` to the picroom workspace `members` (breaks `deny.toml` /
  `unsafe_code = forbid`).
- Load whole uploaded/downloaded files into JS memory.
- Disable a failing test without documenting why.
- Commit a server profile or token to git.

---

## 10. Success Criteria

The feature is **done** when all of the following hold:

| # | Criterion | Verification |
|:-:|---|---|
| C1 | `GET /i/:key` returns image bytes with correct Content-Type, no auth, 404 on missing | axum test + manual curl |
| C2 | `GET /api/v1/images/:id/link` returns an absolute URL that opens unauthenticated | test + manual |
| C3 | Admin user/team/storage/list endpoints return correct data and reject non-admins with 403 | axum tests |
| C4 | `cargo fmt/clippy/test --workspace` clean | CI commands |
| C5 | `desktop/` builds via `npm run tauri build` producing a Windows installer | local build |
| C6 | Client can log in, upload an image (drag-drop), see it in the grid, copy its public link, open it in a private browser → renders | manual smoke §6.3 |
| C7 | Client can download an image to a chosen folder via native dialog | manual |
| C8 | Client can list users, change a role, disable a user, list teams + members, list/create storage policies, page through audit log | manual |
| C9 | A 401 from the server redirects the client to login and clears the stored token | vitest + manual |
| C10 | `docs/api/openapi.yaml` + a new ADR (`0007-tauri-admin-client.md`) are committed | `git diff` |
| C11 | No new clippy/test failures introduced in the workspace; client `npm run lint` clean | commands |

---

## 11. Open Questions

1. **`public_url_base` default**: when unset, fall back to request `Host` (works
   for single-host deploys) — acceptable, or require it to be set explicitly in
   production? **Recommended**: fall back + log a warning.
2. **Audit on public reads**: deferred (volume). Confirm we skip it for MVP.
3. **Multi-profile**: support >1 saved server profile (switchable in Settings)?
   **Recommended**: yes, store is keyed by profile name; cheap to include now.
4. **Variant endpoints** (`/avif`, `/webp`, `/thumbnail`): serve via the same
   `/i/<variant-key>` public route (no new code) — confirm this satisfies the
   "get WebP/AVIF" need, or whether dedicated `/api/v1/images/:id/webp`
   redirect endpoints are still wanted. **Recommended**: rely on `/i/*` for v1;
   add redirect endpoints only if requested.
5. **Updater**: ship Tauri's updater plugin for auto-update? **Recommended**:
   post-MVP.

---

## 12. Phasing (cross-reference to plan/tasks docs)

- **Phase 1 — Backend** (in `crates/`): tasks 1.1–1.7 from the plan.
  Public route + link endpoint first (core, unblocks client), then admin
  user/team/storage endpoints, then OpenAPI + tests.
- **Phase 2 — Client** (in `desktop/`): tasks 2.1–2.13. Scaffold → auth →
  images core (list/upload/link/download) → remaining admin screens → packaging.

Detailed task breakdown lives in `docs/plan-admin-client.md` and
`docs/tasks-admin-client.md` (produced in the PLAN/TASKS phases).

_End of spec._
