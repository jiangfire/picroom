# ADR-0004: S3-compatible API surface in v1

- **Status**: Accepted
- **Date**: 2026-07-05
- **Deciders**: Picroom maintainers

## Context

Picroom needs to integrate with existing tools: PicGo, rclone, AWS CLI, custom
CDNs, build pipelines. Building a bespoke client ecosystem is unrealistic.

Options:

1. **Custom REST only**: simplest but forces tool maintainers to write
   Picroom-specific adapters.
2. **S3-compatible API**: enables any SigV4 client; most realistic path.
3. **Both**: we already do REST; adding S3 doubles endpoint coverage with
   moderate effort.

## Decision

We implement an **AWS S3-compatible API** in v1, mounted under `/s3/`.

Capabilities (v1):

- PUT / GET / HEAD / DELETE object
- Multipart upload (init / part / complete / abort)
- ListObjectsV2
- Path-style URLs (`/s3/:bucket/:key`)
- AWS Signature V4 verification

Capabilities (post-MVP):

- Server-side copy
- Bucket lifecycle policies
- Versioning
- ACLs beyond RBAC mapping

## Consequences

### Positive

- PicGo, rclone, AWS CLI, Cyberduck, MinIO client, and any S3 SDK works
  against Picroom out-of-the-box.
- This is the strongest single differentiator vs. Lsky Pro / EasyImage.
- Plugin ecosystem cost → near zero (reuse existing S3 plugins).

### Negative

- SigV4 spec is intricate; edge cases around `x-amz-content-sha256`,
  unsigned-payload, and presigned URLs require careful testing.
- We must serve `text/xml` for ListObjects responses, adding a serializer
  (`quick-xml`).
- We need to map S3 buckets to Picroom storage policies; this is a soft
  indirection (see ADR-0005).

### Neutral

- We do not implement S3 Select, S3 Batch, S3 Object Lambda (out of scope).
- We do not claim S3-compliance certification (would require $$$); we aim
  for "good-enough" compatibility with common clients.

## Verification strategy

We use the AWS SigV4 reference test suite (see
[sigv4-test-suite](https://docs.aws.amazon.com/general/latest/gr/sigv4-signed-request-examples.html))
plus a contract test that runs real `aws-cli` and `rclone` commands
against a testcontainers-spawned Picroom + MinIO.

## References

- [AWS SigV4 spec](https://docs.aws.amazon.com/IAM/latest/UserGuide/reference_sigv-create-signed-request.html)
- [S3 API reference](https://docs.aws.amazon.com/AmazonS3/latest/API/Welcome.html)
- Internal: `docs/spec.md` §8.2