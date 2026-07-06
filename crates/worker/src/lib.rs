//! # Picroom Worker
//!
//! Async job consumer for the image pipeline.

#![warn(missing_docs)]

pub mod db_queue;
pub mod dlq;
pub mod job;
pub mod pool;
pub mod processor;
pub mod retry;

pub use db_queue::{JobRow, PgJobQueue, SqliteJobQueue};
pub use dlq::{DlqEntry, DlqSink};
pub use job::{Job, JobError, JobKind, JobQueue, JobResult};
pub use pool::WorkerPool;
pub use processor::{ImageLookup, ImageProcessor, ProcessorDeps, VariantRepository};
pub use retry::{RetryPolicy, RetryStrategy};