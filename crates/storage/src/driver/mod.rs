//! Driver implementations of the storage traits.
//!
//! Each driver is feature-gated and exposes a single struct that implements
//! the [`Storage`] trait family.

pub mod local;
pub mod minio;
pub mod s3;

pub use local::LocalDriver;
pub use minio::MinioDriver;
pub use s3::S3Driver;

use crate::StorageError;
use bytes::Bytes;
use picroom_domain::{Page, StorageKey};
use serde::{Deserialize, Serialize};
use std::time::Duration;
use time::OffsetDateTime;
use url::Url;

/// Metadata returned from a storage backend about an object.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ObjectMeta {
    /// Storage key of the object.
    pub key: StorageKey,
    /// Object size in bytes.
    pub bytes: u64,
    /// Last-modified timestamp (UTC).
    pub last_modified: OffsetDateTime,
    /// Hex-encoded SHA-256 of the object, if available.
    pub etag: Option<String>,
}

/// Read operations on a storage backend.
#[async_trait::async_trait]
pub trait StorageReader: Send + Sync {
    /// Fetches the object bytes.
    async fn get(&self, key: &StorageKey) -> Result<Bytes, StorageError>;
    /// Fetches only the metadata.
    async fn head(&self, key: &StorageKey) -> Result<ObjectMeta, StorageError>;
    /// Returns whether the object exists.
    async fn exists(&self, key: &StorageKey) -> Result<bool, StorageError>;
}

/// Write operations on a storage backend.
#[async_trait::async_trait]
pub trait StorageWriter: Send + Sync {
    /// Stores the given bytes under the key (idempotent overwrite).
    async fn put(&self, key: &StorageKey, bytes: Bytes) -> Result<(), StorageError>;
    /// Deletes the object; returns `Ok(())` even if the object didn't exist.
    async fn delete(&self, key: &StorageKey) -> Result<(), StorageError>;
}

/// Listing operations on a storage backend.
#[async_trait::async_trait]
pub trait StorageLister: Send + Sync {
    /// Lists objects with the given prefix, returning a page of metadata.
    async fn list(&self, prefix: &StorageKey) -> Result<Page<ObjectMeta>, StorageError>;
}

/// URL signing for cloud backends.
#[async_trait::async_trait]
pub trait StorageSigner: Send + Sync {
    /// Generates a pre-signed URL for `GET`.
    async fn sign_get_url(&self, key: &StorageKey, ttl: Duration) -> Result<Url, StorageError>;
    /// Generates a pre-signed URL for `PUT`.
    async fn sign_put_url(&self, key: &StorageKey, ttl: Duration) -> Result<Url, StorageError>;
}

/// Supertrait combining all storage capabilities.
pub trait Storage: StorageReader + StorageWriter + StorageLister + StorageSigner {}
