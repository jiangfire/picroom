// SPDX-License-Identifier: MIT
// Copyright (c) 2026 Picroom Contributors

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
    async fn list(&self, prefix: Option<&StorageKey>) -> Result<Page<ObjectMeta>, StorageError> {
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::driver::LocalDriver;
    use bytes::Bytes;
    use picroom_domain::StorageKey;
    use std::time::Duration;

    fn tmp_driver() -> LocalDriver {
        let base = std::env::temp_dir().join(format!("picroom-any-{}", uuid::Uuid::now_v7()));
        std::fs::create_dir_all(&base).unwrap();
        LocalDriver::new(base, "https://cdn.example.com/i")
    }

    #[tokio::test]
    async fn any_local_dispatches_all_operations() {
        let storage = AnyStorage::Local(tmp_driver());
        let key = StorageKey::parse("a/b.bin").unwrap();

        storage.put(&key, Bytes::from_static(b"hi")).await.unwrap();
        let got = storage.get(&key).await.unwrap();
        assert_eq!(got, Bytes::from_static(b"hi"));
        assert!(storage.exists(&key).await.unwrap());
        let meta = storage.head(&key).await.unwrap();
        assert_eq!(meta.bytes, 2);
        let page = storage.list(None).await.unwrap();
        assert_eq!(page.items.len(), 1);
        let url = storage
            .sign_get_url(&key, Duration::from_secs(30))
            .await
            .unwrap();
        assert!(url.as_str().ends_with("a/b.bin"));
        storage.delete(&key).await.unwrap();
        assert!(!storage.exists(&key).await.unwrap());
    }
}
