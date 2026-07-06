# ADR-0001: Use Rust + axum as the primary backend stack

- **Status**: Accepted
- **Date**: 2026-07-05
- **Deciders**: Picroom maintainers

## Context

We need to pick a backend language/runtime for Picroom. Options considered:

| Option | Pros | Cons |
|---|---|---|
| Rust + axum | Single binary, memory safety, excellent async, native image processing via libvips | Steeper learning curve, longer compile times |
| Go + echo/chi | Mature, fast compile, large ecosystem | No native image lib (relies on cgo), GC pauses |
| Node.js (NestJS) | Fast iteration, sharp library | Heavy runtime, no single-binary deployment |
| PHP (Laravel) | Familiar to many, large ecosystem | Slow, hard to ship single binary, ecosystem of image libs is Imagick-dependent |
| Elixir (Phoenix) | Excellent concurrency, fault tolerance | Smaller pool of contributors |

## Decision

We use **Rust + axum + Tokio** for the backend.

## Consequences

### Positive

- Single statically-linked binary (~30 MB) makes deployment trivial
  (single-file + docker), per 12-Factor §5/§9.
- Compile-time guarantees (ownership + lifetimes + sqlx compile-time check)
  reduce runtime errors and security holes.
- Native access to libvips via `bimg` or `image` crate → fast image pipeline.
- Tokio is the most mature async runtime in Rust; axum builds on Tower
  middleware which gives us battle-tested auth/tracing/rate-limit modules.

### Negative

- Compile times are long; mitigated by workspace structure and incremental CI
  caching (`sccache`).
- Smaller pool of contributors vs. Go/Node; mitigated by excellent docs and
  AI-assisted onboarding.
- async/await + lifetimes can confuse newcomers; mitigated by clear
  conventions in `docs/spec.md` §5.

### Neutral

- We commit to keeping Rust edition = 2021 for stability.
- We commit to Tokio (no `async-std`, no `smol`) to avoid fragmentation.

## Alternatives revisited

- **Go** is a strong runner-up. We rejected it because (a) image processing
  relies on cgo which complicates deployment; (b) GC pauses harm tail latency
  on small object reads. We may reconsider if image performance becomes a
  non-issue.
- **Node.js** is rejected because the runtime is large and our S3-compat layer
  benefits from native HTTP streaming semantics.

## References

- [tokio.rs](https://tokio.rs/)
- [axum docs](https://docs.rs/axum/)
- Internal: `docs/spec.md` §2.1, §3