# Coverage Remediation Plan

Spec §1.4 S5 calls for **≥ 80 % line coverage**. As of 2026-07-11 the
workspace measures **64.81 %** (tarpaulin, 2400/3703 lines, `bin/**`
excluded). Blocking every merge on an unmet 80 % target is counter-
productive, so the CI `coverage` job enforces an **interim floor of
60 %** (binaries under `bin/` excluded from the denominator — they are
thin CLI wiring, not unit-tested) while we grow coverage toward the spec
target.

## Strategy

Raise the floor in steps as each area is covered:

| Floor | Trigger |
|-------|---------|
| 60 %  | current (1.0.0 release) |
| 65 %  | after s3compat + storage driver tests |
| 80 %  | after api/worker/infra tests — spec target |

## Work items (highest uncovered line counts first)

- [ ] `crates/service/src/repo.rs` (0/122) — unit-test cursor encode/decode
      and `PgImageRepository` against a test Postgres (`#[sqlx::test]`).
- [ ] `crates/s3compat/src/object.rs` (0/55) + `list.rs` (0/24) +
      `multipart.rs` (0/15) — signed-request round-trip + error-path tests
      against a local MinIO or the `Storage` trait fake.
- [ ] `crates/storage/src/driver/{s3,minio}.rs` + `any.rs` (0/32) — contract
      tests against a fake `Storage` implementation.
- [ ] `crates/api/src/handlers/{teams,admin}.rs` (0/39, 0/24) — extend the
      existing `api/tests/api.rs` harness with team + admin flows.
- [ ] `crates/worker/src/{db_queue,pool,dlq}.rs` — exercise the job queue
      against in-memory SQLite (already wired in `worker/tests/db_queue.rs`).
- [ ] `crates/infra/src/{cache,db,logging}.rs` — cover config loading and
      cache policies.
- [ ] `bin/*` — optionally lift the `bin/**` exclusion once the CLI paths are
      smoke-tested (e.g. `--help`/`--version` and a config-driven dry run).

## Notes

- The `coverage` job uploads to Codecov; treat the Codecov report as the
  source of truth for per-file gaps, not just the aggregate %.
- Do not raise the floor past what the suite currently sustains; each bump
  must be paired with the corresponding tests above.
