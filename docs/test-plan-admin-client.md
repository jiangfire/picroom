# Test Plan: Picroom Tauri Admin Client

> **Status**: Active · **Parent**: [`spec-admin-client.md`](spec-admin-client.md) §8, §10
> **Companion**: [`plan-admin-client.md`](plan-admin-client.md) · [`tasks-admin-client.md`](tasks-admin-client.md)
> **Last updated**: 2026-07-19

## 1. Purpose

Close out the Definition-of-Done for ADR-0008 (spec §10, C1–C11). The plan
reuses the existing 38 axum integration tests where possible, names the gaps
explicitly, and orders execution from cheapest automation to dearest manual
smoke.

Coverage reference: spec §8.1 (server), §8.2 (client), §8.3 (thresholds).

## 2. Tiers

| Tier | What | Cost | When |
|:-:|---|---|---|
| 1 | Automated unit + integration (existing + gaps) | Seconds | Every commit |
| 2 | Integration against real PG + Tauri build | Minutes | Pre-release / nightly |
| 3 | End-to-end manual smoke against docker stack | Human | Pre-release |
| 4 | Regression / boundary / abuse | Human | Time-permitting |

## 3. Tier 1 — Automated

### 3.1 Backend workspace (`cargo test --workspace`)

| § | Scenario | DoD | Existing test | Status |
|:-:|---|:-:|---|:-:|
| 8.1 | `/i/:key` 200 + correct Content-Type | C1 | `public_route_serves_png_bytes_without_auth`, `public_route_sniffs_jpeg_content_type` | ✅ |
| 8.1 | `/i/:key` 404 on missing object | C1 | `public_route_returns_404_for_missing_object` | ✅ |
| 8.1 | `/i/:key` 400 on malformed key | C1 | `public_route_returns_400_for_bad_key` | ✅ |
| 8.1 | `/api/v1/*` still requires auth (regression) | C1 | `api_rejects_missing_token`, `api_rejects_forged_token`, `api_rejects_token_signed_with_wrong_secret` | ✅ |
| 8.1 | `/images/:id/link` absolute URL | C2 | `image_link_returns_absolute_public_url` | ✅ |
| 8.1 | `/images/:id/link` relative URL when base unset | C2 | `image_link_returns_relative_url_when_base_unset` | ✅ |
| 8.1 | `/images/:id/link` IDOR denies viewer | C2 | `image_link_forbids_viewer_accessing_others_image` | ✅ |
| 8.1 | `/images/:id/file` 302 redirect | — | `image_file_redirects_to_public_url`, `image_file_returns_404_for_unknown_image` | ✅ |
| 8.1 | admin users list + RBAC | C3 | `admin_list_users_returns_users_for_admin`, `admin_list_users_forbids_non_admin` | ✅ |
| 8.1 | admin disable/enable user | C3 | `admin_disable_user_returns_204`, `admin_disable_user_rejects_invalid_id` | ✅ |
| 8.1 | teams list + members | C3 | `teams_list_returns_all_teams`, `team_members_list_returns_members` | ✅ |
| 8.1 | storage policy CRUD + RBAC | C3 | `storage_list_returns_policies_for_admin`, `storage_create_returns_201_for_admin`, `storage_list_forbids_non_admin` | ✅ |

**Gap to close**: `public_route_returns_400_for_bad_key` — assert that
`GET /i/../etc/passwd` (or any key failing `StorageKey::parse`) returns 400,
not 404/500.

### 3.2 Backend static checks

| Check | Command | DoD |
|---|---|:-:|
| Format | `cargo fmt --all -- --check` | C4 |
| Lint | `cargo clippy --all-targets --all-features -- -D warnings` | C4 |

### 3.3 Desktop frontend (`cd desktop && npm run test`)

| § | Scenario | Existing test | Status |
|:-:|---|---|:-:|
| 8.2 | 401 clears token + throws `ApiError(401)` | `client.test.ts` | ✅ |
| 8.2 | Bearer header injected on every call | `client.test.ts` | ✅ |
| 8.2 | Each `api/*.ts` module has ≥1 test | — | ❌ **gap** |
| 8.2 | `stores/auth.ts` state machine (login/logout/restore) | — | ❌ **gap** |

Lint: `npm run lint` (vue-tsc --noEmit) — must be clean (C11).

### 3.4 Desktop Rust command layer (`cd desktop && cargo test --manifest-path src-tauri/Cargo.toml`)

| § | Scenario | Existing test | Status |
|:-:|---|---|:-:|
| 8.2 | login 200 returns token | `perform_login_returns_token` | ✅ |
| 8.2 | login 401 propagates status | `perform_login_propagates_error_status` | ✅ |
| 8.2 | upload streams multipart | `perform_upload_streams_file_to_server` | ✅ |
| 8.2 | download resolves absolute + relative URLs | `perform_download_streams_absolute_and_relative_urls` | ✅ |
| 8.2 | upload/download progress events fire on Channel | — | ❌ **gap** |

## 4. Tier 2 — Integration / Build

### 4.1 PostgreSQL round-trip

Repo methods that have in-memory tests but no PG round-trip yet:

- `UserRepository::{list, find_by_id, set_disabled, set_role}`
- `TeamRepository::{list_members}`
- `StoragePolicyRepository::{list, create}`

Precondition: `docker compose -f docker/docker-compose.yml up -d postgres`.

Run: `cargo test --workspace --features postgres-integration` (confirm exact
feature gate before running).

### 4.2 OpenAPI snapshot consistency

- `docs/api/openapi.yaml` covers all 9 spec §4.1 routes — ✅ verified (21
  `/api/v1/*` paths plus `/i/{key}`).
- Schema-vs-response field consistency: no automated check yet; deferred.

### 4.3 Tauri build (C5)

- `cd desktop && npm run tauri build` succeeds.
- Produces MSI/NSIS under `desktop/src-tauri/target/release/bundle/`.
- WebView2 runtime bootstraps correctly (manual install if needed).

Estimated cost: ~10 min compile + ~1 min bundle. Risk: Windows SDK version,
code-signing cert.

**Run 2026-07-19**: ✅ Both installers produced in 53 s (release profile warm).
- MSI: `target/release/bundle/msi/Picroom Admin_0.1.0_x64_en-US.msi` (6.06 MB)
- NSIS: `target/release/bundle/nsis/Picroom Admin_0.1.0_x64-setup.exe` (4.15 MB)

## 5. Tier 3 — End-to-end Manual Smoke

Precondition:

```bash
docker compose -f docker/docker-compose.yml up -d
./target/release/picroom admin migrate
./target/release/picroom admin user create --email admin@example.com --role admin
# set server.public_url_base = "http://localhost:8080" in config
cd desktop && npm run tauri dev
```

### C6 — Core upload / public-link flow
- [ ] Login with `http://localhost:8080` + admin@example.com → lands on Images
- [ ] Drag a PNG onto the dropzone → progress toast → "Upload complete"
- [ ] Grid shows the new image with correct `width × height`, `bytes`, `content_type`
- [ ] "Copy link" puts the link endpoint's `public_url` into the clipboard
- [ ] Open the URL in a **private window** (no Authorization header) → image renders
- [ ] "Upload" button → file picker → pick a JPG → same happy path

### C7 — Download flow
- [ ] "Download" opens a native save dialog
- [ ] Save to disk → bytes match the source (`Get-FileHash` compare)

### C8 — Admin CRUD
- [ ] Users view: list, change role, disable, enable — all take effect
- [ ] Teams view: list + drill into members
- [ ] Storage view: list default policy, create a new policy → list refreshes
- [ ] Audit view: page through events from the operations above

### C9 — 401 redirect
- [ ] Corrupt the stored token (or wait for JWT expiry)
- [ ] Trigger any API call → 401
- [ ] Client clears the token and bounces to /login

### C9.1 — Multi-profile (spec §11 OQ3)
- [ ] Settings: save a second profile
- [ ] "Activate" → session switches to the new server
- [ ] "Delete" a non-active profile → list refreshes
- [ ] Delete the active profile → bounces to /login

## 6. Tier 4 — Regression / Boundary (time-permitting)

| Scenario | Why |
|---|---|
| `/i/../etc/passwd` path traversal | Safety regression |
| `/i/<very long key>` (>1 KB) | DoS guard |
| Upload >10 MB streams without JS heap growth | Memory guard for C5 |
| Desktop start with no server reachable | UX resilience |
| OIDC login → client persists token | Connects to `f1b93da` |

## 7. Execution Order

1. Close Tier 1.1 gap: add `public_route_returns_400_for_bad_key`.
2. Re-run `cargo test --workspace` → expect green.
3. Re-run Tier 1.2 (`cargo fmt --check`, `cargo clippy`) → expect green.
4. Run Tier 1.3 (`npm run test`, `npm run lint`) → expect green.
5. Run Tier 1.4 (`cargo test --manifest-path src-tauri/Cargo.toml`) → expect green.
6. Tier 2.1 PG round-trip — confirm feature name and run.
7. Tier 2.3 Tauri build — kick off, parallel with Tier 3 setup.
8. Tier 3 manual smoke — checkpoint per criterion.
9. Tier 4 as time permits.

## 8. Definition-of-Done rollup

| DoD | Where verified | Status |
|:-:|---|:-:|
| C1 | Tier 1.1 + Tier 3 C6 | ✅ (automated; manual smoke pending docker) |
| C2 | Tier 1.1 + Tier 3 C6 | ✅ (automated; manual smoke pending docker) |
| C3 | Tier 1.1 + Tier 3 C8 | ✅ (automated; manual smoke pending docker) |
| C4 | Tier 1.2 | ✅ |
| C5 | Tier 2.3 | ✅ (2026-07-19 run) |
| C6 | Tier 3 | pending docker |
| C7 | Tier 3 | pending docker |
| C8 | Tier 3 | pending docker |
| C9 | Tier 3 | pending docker |
| C10 | committed (ADR + OpenAPI) | ✅ |
| C11 | Tier 1.2 + Tier 1.3 | ✅ |

## 9. Run log

### 2026-07-19 — Tier 1 + Tier 2.3

| Step | Command | Result |
|---|---|---|
| 1.1 gap | added `public_route_returns_400_for_bad_key` | ✅ 1 new test, 4/4 public-route tests pass |
| 1.1 full | `cargo test --workspace` | ✅ 29 binaries, 0 failed (incl. new test) |
| 1.2 fmt | `cargo fmt --all -- --check` | ✅ exit 0 |
| 1.2 clippy | `cargo clippy --all-targets --all-features -- -D warnings` | ✅ exit 0 |
| 1.3 lint | `cd desktop && npm run lint` | ✅ exit 0 |
| 1.3 test | `cd desktop && npm run test` | ✅ 2/2 pass |
| 1.4 test | `cd desktop && cargo test --manifest-path src-tauri/Cargo.toml` | ✅ 4/4 pass |
| 2.1 PG | docker unavailable on host; deferred to CI | ⏸ blocked |
| 2.3 build | `cd desktop && npm run tauri build` | ✅ MSI 6.06 MB + NSIS 4.15 MB |

## 10. Decisions deferred (out of scope for this pass)

- Per-`api/*.ts`-module vitest coverage (Tier 1.3 gaps) — spec §8.3 has no hard
  threshold; revisit if a regression slips.
- Progress-channel event assertion for upload/download (Tier 1.4 gap) — current
  wiremock tests assert side effects (file on disk, server state), which cover
  the critical path.
- Automated OpenAPI-vs-response schema check (Tier 4.2) — post-MVP.
