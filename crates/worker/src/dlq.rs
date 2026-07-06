//! Dead-letter queue.

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use time::OffsetDateTime;
use uuid::Uuid;

/// A poison-message entry.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DlqEntry {
    /// Job id.
    pub job_id: Uuid,
    /// Final error message.
    pub error: String,
    /// Number of attempts before giving up.
    pub attempts: u32,
    /// When the job was moved to DLQ.
    pub moved_at: OffsetDateTime,
}

/// Dead-letter queue sink.
#[async_trait]
pub trait DlqSink: Send + Sync {
    /// Appends an entry to the DLQ.
    async fn push(&self, entry: DlqEntry) -> Result<(), String>;
}

/// In-memory DLQ sink.
#[derive(Debug, Default)]
pub struct InMemoryDlq {
    entries: std::sync::Mutex<Vec<DlqEntry>>,
}

impl InMemoryDlq {
    /// Creates a new empty DLQ.
    pub fn new() -> Self {
        Self::default()
    }

    /// Returns a snapshot of all entries.
    pub fn entries(&self) -> Vec<DlqEntry> {
        self.entries.lock().expect("mutex poisoned").clone()
    }
}

#[async_trait]
impl DlqSink for InMemoryDlq {
    async fn push(&self, entry: DlqEntry) -> Result<(), String> {
        self.entries.lock().expect("mutex poisoned").push(entry);
        Ok(())
    }
}
