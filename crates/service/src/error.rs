//! Service-layer errors.

use picroom_domain::DomainError;
use thiserror::Error;

/// Service-layer errors.
#[derive(Debug, Error)]
pub enum ServiceError {
    /// Domain error.
    #[error(transparent)]
    Domain(#[from] DomainError),
    /// Storage error.
    #[error(transparent)]
    Storage(#[from] picroom_storage::StorageError),
    /// Auth decision error.
    #[error("auth decision: {0:?}")]
    AuthDecision(picroom_auth::Decision),
    /// Audit error.
    #[error(transparent)]
    Audit(#[from] picroom_audit::sink::AuditSinkError),
    /// Quota exceeded.
    #[error("quota exceeded: {0} bytes requested, {1} bytes available")]
    QuotaExceeded(u64, u64),
    /// Permission denied.
    #[error("permission denied")]
    PermissionDenied,
    /// Internal error.
    #[error("internal: {0}")]
    Internal(String),
}
