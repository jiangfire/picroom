//! Audit tail subcommand (skeleton).

use picroom_audit::AuditEvent;
use thiserror::Error;

/// Audit tail errors.
#[derive(Debug, Error)]
pub enum AuditCmdError {
    /// DB error.
    #[error("db: {0}")]
    Db(String),
}

/// Streams audit events (skeleton — real impl in Phase 6+).
pub async fn audit_tail(_follow: bool, _actor: Option<String>) -> Result<Vec<AuditEvent>, AuditCmdError> {
    Ok(Vec::new())
}