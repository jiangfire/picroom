# Coverage Remediation Plan

Spec §1.4 S5 calls for **≥ 80 % line coverage** workspace-wide. The picture
in practice is uneven per crate — see the baseline table below — so CI
enforces an **interim floor** that is the current workspace-weighted line
coverage (binaries under `bin/` and the `service` crate excluded from the
denominator) and is raised step-by-step as coverage grows.

## Latest baseline

Tarpaulin-style line coverage from `cov_baseline.log` (column = lines hit
percent). The `service` crate is excluded because its tests need a running
PostgreSQL fixture; `bin/picroom` is thin CLI wiring and is not unit-tested.

| Crate           | Lines hit | Notes                                              |
|-----------------|----------:|----------------------------------------------------|
| `picroom-s3compat`   | **91.42 %** | SigV4 verifiers are fully unit-tested             |
| `picroom-domain`     | **81.45 %** | above spec target                                 |
| `picroom-infra`      | **75.37 %** | config / logging / telemetry                      |
| `picroom-auth`       | **69.78 %** | JWT + RBAC + Argon2id                             |
| `picroom-worker`     | **68.07 %** | job queue + processor                             |
| `picroom-storage`    | **67.65 %** | Local + S3 + MinIO contract-tested                |
| `picroom-imaging`    | **55.95 %** | AVIF / WebP / thumbnail encoders                  |
| `picroom-audit`      | **52.55 %** | event / sink / reader                             |
| `picroom-api`        | **54.17 %** | axum router + handlers (large surface)            |
| `picroom-admin`      | **47.58 %** | CLI subcommands (SQLite + PG paths)               |
| `picroom-service`    | —          | excluded; needs PG test fixture (`#[sqlx::test]`) |
| `bin/picroom`        | —          | excluded; thin CLI wiring                        |

Use this table (and the Codecov report) as the source of truth for per-file
gaps rather than the aggregate line number.

## Strategy

Raise the floor in steps as each area is covered:

| Floor  | Trigger                                                |
|--------|--------------------------------------------------------|
| 60 %   | current (1.0.0 release)                                |
| 65 %   | after closing the `service` PG round-trip work items   |
| 70 %   | after `api` handlers + storage drivers gain integration |
| 80 %   | after `imaging` + `audit` golden tests — spec target   |

## Work items (highest uncovered line counts first)

- [ ] `crates/service/src/repo.rs` — unit-test cursor encode/decode and the
      PG implementations against `#[sqlx::test]` (the biggest single source
      of `service` excluding it from the denominator).
- [ ] `crates/api/src/handlers/{images,teams,admin}.rs` — extend the
      existing `api/tests/api.rs` harness with image ACL, team + admin flows.
- [ ] `crates/storage/src/driver/{s3,minio}.rs` + `any.rs` — contract tests
      against a fake `Storage` implementation.
- [ ] `crates/admin/src/{user,team,audit_cmd,storage_test}.rs` — smoke-test
      each CLI subcommand with a `:memory:` SQLite pool (no `unwrap` in
      production path; tests are fine).
- [ ] `crates/imaging/src/processor/{avif,webp,thumbnail}.rs` — golden
      output tests against checked-in fixtures under
      `tests/fixtures/images/`.
- [ ] `crates/audit/src/{sink,db_sink,reader}.rs` — cover event ordering,
      idempotency, and retention sweeps.
- [ ] `crates/worker/src/{db_queue,pool,dlq}.rs` — exercise the job queue
      against in-memory SQLite (already wired in `worker/tests/db_queue.rs`).
- [ ] `bin/*` — optionally lift the `bin/**` exclusion once the CLI paths are
      smoke-tested (e.g. `--help`/`--version` and a config-driven dry run).

## Notes

- The `coverage` job uploads to Codecov; treat the Codecov report as the
  source of truth for per-file gaps, not just the aggregate %.
- Do not raise the floor past what the suite currently sustains; each bump
  must be paired with the corresponding tests above.
- `service` stays excluded from the workspace total until the PG test
  fixture is wired into CI; once that gate lands the spec-target path
  (≥ 80 %) becomes reachable in two or three sprints.