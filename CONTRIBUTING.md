# Contributing to Picroom

Thanks for your interest in contributing! Picroom follows strict engineering
discipline: **spec first, test first, lint always.**

## Ground rules

1. **All code must be MIT-licensed**. By submitting a PR, you agree to the
   MIT License terms.
2. **All code must have tests**. We follow TDD: write the failing test first.
3. **All code must pass CI**: `fmt`, `clippy`, `test`, `audit`, `deny`,
   coverage ≥ 80 %.
4. **All public API changes need an ADR update** (or new ADR).
5. **No new dependency without a justification in the PR description** and an
   ADR.
6. **No `unwrap()` outside tests**.

## Workflow

1. **Pick an issue** (or open one). Tag it `good first issue` if appropriate.
2. **Discuss first** for non-trivial changes. Open an issue or a discussion
   before writing code; spec drift is expensive.
3. **Write a failing test** that captures the acceptance criteria.
4. **Implement** the smallest change that turns the test green.
5. **Refactor** while keeping tests green.
6. **Update docs** (`docs/spec.md`, ADRs, OpenAPI annotations).
7. **Open a PR** using the provided template.
8. **Address review feedback** until approvals.
9. **Squash-merge** once CI is green.

## Commit message format

We follow [Conventional Commits](https://www.conventionalcommits.org/):

```
task(<phase>-<id>): <short description>

<optional body explaining the why>

<optional footer with references>
```

Examples:

```
task(1-2): add StorageKey::parse validation

- Reject empty keys
- Reject path traversal (..)
- Reject keys longer than 1024 chars
```

```
fix(s3compat): verify SigV4 chunked uploads

The previous verifier assumed single-shot PUTs. Multipart part uploads
use `STREAMING-AWS4-HMAC-SHA256-PAYLOAD` token signing, which we now
handle correctly.

Closes #142
```

## Local development

```bash
# Toolchain
rustup toolchain install stable
rustup component add rustfmt clippy rust-analyzer

# Tools
cargo install sqlx-cli --no-default-features --features rustls,postgres
cargo install cargo-audit
cargo install cargo-deny
cargo install cargo-tarpaulin

# Database (dev)
docker compose -f docker/docker-compose.yml up -d postgres minio

# Run migrations
cargo sqlx migrate run

# Build & test
cargo build --workspace
cargo test --workspace

# Lints
cargo fmt --all
cargo clippy --all-targets --all-features -- -D warnings

# Coverage
cargo tarpaulin --workspace --fail-under 80
```

## Code style

- `cargo fmt` defaults.
- `clippy::pedantic` enabled; project-specific allow-list in `clippy.toml`.
- No `unwrap()` outside tests.
- Error handling via `Result<T, Error>` everywhere; no panics in libraries.
- Public items have doc comments with examples.
- No `unsafe` without an explicit justification comment.

See `docs/spec.md` §5 for full style guide and example code.

## Architectural Decision Records (ADR)

When your change introduces or replaces a major dependency, a new
abstraction, or a non-obvious design choice, add or update an ADR in
`docs/adr/`. Use the format:

```
# ADR-NNNN: <title>

- Status: Proposed | Accepted | Deprecated | Superseded by ADR-XXXX
- Date: YYYY-MM-DD
- Deciders: ...

## Context
[What is the issue we're seeing that's motivating this decision?]

## Decision
[What is the change we're proposing/doing?]

## Consequences
### Positive
### Negative
### Neutral
```

## Testing policy

- Unit tests in `#[cfg(test)] mod tests` per file.
- Integration tests in `<crate>/tests/`.
- E2E tests in top-level `tests/`, gated by `--features e2e`.
- Coverage ≥ 80 % per crate; 100 % for `domain`.
- Property-based tests with `proptest` for parsers and serializers.
- Golden / snapshot tests for OpenAPI + config.

## Release process

1. Update `Cargo.toml` versions.
2. Update `CHANGELOG.md`.
3. Open a PR titled `release: vX.Y.Z`.
4. After merge, tag `git tag -s vX.Y.Z -m "vX.Y.Z"`.
5. CI builds and pushes Docker images to `ghcr.io/picroom/picroom`.
6. GitHub release is auto-generated.

## Communication

- GitHub Issues for bugs + features.
- GitHub Discussions for questions + proposals.
- Discord (TBD) for chat.

## Code of conduct

Be kind, assume good faith, focus on the work. Harassment is not tolerated.

## License

By contributing, you agree that your contributions will be licensed under
the MIT License.