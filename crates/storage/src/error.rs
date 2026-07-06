//! Storage error types.

use picroom_domain::DomainError;
use thiserror::Error;
use url::ParseError;

/// Storage-layer errors.
#[derive(Debug, Error)]
pub enum StorageError {
    /// The requested object does not exist.
    #[error("object not found: {0}")]
    NotFound(String),

    /// The backend denied the operation (e.g. invalid credentials).
    #[error("permission denied: {0}")]
    PermissionDenied(String),

    /// Network or transport error.
    #[error("network error: {0}")]
    Network(String),

    /// Backend-specific error.
    #[error("backend error: {0}")]
    Backend(String),

    /// Configuration error (missing credentials, bad bucket, …).
    #[error("configuration error: {0}")]
    Config(String),

    /// Method not yet implemented (placeholder for skeleton).
    #[error("not implemented: {0}")]
    NotImplemented(&'static str),

    /// Underlying domain error.
    #[error(transparent)]
    Domain(#[from] DomainError),

    /// URL parse error.
    #[error(transparent)]
    Url(#[from] ParseError),
}

impl From<StorageError> for DomainError {
    fn from(e: StorageError) -> Self {
        match e {
            StorageError::NotFound(_) => Self::NotFound,
            StorageError::PermissionDenied(_) => Self::PermissionDenied,
            StorageError::Config(_) => Self::Internal(format!("{e}")),
            other => Self::Internal(format!("storage error: {other}")),
        }
    }
}