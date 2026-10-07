// SPDX-License-Identifier: MIT
// Copyright (c) 2026 Picroom Contributors

//! Database-backed job queue.
//!
//! Two backends:
//! - [`PgJobQueue`] uses `SELECT … FOR UPDATE SKIP LOCKED` for safe
//!   concurrent consumption across multiple workers.
//! - [`SqliteJobQueue`] is a single-process implementation suitable for
//!   development and single-host deployments.

use crate::job::{Job, JobError, JobKind, JobQueue, JobResult};
use async_trait::async_trait;
use picroom_domain::ImageId;
use sqlx::PgPool;
use sqlx::SqlitePool;
use time::OffsetDateTime;
use uuid::Uuid;

/// PostgreSQL-backed job queue.
#[derive(Debug, Clone)]
pub struct PgJobQueue {
    pool: PgPool,
    /// How long a claimed job stays exclusively ours before another worker
    /// may reclaim it (D-8).
    lease_secs: i64,
    /// Identity recorded in `jobs.claimed_by` for observability.
    worker_id: String,
}

impl PgJobQueue {
    /// Creates a new queue bound to the given pool (300 s lease).
    pub fn new(pool: PgPool) -> Self {
        Self::with_lease(pool, DEFAULT_LEASE_SECS)
    }

    /// Creates a new queue with an explicit lease duration in seconds.
    pub fn with_lease(pool: PgPool, lease_secs: u64) -> Self {
        Self {
            pool,
            lease_secs: lease_secs as i64,
            worker_id: default_worker_id(),
        }
    }

    /// Returns a clone of the underlying pool.
    pub const fn pool(&self) -> &PgPool {
        &self.pool
    }
}

/// Default claim duration before a `running` job can be reclaimed.
pub const DEFAULT_LEASE_SECS: u64 = 300;

/// Stable-enough worker identity for the `claimed_by` column.
fn default_worker_id() -> String {
    format!(
        "{}#{}",
        std::env::var("HOSTNAME").unwrap_or_else(|_| "worker".into()),
        std::process::id()
    )
}

#[async_trait]
impl JobQueue for PgJobQueue {
    async fn enqueue(&self, job: Job) -> Result<(), JobError> {
        let kind_json = serde_json::to_string(&job.kind)
            .map_err(|e| JobError::Processing(format!("serialize kind: {e}")))?;
        sqlx::query(
            r"
            INSERT INTO jobs (id, image_id, kind, payload, status, attempts, enqueued_at)
            VALUES ($1, $2, $3, $4, 'pending', $5, $6)
            ON CONFLICT (id) DO NOTHING
            ",
        )
        .bind(job.id.to_string())
        .bind(job.image_id.as_uuid().to_string())
        .bind(kind_json)
        .bind(Option::<String>::None)
        .bind(job.attempts as i32)
        .bind(job.enqueued_at)
        .execute(&self.pool)
        .await
        .map_err(|e| JobError::Processing(format!("enqueue: {e}")))?;
        Ok(())
    }

    async fn dequeue(&self) -> Result<Option<Job>, JobError> {
        // `FOR UPDATE SKIP LOCKED` ensures two workers never grab the same
        // row. Lease-expired `running` rows are re-claimable (D-8): a worker
        // that died mid-job does not strand the image forever.
        let row: Option<JobRow> = sqlx::query_as::<_, JobRow>(
            r"
            WITH next_job AS (
                SELECT id
                FROM jobs
                WHERE status = 'pending'
                   OR (status = 'running'
                       AND lease_expires_at IS NOT NULL
                       AND lease_expires_at < NOW())
                ORDER BY enqueued_at
                FOR UPDATE SKIP LOCKED
                LIMIT 1
            )
            UPDATE jobs j
            SET status = 'running', started_at = NOW(), attempts = attempts + 1,
                lease_expires_at = NOW() + make_interval(secs => $2),
                claimed_by = $3
            FROM next_job
            WHERE j.id = next_job.id
            RETURNING j.id, j.image_id, j.kind, j.payload, j.attempts, j.enqueued_at
            ",
        )
        .bind(self.lease_secs as i32)
        .bind(&self.worker_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| JobError::Processing(format!("dequeue: {e}")))?;

        match row {
            Some(r) => Ok(Some(r.try_into()?)),
            None => Ok(None),
        }
    }

    async fn complete(&self, id: Uuid, result: &JobResult) -> Result<(), JobError> {
        let result_json = serde_json::to_string(result)
            .map_err(|e| JobError::Processing(format!("serialize result: {e}")))?;
        sqlx::query(
            r"
            UPDATE jobs
            SET status = 'succeeded', finished_at = NOW(), last_error = NULL,
                payload = $2, lease_expires_at = NULL, claimed_by = NULL
            WHERE id = $1
            ",
        )
        .bind(id.to_string())
        .bind(Some(result_json))
        .execute(&self.pool)
        .await
        .map_err(|e| JobError::Processing(format!("complete: {e}")))?;
        Ok(())
    }

    async fn fail(&self, id: Uuid, error: &str) -> Result<(), JobError> {
        sqlx::query(
            r"
            UPDATE jobs
            SET status = CASE WHEN attempts >= 5 THEN 'dead' ELSE 'pending' END,
                last_error = $2, finished_at = NOW(),
                lease_expires_at = NULL, claimed_by = NULL
            WHERE id = $1
            ",
        )
        .bind(id.to_string())
        .bind(error)
        .execute(&self.pool)
        .await
        .map_err(|e| JobError::Processing(format!("fail: {e}")))?;
        Ok(())
    }
}

/// SQLite-backed job queue (single-process; tests + dev).
#[derive(Debug, Clone)]
pub struct SqliteJobQueue {
    pool: SqlitePool,
    /// Claim duration before a `running` job can be reclaimed (D-8).
    lease_secs: i64,
    /// Identity recorded in `jobs.claimed_by`.
    worker_id: String,
}

impl SqliteJobQueue {
    /// Creates a new queue bound to the given pool (300 s lease).
    pub fn new(pool: SqlitePool) -> Self {
        Self::with_lease(pool, DEFAULT_LEASE_SECS)
    }

    /// Creates a new queue with an explicit lease duration in seconds.
    pub fn with_lease(pool: SqlitePool, lease_secs: u64) -> Self {
        Self {
            pool,
            lease_secs: lease_secs as i64,
            worker_id: default_worker_id(),
        }
    }

    /// Returns a clone of the underlying pool (useful for raw queries).
    pub const fn pool(&self) -> &SqlitePool {
        &self.pool
    }
}

#[async_trait]
impl JobQueue for SqliteJobQueue {
    async fn enqueue(&self, job: Job) -> Result<(), JobError> {
        let kind_json = serde_json::to_string(&job.kind)
            .map_err(|e| JobError::Processing(format!("serialize kind: {e}")))?;
        let res = sqlx::query(
            r"
            INSERT OR IGNORE INTO jobs (id, image_id, kind, payload, status, attempts, enqueued_at)
            VALUES (?1, ?2, ?3, ?4, 'pending', ?5, ?6)
            ",
        )
        .bind(job.id.to_string())
        .bind(job.image_id.as_uuid().to_string())
        .bind(kind_json)
        .bind(Option::<String>::None)
        .bind(job.attempts as i32)
        .bind(job.enqueued_at)
        .execute(&self.pool)
        .await
        .map_err(|e| {
            tracing::error!(error = %e, "sqlite enqueue failed");
            JobError::Processing(format!("enqueue: {e}"))
        })?;
        tracing::info!(job_id = %job.id, image_id = %job.image_id, "enqueued job");
        let _ = res;
        Ok(())
    }

    async fn dequeue(&self) -> Result<Option<Job>, JobError> {
        let row: Option<JobRow> = sqlx::query_as::<_, JobRow>(
            r"
            UPDATE jobs
            SET status = 'running', started_at = CURRENT_TIMESTAMP, attempts = attempts + 1,
                lease_expires_at = strftime('%Y-%m-%d %H:%M:%S', 'now', printf('%+d seconds', ?2)),
                claimed_by = ?3
            WHERE id = (
                SELECT id FROM jobs
                WHERE status = 'pending'
                   OR (status = 'running'
                       AND lease_expires_at IS NOT NULL
                       AND lease_expires_at < strftime('%Y-%m-%d %H:%M:%S', 'now'))
                ORDER BY enqueued_at
                LIMIT 1
            )
            RETURNING id, image_id, kind, payload, attempts, enqueued_at
            ",
        )
        .bind(self.lease_secs as i32)
        .bind(&self.worker_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| JobError::Processing(format!("dequeue: {e}")))?;
        match row {
            Some(r) => Ok(Some(r.try_into()?)),
            None => Ok(None),
        }
    }

    async fn complete(&self, id: Uuid, result: &JobResult) -> Result<(), JobError> {
        let result_json = serde_json::to_string(result)
            .map_err(|e| JobError::Processing(format!("serialize result: {e}")))?;
        sqlx::query(
            r"UPDATE jobs SET status = 'succeeded', finished_at = CURRENT_TIMESTAMP, last_error = NULL, payload = ?2, lease_expires_at = NULL, claimed_by = NULL WHERE id = ?1",
        )
        .bind(id.to_string())
        .bind(Some(result_json))
        .execute(&self.pool)
        .await
        .map_err(|e| JobError::Processing(format!("complete: {e}")))?;
        Ok(())
    }

    async fn fail(&self, id: Uuid, error: &str) -> Result<(), JobError> {
        sqlx::query(
            r"UPDATE jobs SET status = CASE WHEN attempts >= 5 THEN 'dead' ELSE 'pending' END, last_error = ?2, finished_at = CURRENT_TIMESTAMP, lease_expires_at = NULL, claimed_by = NULL WHERE id = ?1",
        )
        .bind(id.to_string())
        .bind(error)
        .execute(&self.pool)
        .await
        .map_err(|e| JobError::Processing(format!("fail: {e}")))?;
        Ok(())
    }
}

/// Row representation of the `jobs` table.
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct JobRow {
    /// Job id (TEXT in DB; parsed into `Uuid`).
    pub id: String,
    /// Image id.
    pub image_id: String,
    /// Job kind JSON (serialized string).
    pub kind: String,
    /// Payload JSON (serialized string).
    pub payload: Option<String>,
    /// Attempt count.
    pub attempts: i32,
    /// Enqueue timestamp.
    pub enqueued_at: OffsetDateTime,
}

impl TryFrom<JobRow> for Job {
    type Error = JobError;
    fn try_from(r: JobRow) -> Result<Self, Self::Error> {
        let id = Uuid::parse_str(&r.id)
            .map_err(|e| JobError::Processing(format!("parse job id: {e}")))?;
        let image_id = Uuid::parse_str(&r.image_id)
            .map_err(|e| JobError::Processing(format!("parse image id: {e}")))?;
        let kind: JobKind = serde_json::from_str(&r.kind)
            .map_err(|e| JobError::Processing(format!("deserialize kind: {e}")))?;
        Ok(Self {
            id,
            image_id: ImageId(image_id),
            kind,
            attempts: r.attempts as u32,
            enqueued_at: r.enqueued_at,
        })
    }
}
