# ADR-0003: Split Storage trait by capability (Interface Segregation)

- **Status**: Accepted
- **Date**: 2026-07-05
- **Deciders**: Picroom maintainers

## Context

The naïve design for a storage abstraction is a single `Storage` trait with
all methods (`put`, `get`, `delete`, `list`, `sign_get_url`, `sign_put_url`,
`copy`, `multipart_init`, ...). This violates the Interface Segregation
Principle (ISP): a `LocalDriver` does not need `sign_put_url` (presigned URLs
are a cloud concept); an `S3Driver` always supports all of them.

Problems with the fat trait:

1. Drivers must stub out inapplicable methods.
2. Adding a new method to the trait breaks every implementor.
3. Mock implementations become unrealistic.
4. Test setup must construct unrealistic drivers.

## Decision

We split the storage surface into **four capabilities** combined into a
`Storage` supertrait:

```rust
#[async_trait]
pub trait StorageReader: Send + Sync {
    async fn get(&self, key: &StorageKey) -> Result<Bytes, StorageError>;
    async fn head(&self, key: &StorageKey) -> Result<ObjectMeta, StorageError>;
    async fn exists(&self, key: &StorageKey) -> Result<bool, StorageError>;
}

#[async_trait]
pub trait StorageWriter: Send + Sync {
    async fn put(&self, key: &StorageKey, bytes: Bytes) -> Result<(), StorageError>;
    async fn delete(&self, key: &StorageKey) -> Result<(), StorageError>;
}

#[async_trait]
pub trait StorageLister: Send + Sync {
    async fn list(&self, prefix: &StorageKey) -> Result<Page<ObjectMeta>, StorageError>;
}

#[async_trait]
pub trait StorageSigner: Send + Sync {
    async fn sign_get_url(&self, key: &StorageKey, ttl: Duration) -> Result<Url, StorageError>;
    async fn sign_put_url(&self, key: &StorageKey, ttl: Duration) -> Result<Url, StorageError>;
}

pub trait Storage: StorageReader + StorageWriter + StorageLister + StorageSigner {}
```

A driver that doesn't support signing (e.g., a future FTP driver) implements
only `StorageReader + StorageWriter + StorageLister`. The `Storage` supertrait
is reserved for fully-capable drivers.

## Consequences

### Positive

- Adding `StoragePinner` or `StorageReplicator` later is a non-breaking change.
- Tests can mock only what they need.
- LocalDriver can opt out of signing; the API layer will require a fully
  capable driver for S3-compat endpoints.

### Negative

- Slightly more verbose at callsites; we mitigate with an `AnyStorage` enum
  that implements all four supertraits via `match` dispatch (zero-cost).
- A consumer wanting "any storage" needs to declare which capability it
  needs in its bounds; this is good.

### Neutral

- We deliberately do not define `Multipart` as a separate trait in v1; S3
  multipart is implemented in the `s3compat` crate and uses `StorageWriter`
  under the hood.

## References

- Internal: `docs/spec.md` §12
- [AWS S3 API](https://docs.aws.amazon.com/AmazonS3/latest/API/API_Operations.html)