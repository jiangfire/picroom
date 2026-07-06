//! Domain error type.

use thiserror::Error;

/// Domain-level errors.
#[derive(Debug, Error, PartialEq, Eq)]
pub enum DomainError {
    /// Resource not found.
    #[error("not found")]
    NotFound,

    /// Permission denied.
    #[error("permission denied")]
    PermissionDenied,

    /// Validation error (bad input).
    #[error("validation: {0}")]
    Validation(String),

    /// Internal error.
    #[error("internal: {0}")]
    Internal(String),

    /// Conflict (e.g. duplicate).
    #[error("conflict: {0}")]
    Conflict(String),
}

impl DomainError {
    /// Convenience constructor.
    pub fn validation(msg: impl Into<String>) -> Self {
        Self::Validation(msg.into())
    }
}