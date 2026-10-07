# Tasks: Picroom Tauri Admin Client

> **Status**: Phase 1 + Phase 2 shipped; Phase 3 in progress
> **Parent**: [`plan-admin-client.md`](plan-admin-client.md) · [`spec-admin-client.md`](spec-admin-client.md)
> **Last updated**: 2026-07-25

Each task maps to one vertical slice and is sized S/M. Acceptance criteria are
testable in a single command. Strike-through = done.

## Phase 1 — Backend

### ~~Task 1.1: Public image-byte route~~ ✅
- ~~`GET /i/*key` serves raw bytes, no auth~~ → `crates/api/src/handlers/public.rs`
- ~~Dependency-free Content-Type sniffing (JPEG/PNG/GIF/WebP/AVIF)~~
- ~~`Cache-Control: public, max-age=31536000, immutable`~~
- **Verify**: `cargo test -p picroom-api -- handlers::public`

### ~~Task 1.2: Image link + file redirect endpoints~~ ✅
- ~~`GET /api/v1/images/:id/link` → `{ public_url, expires_at: null }`~~
- ~~`GET /api/v1/images/:id/file` → 302 to public URL~~
- ~~Honors `server.public_url_base` when set (absolute) else path-relative~~
- **Verify**: `cargo test -p picroom-api`

### ~~Task 1.3: `public_url_base` config~~ ✅
- ~~`AppState::public_url_base: Option<String>`~~
- ~~Builder method `with_public_url_base`~~
- ~~Loaded from `[server]` in `infra/src/config.rs`~~
- **Verify**: `cargo test -p picroom-api -- state::tests::{public_url_base_defaults_to_none,with_public_url_base_sets_field}`

### ~~Task 1.4: UserRepository admin methods~~ ✅
- ~~`UserRepository::list(PageReq) -> Page<User>`~~
- ~~`UserRepository::find_by_id(UserId) -> Option<User>`~~
- ~~`UserRepository::set_disabled(UserId, bool)`~~
- ~~PostgreSQL implementations in `repo.rs`~~
- **Verify**: `cargo test -p picroom-service`

### ~~Task 1.5: Admin user handlers~~ ✅
- ~~`GET /api/v1/admin/users` (paginated, `User/Admin` RBAC)~~
- ~~`POST /api/v1/admin/users/:id/disable`~~
- ~~`POST /api/v1/admin/users/:id/enable`~~
- ~~`PATCH /api/v1/admin/users/:id/role`~~
- **Verify**: `cargo test -p picroom-api`; OpenAPI §`/admin/users/*`

### ~~Task 1.6: Team listing + members~~ ✅
- ~~`GET /api/v1/teams`~~
- ~~`GET /api/v1/teams/:id/members`~~
- **Verify**: `cargo test -p picroom-api`
- **Correction (2026-10-07, review-v1.0 R-13)**: this task originally claimed
  team listing shipped with `Team/Read` RBAC. It did not — both endpoints
  discarded the caller identity. As of the v1.0 remediation, `GET /teams`
  returns the caller's teams (managers/admins see all), and `GET
  /teams/:id[/members]` requires membership or `Team/Read`; non-members get
  404.

### ~~Task 1.7: Storage-policy management~~ ✅
- ~~`GET /api/v1/admin/storage/policies`~~
- ~~`POST /api/v1/admin/storage/policies`~~
- **Verify**: `cargo test -p picroom-api`

### ~~Task 1.8: OpenAPI snapshot~~ ✅
- ~~All new routes documented in `docs/api/openapi.yaml`~~
- **Verify**: 21 paths under `/api/v1/*` + `/i/{key}`

## Phase 2 — Client

### ~~Task 2.1: Tauri scaffold (standalone Cargo project)~~ ✅
- ~~`desktop/` with Vite + Vue 3 + TS~~
- ~~`desktop/src-tauri/Cargo.toml` carries `[workspace]` empty-table marker~~
- ~~`cargo deny` does NOT include this graph~~
- **Verify**: `desktop/src-tauri/Cargo.toml:36`; root `Cargo.toml` excludes `desktop/`

### ~~Task 2.2: Tauri plugins wired~~ ✅
- ~~`tauri-plugin-{http,store,dialog,fs,clipboard-manager,opener}` registered~~
- **Verify**: `desktop/src-tauri/src/lib.rs`

### ~~Task 2.3: Auth command layer~~ ✅
- ~~Commands: `login`, `logout`, `get_session`, `list_profiles`, `save_profile`, `set_active_profile`, `remove_profile`~~
- ~~Token + profile persisted via `tauri-plugin-store`~~
- **Verify**: `cargo test --manifest-path desktop/src-tauri/Cargo.toml -- commands::auth`

### ~~Task 2.4: Upload command~~ ✅
- ~~`upload_file(file_path, team_id?, on_progress)` streams multipart via `reqwest`~~
- ~~Progress events emitted through a Tauri `Channel`~~
- **Verify**: `cargo test --manifest-path desktop/src-tauri/Cargo.toml -- commands::upload`

### ~~Task 2.5: Download command~~ ✅
- ~~`download_image(image_id, save_path)` streams to disk~~
- ~~Resolves both absolute and path-relative public URLs~~
- **Verify**: `cargo test --manifest-path desktop/src-tauri/Cargo.toml -- commands::download`

### ~~Task 2.6: Frontend API client~~ ✅
- ~~`api/client.ts` wraps `@tauri-apps/plugin-http` `fetch`~~
- ~~Injects `Authorization: Bearer <token>`~~
- ~~On `401`: clears token, throws `ApiError(401, ...)`~~
- **Verify**: `cd desktop && npm run test -- client`

### ~~Task 2.7: DTO modules~~ ✅
- ~~`api/{images,users,teams,audit,storage,types}.ts` mirror OpenAPI DTOs~~
- **Verify**: `cd desktop && npm run lint`

### ~~Task 2.8: Login + auth guard~~ ✅
- ~~`LoginView.vue` collects `{ server_url, email, password }`~~
- ~~Pinia `stores/auth.ts` holds session + token~~
- ~~Router guard redirects unauthenticated users to `/login`~~
- **Verify**: `cd desktop && npm run build` (vue-tsc clean)

### ~~Task 2.9: ImagesView (core image flow)~~ ✅
- ~~List images (paginated `n-data-table`)~~
- ~~UI split into `src/components/{UploadDropzone,ImageGrid,CopyLinkButton}.vue`; `ImagesView.vue` is a thin composition root~~ (2026-07-25)
- ~~Drag-drop upload via `webview.onDragDropEvent`~~
- ~~"Copy link" button → `writeText` to clipboard~~
- ~~"Download" button → native `save()` dialog → `invoke('download_image')`~~
- ~~"Delete" button → `DELETE /api/v1/images/:id`~~
- **Verify**: `cd desktop && npm run lint`

### ~~Task 2.10: Admin screens~~ ✅
- ~~`UsersView.vue` — list, change role, disable/enable~~
- ~~`TeamsView.vue` — list teams + members~~
- ~~`StorageView.vue` — list/create storage policies~~
- ~~`AuditView.vue` — page through audit log~~
- **Verify**: `cd desktop && npm run lint`

### ~~Task 2.11: SettingsView (multi-profile)~~ ✅
- ~~List saved profiles, activate, delete~~
- ~~Active profile indicator~~
- **Verify**: `cd desktop && npm run lint`

### ~~Task 2.12: App shell~~ ✅
- ~~Naive UI `n-config-provider` + `n-message-provider` + `n-dialog-provider`~~
- ~~Sidebar menu + logout~~
- **Verify**: `cd desktop && npm run build`

## Phase 3 — Polish (Definition of Done)

### ~~Task 3.0: Spec-alignment refactor~~ ✅ (2026-07-25)
- ~~Extract `src/components/{UploadDropzone,ImageGrid,CopyLinkButton}.vue` from `ImagesView.vue`~~
- ~~Split `desktop/src-tauri/src/{error,state,store}.rs`; delete `config.rs`~~
- ~~`link`/`file` RBAC corrected to `Image/Read` (spec §4.1); add positive viewer test, negative test now uses empty-scope token~~
- ~~`auth.rs::login` profile name derived from server host (multi-profile)~~
- **Verify**: `cargo test -p picroom-api` (41 pass) · `npm run lint` + `npm test` (2/2) · `cargo clippy`/`test` on src-tauri (4/4)

### Task 3.1: Workspace fmt clean ⏳
- [ ] `cargo fmt --all -- --check` exits 0
- **Plan**: single `cargo fmt --all` run; review diff; commit
- **Files likely touched**: ~30 across `crates/{admin,api,auth,infra,s3compat,service,storage}`
- **Verify**: `cargo fmt --all -- --check`

### Task 3.2: ADR reference correction ⏳
- [ ] `crates/api/src/handlers/public.rs:10` says "ADR 0007" → "ADR 0008"
- [ ] `crates/api/src/router.rs:20` says "ADR 0007" → "ADR 0008"
- **Why**: ADR-0007 is security hardening; the public-link decision lives in ADR-0008
- **Verify**: `rg "ADR 0007" crates/api/src` returns nothing

### ~~Task 3.3: Plan + tasks docs committed~~ 🟦 (this PR)
- ~~`docs/plan-admin-client.md` exists~~
- ~~`docs/tasks-admin-client.md` exists (this file)~~
- **Verify**: `git diff --stat`

### Task 3.4: Tauri Windows installer via GitHub Actions ⏳
- [ ] Push a `v*.*.*` tag → `release.yml` `desktop` job builds NSIS + MSI on `windows-latest` and attaches both to the GitHub Release
- [ ] Authenticode signing active once `WINDOWS_CERTIFICATE` (base64 `.p12`) + `WINDOWS_CERTIFICATE_PASSWORD` repo secrets are set; absent → unsigned build (SmartScreen warns)
- **Verify**: `gh release view <tag>` lists `*.exe` (NSIS) + `*.msi` under Assets
- **Prereq**: enable the workflow in repo **Actions settings** (was disabled — `ci.yml` had 0 runs) and add the signing secret
- **Unblocked**: no local Windows SDK — `windows-latest` provides VS Build Tools + Windows SDK + WiX/NSIS; `swatinem/rust-cache` + npm cache cut the ~10 min first build

### Task 3.5: End-to-end smoke (manual) ⏳
Prereqs: a running **Docker daemon** + a **display** (for the Tauri GUI). Cannot
run in CI/sandbox — this env has no Docker daemon and no `DISPLAY`. CLI surface
verified 2026-07-25 against `target/release/picroom(.exe)`.
- [ ] `docker compose -f docker/docker-compose.yml up -d`
      (the `picroom-migrate` service auto-runs `admin migrate run`; wait for `picroom-api` healthy)
- [ ] Create the admin user (the compose `api` container already has `PICROOM_DATABASE__URL`):
      `docker compose exec api picroom admin user create --email admin@example.com --name Admin --password "smoke-pass-123" --role admin`
      (requires `--name` + `--password`; `--role` defaults to `viewer`)
- [ ] `cd desktop && npm run tauri dev`  (Windows binary is `picroom.exe`)
- [ ] Log in at server URL `http://localhost:8080` (email/password above)
- [ ] Upload, copy 公链, open in private window → image renders
- [ ] Download to a chosen folder → bytes match
- [ ] List users, change a role, disable + enable
- [ ] List teams + members; list/create storage policies
- [ ] Page through audit log
- [ ] Force a 401 (expire token) → redirected to login, token cleared
- **Blocked on**: Docker + GUI display on the local machine; human verification

## Out-of-scope (deliberately deferred)

| Item | Source | Rationale |
|---|---|---|
| `tauri-plugin-notification` | spec §2.1 | Naive UI `useMessage` already provides in-app toasts |
| Refresh-token rotation | spec §1.4 | Post-MVP; client re-logs-in on 401 |
| `is_public` per-image flag | spec §11 OQ, ADR-0008 §Consequences | URL = capability model is the v1 contract |
| Public-read audit logging | ADR-0008 §Consequences | Volume; deferred until abuse detection needed |
| Tauri updater plugin | spec §11 OQ5 | Post-MVP |
| Mobile (iOS/Android) builds | spec §1.4 | Desktop (Windows first) only |
