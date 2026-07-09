// SPDX-License-Identifier: MIT
// Copyright (c) 2026 Picroom Contributors

//! Audit tail subcommand.
//!
//! Reads events back from the `audit_events` table (the same table the HTTP
//! API writes via [`picroom_audit::DbAuditSink`]) so operators can inspect the
//! audit log from the CLI.

use crate::user::AnyPool;
use picroom_audit::{AuditAction, AuditEvent};
use serde_json;
use std::collections::HashSet;
use std::time::Duration;
use thiserror::Error;
use time::format_description::well_known::Rfc3339;
use time::OffsetDateTime;
use uuid::Uuid;

/// Audit tail errors.
#[derive(Debug, Error)]
pub enum AuditCmdError {
    /// DB error.
    #[error("db: {0}")]
    Db(String),
}

/// Streams audit events (newest first), optionally following new ones.
///
/// Reads from `audit_events` for either `PostgreSQL` or `SQLite`. When `follow` is
/// set, re-queries every couple of seconds and prints only events not seen
/// before. An optional `actor` email filters the result server-side in memory.
pub async fn audit_tail(
    pool: &AnyPool,
    follow: bool,
    actor: Option<String>,
) -> Result<(), AuditCmdError> {
    let mut seen: HashSet<Uuid> = HashSet::new();
    loop {
        let mut events = match pool {
            AnyPool::Pg(p) => audit_list_pg(p).await?,
            AnyPool::Sqlite(p) => audit_list_sqlite(p).await?,
        };
        if let Some(a) = &actor {
            events.retain(|e| e.actor_label.as_deref() == Some(a.as_str()));
        }
        // Print oldest→newest so the tail reads chronologically.
        for ev in events.iter().rev() {
            if seen.insert(ev.id) {
                println!(
                    "{} {} {} {}",
                    ev.timestamp,
                    ev.action.as_str(),
                    ev.target_type,
                    ev.target_id.as_deref().unwrap_or("-")
                );
            }
        }
        if !follow {
            return Ok(());
        }
        tokio::time::sleep(Duration::from_secs(2)).await;
    }
}

async fn audit_list_pg(pool: &sqlx::PgPool) -> Result<Vec<AuditEvent>, AuditCmdError> {
    let rows = sqlx::query_as::<_, (
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
    )>(
        "SELECT id, timestamp, actor_id, actor_label, action, target_type, target_id, ip::text, user_agent, metadata
         FROM audit_events
         ORDER BY timestamp DESC
         LIMIT 100",
    )
    .fetch_all(pool)
    .await
    .map_err(|e| AuditCmdError::Db(e.to_string()))?;

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

async fn audit_list_sqlite(pool: &sqlx::SqlitePool) -> Result<Vec<AuditEvent>, AuditCmdError> {
    let rows = sqlx::query_as::<_, (
        String,
        String,
        Option<String>,
        Option<String>,
        String,
        String,
        Option<String>,
        Option<String>,
        Option<String>,
        String,
    )>(
        "SELECT id, timestamp, actor_id, actor_label, action, target_type, target_id, ip, user_agent, metadata
         FROM audit_events
         ORDER BY timestamp DESC
         LIMIT 100",
    )
    .fetch_all(pool)
    .await
    .map_err(|e| AuditCmdError::Db(e.to_string()))?;

    Ok(rows
        .into_iter()
        .map(|r| AuditEvent {
            id: Uuid::parse_str(&r.0).unwrap_or_else(|_| Uuid::nil()),
            timestamp: OffsetDateTime::parse(&r.1, &Rfc3339).unwrap_or(OffsetDateTime::UNIX_EPOCH),
            actor_id: r.2.and_then(|s| Uuid::parse_str(&s).ok()),
            actor_label: r.3,
            action: AuditAction::parse(&r.4),
            target_type: r.5,
            target_id: r.6,
            ip: r.7,
            user_agent: r.8,
            metadata: serde_json::from_str(&r.9).unwrap_or(serde_json::Value::Null),
        })
        .collect())
}
