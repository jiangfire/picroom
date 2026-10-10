# Tasks: Post-1.1.1 — Coverage, Hygiene, Spec Gaps

> **Status**: Drafted 2026-10-09
> **Parent**: [`plan-post-1.1.1.md`](plan-post-1.1.1.md) · **Prior**:
> v1.0 remediation tasks (P0–P3 ✅; P4 continues here)
> **Last updated**: 2026-10-09

Rule (inherited D-12): every task ships with the test that fails without its
fix. Legend: ⬜ not started · 🔄 in progress · ✅ done · 🔲 deliberately deferred

---

## Phase P4 — Coverage 75.32 % → 80 %

### ⬜ Task 4.A: Per-crate coverage visibility (D-13)
- CI coverage step parses `target/coverage/cobertura.xml`, prints one
  `crate: covered/total, %` line per workspace crate, uploads the XML artifact
- **Verify**: coverage job log shows the table; record numbers below
- **Files**: `.github/workflows/ci.yml`

### ⬜ Task 4.B: `crates/admin` → 80 %
- CLI error paths + exit codes, `config validate` semantic branches,
  `migrate` status classification
- **Verify**: per-crate table ≥ 80 %

### ⬜ Task 4.C: `crates/audit` → 80 %
- DB sink error paths, reader `limit`/`before` edges, redaction
- **Verify**: per-crate table ≥ 80 %

### ⬜ Task 4.D: `crates/imaging` → 80 %
- 1-px / extreme-aspect resize, probe rejections, quality clamps
- **Verify**: per-crate table ≥ 80 %

### ⬜ Task 4.E: `crates/api` handler branches
- 403/404 mapping on all routes, multipart error paths, admin validation
- **Verify**: per-crate table ≥ 80 %

### ⬜ Task 4.F: remaining crates to the floor
- s3compat ~91 % already; top up worker/service/storage/domain per the 4.A table
- **Verify**: per-crate table ≥ 80 % everywhere

### ⬜ Task 4.G: flip the gate (D-14)
- `--fail-under 80` + per-crate floors in `.github/workflows/ci.yml`;
  update `spec.md` §1.4 S5 note and the job header comment
- **Verify**: gate green on two consecutive master runs before merging

### Checkpoint P4
- [ ] Per-crate table in CI log; every crate ≥ 80 %; gate at 80 %
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

### ⬜ Task H3: ListObjectsV2 cross-driver-page pagination
- Loop `storage().list(next_cursor)` until `max_keys`/exhausted; `IsTruncated`
  from the loop; test with a fake store returning 2 pages
- **Verify**: `cargo test -p picroom-s3compat` + new pagination test

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
