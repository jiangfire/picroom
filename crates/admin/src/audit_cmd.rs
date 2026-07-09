// SPDX-License-Identifier: MIT
// Copyright (c) 2026 Picroom Contributors

//! Audit tail subcommand.
//!
//! The HTTP API records audit events via `DbAuditSink`, but the CLI tail is
//! not yet wired to read them back. Rather than silently returning an empty
//! list (which looks like "no events"), this command fails explicitly so
//! operators know the feature is pending.

use picroom_audit::AuditEvent;
use thiserror::Error;

/// Audit tail errors.
#[derive(Debug, Error)]
pub enum AuditCmdError {
    /// DB error.
    #[error("db: {0}")]
    Db(String),
    /// Feature not yet implemented.
    #[error("audit tail is not yet wired to the database (events are recorded via DbAuditSink but the CLI reader is unimplemented; see docs/review-2026-07.md §3.8)")]
    NotImplemented,
}

/// Streams audit events.
///
/// Currently unimplemented against the DB — returns `NotImplemented` so the
/// CLI surfaces a clear message instead of an empty result.
pub async fn audit_tail(
    _follow: bool,
    _actor: Option<String>,
) -> Result<Vec<AuditEvent>, AuditCmdError> {
    Err(AuditCmdError::NotImplemented)
}
