//! # Picroom API
//!
//! HTTP API surface (REST + S3-compat).

#![warn(missing_docs)]

pub mod error;
pub mod extractors;
pub mod handlers;
pub mod middleware;
pub mod router;
pub mod state;

pub use router::build_router;
pub use state::{AppState, StorageWriterFromArc};