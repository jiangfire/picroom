// SPDX-License-Identifier: MIT
// Copyright (c) 2026 Picroom Contributors

//! Cache trait + in-memory implementation.

use async_trait::async_trait;
use std::collections::HashMap;
use std::sync::RwLock;
use std::time::{Duration, Instant};
use thiserror::Error;

/// Cache errors.
#[derive(Debug, Error)]
pub enum CacheError {
    /// Get returned no value.
    #[error("miss")]
    Miss,
    /// Backend error.
    #[error("backend: {0}")]
    Backend(String),
}

/// Cache backend trait.
#[async_trait]
pub trait Cache: Send + Sync {
    /// Fetches a value by key.
    async fn get(&self, key: &str) -> Result<Vec<u8>, CacheError>;
    /// Stores a value with a TTL.
    async fn set(&self, key: &str, value: Vec<u8>, ttl: Duration) -> Result<(), CacheError>;
    /// Deletes a key.
    async fn delete(&self, key: &str) -> Result<(), CacheError>;
}

/// In-memory cache (used for tests + single-node deployments).
#[derive(Debug, Default)]
pub struct InMemoryCache {
    inner: RwLock<HashMap<String, (Vec<u8>, Instant)>>,
}

impl InMemoryCache {
    /// Creates a new empty in-memory cache.
    pub fn new() -> Self {
        Self::default()
    }
}

#[async_trait]
impl Cache for InMemoryCache {
    async fn get(&self, key: &str) -> Result<Vec<u8>, CacheError> {
        // Recover from a poisoned lock rather than panicking: for an in-memory
        // cache the worst case is a stale value, which is preferable to taking
        // the process down.
        let guard = self
            .inner
            .read()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if let Some((v, expires_at)) = guard.get(key) {
            if Instant::now() < *expires_at {
                return Ok(v.clone());
            }
        }
        Err(CacheError::Miss)
    }

    async fn set(&self, key: &str, value: Vec<u8>, ttl: Duration) -> Result<(), CacheError> {
        self.inner
            .write()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .insert(key.to_string(), (value, Instant::now() + ttl));
        Ok(())
    }

    async fn delete(&self, key: &str) -> Result<(), CacheError> {
        self.inner
            .write()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .remove(key);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[tokio::test]
    async fn set_then_get_returns_stored_value() {
        let cache = InMemoryCache::new();
        cache
            .set("k", b"v".to_vec(), Duration::from_secs(60))
            .await
            .unwrap();
        assert_eq!(cache.get("k").await.unwrap(), b"v");
    }

    #[tokio::test]
    async fn get_missing_key_is_miss() {
        let cache = InMemoryCache::new();
        assert!(matches!(cache.get("nope").await, Err(CacheError::Miss)));
    }

    #[tokio::test]
    async fn entry_expires_after_ttl() {
        let cache = InMemoryCache::new();
        cache
            .set("k", b"v".to_vec(), Duration::from_millis(20))
            .await
            .unwrap();
        // Immediately readable.
        assert_eq!(cache.get("k").await.unwrap(), b"v");
        // After the TTL elapses it is a miss.
        tokio::time::sleep(Duration::from_millis(40)).await;
        assert!(matches!(cache.get("k").await, Err(CacheError::Miss)));
    }

    #[tokio::test]
    async fn delete_removes_entry() {
        let cache = InMemoryCache::new();
        cache
            .set("k", b"v".to_vec(), Duration::from_secs(60))
            .await
            .unwrap();
        cache.delete("k").await.unwrap();
        assert!(matches!(cache.get("k").await, Err(CacheError::Miss)));
    }
}
