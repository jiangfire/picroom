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
        let guard = self.inner.read().unwrap();
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
            .unwrap()
            .insert(key.to_string(), (value, Instant::now() + ttl));
        Ok(())
    }

    async fn delete(&self, key: &str) -> Result<(), CacheError> {
        self.inner.write().unwrap().remove(key);
        Ok(())
    }
}
