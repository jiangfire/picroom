//! Job types and queue.

use async_trait::async_trait;
use picroom_domain::ImageId;
use serde::{Deserialize, Serialize};
use thiserror::Error;
use time::OffsetDateTime;
use uuid::Uuid;

/// Kinds of async jobs.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum JobKind {
    /// Encode AVIF variant.
    EncodeAvif,
    /// Encode WebP variant.
    EncodeWebp,
    /// Generate thumbnail at given size.
    GenerateThumbnail {
        /// Target size (px).
        size: u32,
    },
    /// Apply watermark.
    ApplyWatermark,
    /// Strip EXIF.
    StripExif,
}

/// A unit of work enqueued for the worker.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Job {
    /// Stable job id.
    pub id: Uuid,
    /// Target image.
    pub image_id: ImageId,
    /// What to do.
    pub kind: JobKind,
    /// Number of times this job has been attempted.
    pub attempts: u32,
    /// When the job was enqueued.
    pub enqueued_at: OffsetDateTime,
}

/// Job errors.
#[derive(Debug, Error)]
pub enum JobError {
    /// Job not found.
    #[error("not found")]
    NotFound,
    /// Job locked by another worker.
    #[error("locked")]
    Locked,
    /// Job processing failed (transient).
    #[error("processing: {0}")]
    Processing(String),
    /// Backend storage error.
    #[error("storage: {0}")]
    Storage(String),
}

/// Result of running a job.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum JobResult {
    /// Variant produced.
    Variant {
        /// Variant kind (avif, webp, `thumb_200`, …).
        kind: String,
        /// Storage key.
        key: String,
        /// Bytes (for small variants, optional; `Vec<u8>` for serde).
        bytes: Option<Vec<u8>>,
    },
    /// Job skipped (already done, idempotency).
    Skipped,
}

/// Queue abstraction.
#[async_trait]
pub trait JobQueue: Send + Sync {
    /// Enqueues a job.
    async fn enqueue(&self, job: Job) -> Result<(), JobError>;
    /// Dequeues the next runnable job (or `None` if empty).
    async fn dequeue(&self) -> Result<Option<Job>, JobError>;
    /// Marks a job complete.
    async fn complete(&self, id: Uuid, result: &JobResult) -> Result<(), JobError>;
    /// Marks a job failed (retry per policy, or DLQ).
    async fn fail(&self, id: Uuid, error: &str) -> Result<(), JobError>;
}