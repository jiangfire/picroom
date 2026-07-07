//! # Picroom S3 Compatibility
//!
//! AWS S3-compatible HTTP endpoint with `SigV4` verification.

#![allow(missing_docs)]

pub mod bucket;
pub mod error;
pub mod list;
pub mod multipart;
pub mod object;
pub mod routes;
pub mod sigv4;

pub use error::S3Error;
pub use routes::s3_router;

use async_trait::async_trait;
use picroom_storage::Storage;
use std::sync::Arc;

/// Trait that the S3-compatible handlers require from the application
/// state. Implemented by `AppState` in the `picroom-api` crate.
#[async_trait]
pub trait S3State: Clone + Send + Sync + 'static {
    /// Returns a reference to the storage backend.
    fn storage(&self) -> &Arc<dyn Storage>;
}
