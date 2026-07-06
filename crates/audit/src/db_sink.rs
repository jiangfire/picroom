//! PostgreSQL-backed audit sink.

use crate::event::AuditEvent;
use crate::sink::{AuditSink, AuditSinkError};
use async_trait::async_trait;
use sqlx::PgPool;

/// DB-backed audit sink. Inserts events into the `audit_events` table.
#[derive(Debug, Clone)]
pub struct DbAuditSink {
    pool: PgPool,
}

impl DbAuditSink {
    /// Creates a new sink bound to the given pool.
    pub const fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl AuditSink for DbAuditSink {
    async fn record(&self, event: &AuditEvent) -> Result<(), AuditSinkError> {
        sqlx::query(
            r"
            INSERT INTO audit_events (
                id, timestamp, actor_id, actor_label, action,
                target_type, target_id, ip, user_agent, metadata
            )
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8::inet, $9, $10::jsonb)
            ",
        )
        .bind(event.id)
        .bind(event.timestamp)
        .bind(event.actor_id)
        .bind(event.actor_label.as_deref())
        .bind(event.action.as_str())
        .bind(&event.target_type)
        .bind(event.target_id.as_deref())
        .bind(event.ip.as_deref())
        .bind(event.user_agent.as_deref())
        .bind(&event.metadata)
        .execute(&self.pool)
        .await
        .map_err(|e| AuditSinkError::Write(format!("insert audit: {e}")))?;
        Ok(())
    }
}
