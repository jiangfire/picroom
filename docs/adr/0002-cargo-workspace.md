# ADR-0002: Adopt Cargo Workspace with multiple crates

- **Status**: Accepted
- **Date**: 2026-07-05
- **Deciders**: Picroom maintainers

## Context

Picroom has many distinct concerns: API, services, domain, storage, imaging,
auth, audit, worker, S3-compat, infra, admin. A monolithic crate would be
hard to navigate and test in isolation. A separate repository per crate would
fragment the codebase.

## Decision

We adopt a **Cargo Workspace** with 11 member crates + 1 binary crate:

```
crates/
  api/         service/    domain/      storage/
  imaging/     auth/       audit/       s3compat/
  worker/      infra/      admin/
bin/
  picroom/                  # single binary entry
```

Dependency rules (current, verified from each crate's `Cargo.toml`):

```
domain    ← (only std + thiserror + optional serde)
storage   ← domain
imaging   ← domain
auth      ← domain
audit     ← domain
infra     ← domain
worker    ← domain, storage, imaging, audit, infra
service   ← domain, storage, imaging, auth, audit, worker
s3compat  ← domain, storage, auth, audit, service
admin     ← domain, infra, storage, auth, audit
api       ← domain, service, auth, audit, infra, storage, s3compat, worker
picroom   ← all eleven internal crates (composition root)
```

Forbidden:

- `domain` depending on anything except std + thiserror (+ optional serde).
- `service` depending on `api`.
- `storage` driver depending on `api`.
- Any circular dependency between crates.

> **Layering refinements since the original decision.** `service` now depends
> on `worker` (it composes the worker's enqueue surface into the upload use
> case) and intentionally does **not** depend on `infra` — it reaches
> persistence via repository traits + `sqlx` instead. `admin` additionally
> depends on `storage`/`auth`/`audit` so the CLI can run `storage-test`, role
> management, and `audit tail`. The original "service must not depend on
> worker" rule is therefore relaxed.

These rules are intended to be enforced by an integration test that runs
`cargo metadata` and asserts the dependency graph; that test is **not yet
implemented** (tracked follow-up). Until it lands, the graph above is the
authoritative reference and is reconciled against `docs/spec.md` §4.1.

## Consequences

### Positive

- Compile time: changes to `domain` rebuild only what depends on it; isolated
  crates compile fast individually.
- Testability: each crate has its own `tests/` directory; integration tests
  can use any subset.
- Clear ownership boundaries; reinforces single-responsibility per SOLID §S.
- Shared `[workspace.dependencies]` keeps version pins consistent.

### Negative

- More `Cargo.toml` files to maintain; mitigated by `cargo xtask` patterns
  (post-MVP).
- Mental overhead for newcomers to understand boundaries; mitigated by
  explicit dependency diagram in this ADR + spec §4.1.

### Neutral

- We will not publish crates to crates.io in v1; workspace is for in-repo
  modularity, not external distribution.

## References

- [Cargo Workspaces](https://doc.rust-lang.org/cargo/reference/workspaces.html)
- Internal: `docs/spec.md` §4