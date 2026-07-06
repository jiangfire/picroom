//! Zero-cost enum dispatch over all storage drivers.
//!
//! Use this when a service needs to operate on "any storage backend"
//! without paying for dynamic dispatch.

use crate::driver::{
    LocalDriver, ObjectMeta, S3Driver, Storage, StorageLister, StorageReader, StorageSigner,
    StorageWriter,
};
use crate::StorageError;
use async_trait::async_trait;
use bytes::Bytes;
use picroom_domain::{Page, StorageKey};
use std::time::Duration;
use url::Url;

/// Enum dispatching to a concrete driver at compile time.
#[derive(Debug, Clone)]
pub enum AnyStorage {
    /// Local filesystem driver.
    Local(LocalDriver),
    /// AWS S3 / S3-compatible driver.
    S3(S3Driver),
}

#[async_trait]
impl StorageReader for AnyStorage {
    async fn get(&self, key: &StorageKey) -> Result<Bytes, StorageError> {
        match self {
            Self::Local(d) => d.get(key).await,
            Self::S3(d) => d.get(key).await,
        }
    }

    async fn head(&self, key: &StorageKey) -> Result<ObjectMeta, StorageError> {
        match self {
            Self::Local(d) => d.head(key).await,
            Self::S3(d) => d.head(key).await,
        }
    }

    async fn exists(&self, key: &StorageKey) -> Result<bool, StorageError> {
        match self {
            Self::Local(d) => d.exists(key).await,
            Self::S3(d) => d.exists(key).await,
        }
    }
}

#[async_trait]
impl StorageWriter for AnyStorage {
    async fn put(&self, key: &StorageKey, bytes: Bytes) -> Result<(), StorageError> {
        match self {
            Self::Local(d) => d.put(key, bytes).await,
            Self::S3(d) => d.put(key, bytes).await,
        }
    }

    async fn delete(&self, key: &StorageKey) -> Result<(), StorageError> {
        match self {
            Self::Local(d) => d.delete(key).await,
            Self::S3(d) => d.delete(key).await,
        }
    }
}

#[async_trait]
impl StorageLister for AnyStorage {
    async fn list(&self, prefix: &StorageKey) -> Result<Page<ObjectMeta>, StorageError> {
        match self {
            Self::Local(d) => d.list(prefix).await,
            Self::S3(d) => d.list(prefix).await,
        }
    }
}

#[async_trait]
impl StorageSigner for AnyStorage {
    async fn sign_get_url(&self, key: &StorageKey, ttl: Duration) -> Result<Url, StorageError> {
        match self {
            Self::Local(d) => d.sign_get_url(key, ttl).await,
            Self::S3(d) => d.sign_get_url(key, ttl).await,
        }
    }

    async fn sign_put_url(&self, key: &StorageKey, ttl: Duration) -> Result<Url, StorageError> {
        match self {
            Self::Local(d) => d.sign_put_url(key, ttl).await,
            Self::S3(d) => d.sign_put_url(key, ttl).await,
        }
    }
}

impl Storage for AnyStorage {}
