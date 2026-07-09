// SPDX-License-Identifier: MIT
// Copyright (c) 2026 Picroom Contributors

//! Read access to the append-only audit log.

use crate::event::{AuditAction, AuditEvent};
use crate::sink::AuditSinkError;
use async_trait::async_trait;
use time::OffsetDateTime;
use uuid::Uuid;

/// Read-only access to recorded audit events.
///
/// Implemented by [`crate::DbAuditSink`] so the API and CLI can page through
/// the audit log that the sink writes.
#[async_trait]
pub trait AuditReader: Send + Sync {
    /// Returns up to `limit` most-recent events, optionally restricted to those
    /// occurring strictly before `before` (used as a pagination cursor).
    async fn list(
        &self,
        limit: i64,
        before: Option<OffsetDateTime>,
    ) -> Result<Vec<AuditEvent>, AuditSinkError>;
}

#[async_trait]
impl AuditReader for crate::DbAuditSink {
    async fn list(
        &self,
        limit: i64,
        before: Option<OffsetDateTime>,
    ) -> Result<Vec<AuditEvent>, AuditSinkError> {
        let rows = sqlx::query_as::<
            _,
            (
                Uuid,
                OffsetDateTime,
                Option<Uuid>,
                Option<String>,
                String,
                String,
                Option<String>,
                Option<String>,
                Option<String>,
                serde_json::Value,
            ),
        >(
            r"SELECT id, timestamp, actor_id, actor_label, action, target_type,
                      target_id, ip::text, user_agent, metadata
               FROM audit_events
               WHERE ($1::timestamptz IS NULL OR timestamp < $1)
               ORDER BY timestamp DESC
               LIMIT $2",
        )
        .bind(before)
        .bind(limit)
        .fetch_all(&self.pool)
        .await
        .map_err(|e| AuditSinkError::Write(format!("list audit: {e}")))?;

        Ok(rows
            .into_iter()
            .map(|r| AuditEvent {
                id: r.0,
                timestamp: r.1,
                actor_id: r.2,
                actor_label: r.3,
                action: AuditAction::parse(&r.4),
                target_type: r.5,
                target_id: r.6,
                ip: r.7,
                user_agent: r.8,
                metadata: r.9,
            })
            .collect())
    }
}
