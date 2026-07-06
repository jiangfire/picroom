//! Auth middleware — re-exports from `extractors::auth`.
//!
//! The real `require_auth` middleware lives in
//! [`crate::extractors::auth::require_auth`] so router.rs gets
//! it from a single consolidated location.

pub use crate::extractors::auth::require_auth;