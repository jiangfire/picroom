# Implementation Plan: Picroom Tauri Admin Client

> **Status**: Implemented (Phase 1 + Phase 2 complete) · **Parent**: [`spec-admin-client.md`](spec-admin-client.md)
> **Last updated**: 2026-07-25

## Overview

A Tauri 2 + Vue 3 desktop client (`desktop/`) acting as a thin HTTP client over
the existing Picroom axum REST API, plus the supporting backend surface for the
"公链" (public link) capability. The client never embeds the server and never
talks to the DB; RBAC stays enforced server-side. See ADR-0008 for the
architecture decisions.

This plan is the post-hoc record of what was built, split into vertical slices
that each deliver working, testable functionality. It is paired with the
per-task checklist in [`tasks-admin-client.md`](tasks-admin-client.md).

## Architecture Decisions (recorded in ADR-0008)

- **Admin client transport = Tauri HTTP client** (reuses axum + JWT/RBAC, thin).
- **Public link = self-hosted unauthenticated `GET /i/{key}`** with capability
  URLs (`img/{uuid_v7}.bin`).
- **Content-Type sniffing is dependency-free** (no `image` crate pulled into the
  API production deps).
- **`desktop/` lives outside the Rust workspace** to isolate Tauri's webview
  dependencies from `unsafe_code = "forbid"` and the MIT-only `deny.toml`.

## Dependency Graph (built bottom-up)

```
Config + StorageKey ──► /i/* public route ──┐
                                            ├─► /images/:id/link (+ /file redirect)
User/Team/Storage repos ──► admin handlers ─┤
                                            ├─► desktop Rust command layer
                                            └─► desktop Vue views (api/* + views/*)
```

The public route unblocks the client core (link copy + open); admin endpoints
unblock the remaining admin screens.

## Phasing

### Phase 1 — Backend (in `crates/`)

Vertical slice per row; each row ships its own axum integration test.

| # | Slice | Status | Verification |
|:-:|---|:-:|---|
| 1.1 | `GET /i/*key` public route + magic-byte sniffing | ✅ | `crates/api/src/handlers/public.rs` (135 LOC, 6 sniff tests) |
| 1.2 | `GET /api/v1/images/:id/link` + `/file` redirect | ✅ | `crates/api/src/handlers/images.rs:250,280` |
| 1.3 | `public_url_base` config + `AppState::with_public_url_base` | ✅ | `crates/api/src/state.rs:60,134` |
| 1.4 | `UserRepository::{list, find_by_id, set_disabled, set_role}` | ✅ | `crates/service/src/repo.rs:275-412` |
| 1.5 | Admin users handlers (`/admin/users`, `/disable`, `/enable`, `/role`) | ✅ | `crates/api/src/handlers/admin.rs` |
| 1.6 | Team listing + members (`/teams`, `/teams/:id/members`) | ✅ | `crates/api/src/handlers/teams.rs` |
| 1.7 | Storage-policy list/create (`/admin/storage/policies`) | ✅ | `crates/api/src/handlers/storage.rs` |
| 1.8 | OpenAPI snapshot updated with all new routes | ✅ | `docs/api/openapi.yaml` (21 v1 paths) |

**Checkpoint P1**: ✅ `cargo test --workspace` → 280+ tests green; clippy clean.

### Phase 2 — Client (in `desktop/`)

| # | Slice | Status | Verification |
|:-:|---|:-:|---|
| 2.1 | Tauri 2 + Vue 3 + Vite scaffold; standalone Cargo project | ✅ | `desktop/src-tauri/Cargo.toml` (NOT a workspace member) |
| 2.2 | `tauri-plugin-{http,store,dialog,fs,clipboard-manager}` wired | ✅ | `desktop/src-tauri/src/lib.rs` |
| 2.3 | Rust command layer: `auth::{login,logout,get_session,list_profiles,save_profile,set_active_profile,remove_profile}` | ✅ | `commands/auth.rs` (199 LOC, 2 wiremock tests) |
| 2.4 | Rust command layer: `upload_file` (streaming multipart + progress channel) | ✅ | `commands/upload.rs` (189 LOC, 1 wiremock test) |
| 2.5 | Rust command layer: `download_image` (stream-to-disk, relative + absolute URLs) | ✅ | `commands/download.rs` (139 LOC, 1 wiremock test) |
| 2.6 | Profile persistence via `tauri-plugin-store` | ✅ | `desktop/src-tauri/src/store.rs` (Profile + persistence), `state.rs` (login session) |
| 2.7 | Frontend `api/*` modules + Bearer-injecting client | ✅ | `desktop/src/api/{client,images,users,teams,audit,storage}.ts` |
| 2.8 | `LoginView` + `router` auth guard + `stores/auth` Pinia | ✅ | `desktop/src/views/LoginView.vue`, `stores/auth.ts` |
| 2.9 | `ImagesView` + extracted `components/{UploadDropzone,ImageGrid,CopyLinkButton}.vue`: list, drag-drop upload, copy link, download, delete | ✅ | `desktop/src/views/ImagesView.vue` + `desktop/src/components/*.vue` |
| 2.10 | `UsersView` + `TeamsView` + `StorageView` + `AuditView` | ✅ | `desktop/src/views/*.vue` |
| 2.11 | `SettingsView` with multi-profile switch/activate/delete | ✅ | `desktop/src/views/SettingsView.vue` (spec §11 OQ3 ✓) |
| 2.12 | `App.vue` shell + Naive UI layout + menu + logout | ✅ | `desktop/src/App.vue` |
| 2.13 | `vitest` for `api/client.ts` (401 → token cleared, Bearer injected) | ✅ | `desktop/src/api/client.test.ts` |

**Checkpoint P2**: ✅ `npm run lint` clean; `npm run test` 2/2; `cargo test --manifest-path src-tauri/Cargo.toml` 4/4.

### Phase 3 — Polish & Definition-of-Done

| # | Slice | Status |
|:-:|---|:-:|
| 3.1 | Workspace `cargo fmt --all -- --check` clean (C4) | ⏳ pending |
| 3.2 | ADR references corrected in source comments | ⏳ pending |
| 3.3 | This plan + `tasks-admin-client.md` committed (spec §12) | ⏳ in progress |
| 3.4 | `npm run tauri build` produces Windows installer (C5) | manual |
| 3.5 | End-to-end smoke §6.3 (login → upload → 公链 → private window) | manual |

## Risks and Mitigations

| Risk | Impact | Mitigation |
|---|:-:|---|
| `desktop/src-tauri` accidentally joins workspace → breaks `deny.toml` | High | `[workspace]` empty-table marker in `desktop/src-tauri/Cargo.toml:36`; CI builds both graphs separately |
| Public link leakage (leaked key = permanent read) | Medium | UUID v7 keys (122 bits entropy); no directory listing; immutable cache |
| 401 storm on token expiry | Low | Client clears token + redirects to login on first 401; refresh flow deferred (spec §1.4) |
| Tauri WebView2 deploys differently per Windows SKU | Low | C5 manual build verifies installer before each release |

## Open Questions (resolved)

| # | Question | Resolution |
|:-:|---|---|
| 1 | `public_url_base` default when unset? | Path-relative `/i/{key}`; client resolves against known server (spec §11 OQ1) |
| 2 | Audit on public reads? | Deferred for MVP — volume (spec §11 OQ2) |
| 3 | Multi-profile support? | Shipped — SettingsView manages N profiles (spec §11 OQ3) |
| 4 | Variant endpoints (`/avif`, `/webp`, `/thumbnail`)? | Routed through same `/i/<variant-key>`; OpenAPI also lists dedicated redirect endpoints (spec §11 OQ4) |
| 5 | Tauri updater plugin? | Post-MVP (spec §11 OQ5) |

## Deviations from spec (intentional, documented)

Items 1–2 below were **resolved on 2026-07-25** — the code was refactored to
match the spec layout exactly. Items 3–4 remain deliberate deviations from the
spec's literal file/structure layout but deliver the same functionality.

1. ~~**`state.rs` / `store.rs` / `error.rs` collapsed into `config.rs`** (spec §5)~~
   **RESOLVED 2026-07-25.** `desktop/src-tauri/src/` now ships the three modules
   per spec: `error.rs` (`Error` newtype + `Result<T>` + `From` impls),
   `store.rs` (Profile + `tauri-plugin-store` persistence), `state.rs` (login
   `Session`/`LoginPayload`/`LoginResult` + `perform_login`). The former
   `config.rs` was deleted; `upload.rs`/`download.rs`/`auth.rs` now return
   `crate::error::Result<_>`.
2. ~~**`components/{UploadDropzone,ImageGrid,CopyLinkButton}.vue` inlined into
   `ImagesView.vue`** (spec §5)~~ **RESOLVED 2026-07-25.** The three components
   are now extracted into `desktop/src/components/`; `ImagesView.vue` is a thin
   composition root (`<UploadDropzone @uploaded>` + `<ImageGrid>`).
3. **`tauri-plugin-notification` not installed** (spec §2.1). Upload-complete
   feedback uses Naive UI `useMessage().success(...)` which renders in-app
   toast already; a native OS notification would duplicate that channel.
4. **`ImageRepository::find_by_key` not added** (spec §4.2). The public route
   goes straight through `Storage::get` with no DB lookup (spec §3.3, §4.4
   "no per-object DB lookups"), so the method would be dead code.

### Corrections (2026-07-25)

- **`link`/`file` RBAC gate corrected to `Image/Read`** (spec §4.1). The handler
  guard was `owner OR Image/Update`; it is now `owner OR Image/Read`, matching
  the spec table. Consequence: a `viewer` (who holds `Image/Read`) can now
  generate the public link for *any* image. The integration test
  `image_link_forbids_viewer_accessing_others_image` now uses a token with an
  empty scope (no `Image/Read`) to assert 403, and a new positive test
  `image_link_allows_viewer_accessing_others_image` asserts 200 for a viewer.
- **`auth.rs::login` profile name derived from server host** (multi-profile):
  each server now gets an independent profile instead of the hardcoded
  `"default"`, matching the multi-profile Settings UI.

## Verification Commands

```bash
# backend
cargo fmt --all -- --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test --workspace

# client (frontend)
cd desktop && npm run lint && npm run test

# client (Rust command layer)
cd desktop && cargo test --manifest-path src-tauri/Cargo.toml

# client (full build, ~10 min, manual)
cd desktop && npm run tauri build
```
