# Tasks: Post-1.1.1 — Coverage, Hygiene, Spec Gaps

> **Status**: Drafted 2026-10-09
> **Parent**: [`plan-post-1.1.1.md`](plan-post-1.1.1.md) · **Prior**:
> v1.0 remediation tasks (P0–P3 ✅; P4 continues here)
> **Last updated**: 2026-10-10

Rule (inherited D-12): every task ships with the test that fails without its
fix. Legend: ⬜ not started · 🔄 in progress · ✅ done · 🔲 deliberately deferred

---

## Phase P4 — Coverage 75.32 % → 80 %

### ✅ Task 4.A: Per-crate coverage visibility (D-13)
- CI coverage step parses `target/coverage/cobertura.xml`, prints one
  `crate: covered/total, %` line per workspace crate (weakest first), and
  uploads the XML artifact
- **Verify**: confirmed on CI run 38056694504 — the table printed, and its
  sum matched tarpaulin's own total (5582/7252 both ways), so the crate
  attribution holds against real `strip_base_dir` output. Numbers recorded in
  `plan-post-1.1.1.md` §4
- **Files**: `.github/workflows/ci.yml`

### 🔄 Task 4.B: `crates/admin` → 80 % *(CI: 75.98 % after the work below, +20 to go)*
- Added: audit tail's SQLite read + corrupt-row tolerance, its missing-table
  error path, `open_pool` scheme dispatch (SQLite accept / unknown scheme),
  `migrate status` against an unmigrated DB, and the three missing
  `config validate` guards (`max_connections`, `quality.webp`,
  `jwt_ttl_secs`) — 27 tests, clippy/fmt clean
- The `*_pg` functions (`team_create_pg`, `user_list_pg`, `audit_list_pg`, …)
  are now reachable: the coverage job has a migrated PostgreSQL. Writing those
  tests is what closes the last ~20 lines
- **Verify**: per-crate table ≥ 80 %

### 🔄 Task 4.C: `crates/audit` → 80 % *(CI: 41.98 % — the worst crate)*
- DB sink error paths, reader `limit`/`before` edges, redaction
- Needs ~+31 lines. A local cargo-llvm-cov reading suggested only ~5 were
  needed; the CI table is the one that counts
- **Verify**: per-crate table ≥ 80 %

### ✅ Task 4.D: `crates/imaging` → 80 % *(CI: 95.24 % — no backfill needed)*
- The plan assumed imaging was ~56 % weak; it clears the floor comfortably, so
  the resize/probe/quality-clamp backfill is dropped
- **Verify**: per-crate table ≥ 80 %

### ✅ Task 4.E: `crates/api` handler branches *(CI: 85.56 % — already above)*
- No backfill required; 403/404 mapping, multipart error paths and admin
  validation are already covered
- **Verify**: per-crate table ≥ 80 %

### ⬜ Task 4.F: remaining crates to the floor
- **service 56.21 % is the whole remaining problem (~+314 lines)** — it was not
  in the original list at all
- small top-ups: storage +19, admin +20, infra +10
- s3compat, domain, worker, auth already clear the floor
- **Verify**: per-crate table ≥ 80 % everywhere

> Numbers: `plan-post-1.1.1.md` §4 (CI run 38056694504). A local
> cargo-llvm-cov run disagreed sharply with tarpaulin (audit 76.81 % vs
> 41.98 %, api 71.42 % vs 85.56 %) — target off the CI table, not the proxy.

### ⬜ Task 4.G: flip the gate (D-14)
- `--fail-under 80` + per-crate floors in `.github/workflows/ci.yml`;
  update `spec.md` §1.4 S5 note and the job header comment
- **Verify**: gate green on two consecutive master runs before merging

### Checkpoint P4
- [x] Per-crate table in CI log (confirmed, run 38056694504)
- [ ] every crate ≥ 80 % — outstanding: audit, service, infra, admin, storage
- [ ] gate at 80 %
- [ ] `spec.md` S5 matches reality; remediation P4 fully closed

---

## Phase H — Hygiene

### ⬜ Task H1: SQLite posture documented (D-15)
- Warning in `deployment.md` + `README.md`: SQLite mode = dev only, no
  repositories/auth enforcement; production requires PostgreSQL
- **Verify**: docs review; no doc sentence claims SQLite auth

### ⬜ Task H2: `require_sessions` cutover runbook
- `docs/operations.md`: after one JWT TTL, set `[auth].require_sessions = true`;
  note the one-time mass-401 risk and how to stage it
- **Verify**: docs review

### ✅ Task H3: ListObjectsV2 cross-driver-page pagination
- The truncation was one layer lower than this task assumed. `s3compat` pages
  correctly over whatever `storage().list()` returns, but `StorageLister::list`
  has no cursor parameter — so the paging had to happen inside the driver.
  `S3Driver::list` (which `MinioDriver` aliases) issued a **single**
  `list-type=2` request with no `continuation-token` and no `max-keys`, and
  never parsed `IsTruncated`. S3 caps a response at 1000 keys, so everything
  past that was silently dropped — from the admin listing, the worker and
  `aws s3 ls` alike. `LocalDriver` returns everything in one page, which is why
  local deployments never showed it.
- Now walks `NextContinuationToken` until the backend stops claiming truncation,
  returning one complete `Page` (what the trait promises — matches `LocalDriver`,
  no trait change, no new dependency). Stops early if a backend repeats a
  non-advancing token rather than spinning to the page cap.
- Parsing is now whole-document instead of line-oriented: a line-based scrape
  read **nothing** from a single-line response body, so it would also have
  missed `IsTruncated` on backends that do not pretty-print.
- **Verify**: `cargo test -p picroom-storage --test s3` — two wiremock tests
  (two-page walk, non-advancing token); both fail against the old driver

### ⬜ Task H4: ListObjectsV2 `delimiter`
- Implement `CommonPrefixes` grouping, or answer `NotImplemented` when
  `delimiter` is present — no silent ignore
- **Verify**: new test; `aws s3 ls` behavior documented

### ⬜ Task H5: LocalDriver temp-file sweep
- On construction, best-effort delete dot-prefixed `*.tmp` older than 24 h
- **Verify**: unit test with an aged fixture file

### ⬜ Task H6: spec §10.1 role wording (D-17)
- Built-in four roles are the only roles; drop the `team.invite` /
  custom-roles implication (pairs with Task G2)
- **Verify**: docs review

### ⬜ Task H7: expired-session fidelity
- `MemSessions` honors `expires_at`; new test: expired-but-unrevoked → 401
- **Verify**: `cargo test -p picroom-api --test p1_authz`

### Checkpoint H
- [ ] All merged; `cargo test --workspace` green; CI green

---

## Phase G — Spec gaps: implement or descope

### ⬜ Task G1: auth rate limiting (OQ-6, D-16)
- `[rate_limit]` config block; fixed-window per-IP+email counters on
  `POST /api/v1/auth/login` and OIDC start; `429` + `Retry-After`
- Tests: threshold trip, window reset, per-IP isolation, bypass attempt
- **Verify**: `cargo test -p picroom-api`; OpenAPI `429` on login is now true

### ⬜ Task G2: custom roles descope (D-17)
- spec §10.1 + ADR-0005: built-in four roles are v1's only roles
- **Verify**: docs review

### ⬜ Task G3: SQLite layer descope (D-15)
- Covered by H1 + a statement in `spec.md`
- **Verify**: docs review

### Checkpoint G
- [ ] No sentence in the repo claims an unimplemented feature

---

## Lanes

| Lane | Tasks | Constraint |
|---|---|---|
| A | 4.A → 4.B–4.F → 4.G | Sequential |
| B | H3, H4, H5 | Independent |
| C | H1, H2, H6, H7, G2, G3 | Any time |
| D | G1 | Independent |
