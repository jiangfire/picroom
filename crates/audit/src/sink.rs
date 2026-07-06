//! Audit sinks.

use crate::event::AuditEvent;
use async_trait::async_trait;
use std::sync::{Arc, Mutex};
use thiserror::Error;

/// Audit sink errors.
#[derive(Debug, Error)]
pub enum AuditSinkError {
    /// Backend write failure.
    #[error("write: {0}")]
    Write(String),
}

/// Sink for audit events.
#[async_trait]
pub trait AuditSink: Send + Sync {
    /// Persist the event.
    async fn record(&self, event: &AuditEvent) -> Result<(), AuditSinkError>;
}

/// No-op sink (used in tests where audit isn't asserted).
#[derive(Debug, Default, Clone)]
pub struct NoopAuditSink;

#[async_trait]
impl AuditSink for NoopAuditSink {
    async fn record(&self, _event: &AuditEvent) -> Result<(), AuditSinkError> {
        Ok(())
    }
}

/// In-memory sink (used in tests).
#[derive(Debug, Default, Clone)]
pub struct InMemoryAuditSink {
    events: Arc<Mutex<Vec<AuditEvent>>>,
}

impl InMemoryAuditSink {
    /// Creates a new empty sink.
    pub fn new() -> Self {
        Self::default()
    }

    /// Returns a snapshot of recorded events.
    pub fn events(&self) -> Vec<AuditEvent> {
        self.events.lock().expect("mutex poisoned").clone()
    }
}

#[async_trait]
impl AuditSink for InMemoryAuditSink {
    async fn record(&self, event: &AuditEvent) -> Result<(), AuditSinkError> {
        self.events
            .lock()
            .expect("mutex poisoned")
            .push(event.clone());
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::event::AuditAction;

    #[tokio::test]
    async fn noop_sink_succeeds() {
        let s = NoopAuditSink;
        let e = AuditEvent {
            id: uuid::Uuid::now_v7(),
            timestamp: time::OffsetDateTime::now_utc(),
            actor_id: None,
            actor_label: None,
            action: AuditAction::Other,
            target_type: "test".into(),
            target_id: None,
            ip: None,
            user_agent: None,
            metadata: serde_json::Value::Null,
        };
        s.record(&e).await.unwrap();
    }

    #[tokio::test]
    async fn in_memory_sink_records() {
        let s = InMemoryAuditSink::new();
        let e = AuditEvent {
            id: uuid::Uuid::now_v7(),
            timestamp: time::OffsetDateTime::now_utc(),
            actor_id: None,
            actor_label: None,
            action: AuditAction::ImageUpload,
            target_type: "image".into(),
            target_id: Some("id".into()),
            ip: None,
            user_agent: None,
            metadata: serde_json::Value::Null,
        };
        s.record(&e).await.unwrap();
        assert_eq!(s.events().len(), 1);
    }
}
