# ADR-0006: Image pipeline — async background queue

- **Status**: Accepted
- **Date**: 2026-07-05
- **Deciders**: Picroom maintainers

## Context

Uploading an image triggers several CPU/IO-heavy operations:

- AVIF encoding (~5–10x slower than JPEG)
- WebP encoding
- Thumbnail generation (3 sizes)
- EXIF stripping
- Optional watermark

Synchronous encoding would block the upload response for seconds, hurting UX
and throughput. We need an async pipeline that:

1. Returns upload acceptance immediately with an `image_id`.
2. Generates variants in the background.
3. Is observable (status, retries, DLQ).
4. Is fault-tolerant (worker crash → job recovers).

## Decision

We adopt an **asynchronous background queue** model:

```
Upload → Validate → Probe → Persist original → Enqueue job → 202-style response
                                                  ↓
                              Worker pool consumes jobs
                                                  ↓
                          ┌──────────────┬──────────────┐
                          ▼              ▼              ▼
                      AVIF          WebP         Thumbnail(s)
                          └──────────────┴──────────────┘
                                                  ↓
                              Update image_variants table
                                                  ↓
                              Emit audit event
```

- Queue: database-backed (`SELECT ... FOR UPDATE SKIP LOCKED`).
- Concurrency: configurable pool size (default = num CPUs).
- Retry: exponential backoff (1s → 60s), max 5 attempts.
- DLQ: poison messages land in a separate table for operator review.

## Consequences

### Positive

- Upload response time < 50 ms (just validation + persist + enqueue).
- Worker can scale horizontally (`picroom worker` × N replicas).
- Retries are automatic and observable.

### Negative

- Variants are not immediately available after upload; clients must handle
  `404 variant_not_ready` gracefully.
- More moving parts; mitigated by clear audit log + DLQ dashboard.

### Neutral

- We use the database as the queue, not Redis/Kafka, to keep infra simple.
  This caps throughput at ~1000 jobs/s but is more than sufficient for v1.
- We do not implement WebSocket push for variant readiness (out of scope;
  clients can poll or subscribe to webhooks).

## References

- Internal: `docs/spec.md` §11
- [`SKIP LOCKED` queue pattern](https://www.crunchydata.com/blog/skip-locked)