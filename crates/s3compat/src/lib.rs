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
