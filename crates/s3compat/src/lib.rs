// SPDX-License-Identifier: MIT
// Copyright (c) 2026 Picroom Contributors

//! # Picroom S3 Compatibility
//!
//! AWS S3-compatible HTTP endpoint with `SigV4` verification.

#![allow(missing_docs)]

pub mod bucket;
pub mod error;
pub mod list;
pub mod middleware;
pub mod multipart;
pub mod object;
pub mod routes;
pub mod sigv4;

pub use error::S3Error;
pub use routes::s3_router;

use async_trait::async_trait;
use picroom_storage::Storage;
use std::sync::Arc;

/// A single accepted S3 client credential (`SigV4` access key + secret).
#[derive(Debug, Clone)]
pub struct S3Credential {
    /// Access key id clients present in the `Authorization` header.
    pub access_key: String,
    /// Matching secret used to recompute the signature.
    pub secret: String,
}

/// Trait that the S3-compatible handlers require from the application
/// state. Implemented by `AppState` in the `picroom-api` crate.
#[async_trait]
pub trait S3State: Clone + Send + Sync + 'static {
    /// Returns a reference to the storage backend.
    fn storage(&self) -> &Arc<dyn Storage>;

    /// Returns the S3 client credential to validate `SigV4` signatures
    /// against. When `None`, the S3 endpoint runs unauthenticated (dev mode).
    fn s3_credentials(&self) -> Option<S3Credential>;
}

#[cfg(test)]
pub(crate) mod test_util {
    //! In-memory test doubles for the S3-compatible handlers.
    use super::*;
    use async_trait::async_trait;
    use bytes::Bytes;
    use picroom_domain::{Page, PageReq, StorageKey};
    use picroom_storage::driver::{
        ObjectMeta, Storage, StorageLister, StorageReader, StorageSigner, StorageWriter,
    };
    use picroom_storage::StorageError;
    use std::collections::HashMap;
    use std::sync::Arc;
    use time::OffsetDateTime;
    use url::Url;

    /// In-memory [`Storage`] fake. `set_fail` injects a backend error into
    /// every operation so handler error paths can be exercised.
    #[derive(Default)]
    pub struct MockStorage {
        inner: std::sync::Mutex<HashMap<String, Bytes>>,
        fail: std::sync::Mutex<bool>,
    }

    impl MockStorage {
        pub fn new() -> Self {
            Self::default()
        }

        pub fn set_fail(&self, v: bool) {
            *self.fail.lock().unwrap() = v;
        }

        fn failing(&self) -> bool {
            *self.fail.lock().unwrap()
        }
    }

    fn mock_err() -> StorageError {
        StorageError::Backend("mock storage failure".into())
    }

    #[async_trait]
    impl StorageReader for MockStorage {
        async fn get(&self, key: &StorageKey) -> Result<Bytes, StorageError> {
            if self.failing() {
                return Err(mock_err());
            }
            self.inner
                .lock()
                .unwrap()
                .get(key.as_str())
                .cloned()
                .ok_or_else(|| StorageError::NotFound(key.to_string()))
        }

        async fn head(&self, key: &StorageKey) -> Result<ObjectMeta, StorageError> {
            if self.failing() {
                return Err(mock_err());
            }
            let lock = self.inner.lock().unwrap();
            match lock.get(key.as_str()) {
                Some(b) => Ok(ObjectMeta {
                    key: key.clone(),
                    bytes: b.len() as u64,
                    last_modified: OffsetDateTime::now_utc(),
                    etag: None,
                }),
                None => Err(StorageError::NotFound(key.to_string())),
            }
        }

        async fn exists(&self, key: &StorageKey) -> Result<bool, StorageError> {
            if self.failing() {
                return Err(mock_err());
            }
            Ok(self.inner.lock().unwrap().contains_key(key.as_str()))
        }
    }

    #[async_trait]
    impl StorageWriter for MockStorage {
        async fn put(&self, key: &StorageKey, bytes: Bytes) -> Result<(), StorageError> {
            if self.failing() {
                return Err(mock_err());
            }
            self.inner
                .lock()
                .unwrap()
                .insert(key.as_str().to_string(), bytes);
            Ok(())
        }

        async fn delete(&self, key: &StorageKey) -> Result<(), StorageError> {
            if self.failing() {
                return Err(mock_err());
            }
            self.inner.lock().unwrap().remove(key.as_str());
            Ok(())
        }
    }

    #[async_trait]
    impl StorageLister for MockStorage {
        async fn list(
            &self,
            _prefix: Option<&StorageKey>,
        ) -> Result<Page<ObjectMeta>, StorageError> {
            if self.failing() {
                return Err(mock_err());
            }
            let lock = self.inner.lock().unwrap();
            let items: Vec<ObjectMeta> = lock
                .iter()
                .map(|(k, b)| ObjectMeta {
                    key: StorageKey::parse(k).unwrap(),
                    bytes: b.len() as u64,
                    last_modified: OffsetDateTime::now_utc(),
                    etag: None,
                })
                .collect();
            Ok(Page::new(items, None, PageReq::default()))
        }
    }

    #[async_trait]
    impl StorageSigner for MockStorage {
        async fn sign_get_url(
            &self,
            _key: &StorageKey,
            _ttl: std::time::Duration,
        ) -> Result<Url, StorageError> {
            Err(StorageError::NotImplemented("signing unsupported in mock"))
        }

        async fn sign_put_url(
            &self,
            _key: &StorageKey,
            _ttl: std::time::Duration,
        ) -> Result<Url, StorageError> {
            Err(StorageError::NotImplemented("signing unsupported in mock"))
        }
    }

    impl Storage for MockStorage {}

    /// Test [`S3State`] backed by [`MockStorage`].
    #[derive(Clone)]
    pub struct TestState {
        inner: Arc<MockStorage>,
        as_trait: Arc<dyn Storage>,
    }

    impl TestState {
        pub fn new() -> Self {
            let inner = Arc::new(MockStorage::new());
            let as_trait: Arc<dyn Storage> = inner.clone();
            Self { inner, as_trait }
        }

        pub fn set_fail(&self, v: bool) {
            self.inner.set_fail(v);
        }
    }

    #[async_trait]
    impl S3State for TestState {
        fn storage(&self) -> &Arc<dyn Storage> {
            &self.as_trait
        }

        fn s3_credentials(&self) -> Option<S3Credential> {
            None
        }
    }
}
