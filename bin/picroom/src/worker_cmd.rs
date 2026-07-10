// SPDX-License-Identifier: MIT
// Copyright (c) 2026 Picroom Contributors

//! `picroom worker` subcommand.

use crate::app::{build_deps, DatabaseHandle};
use async_trait::async_trait;
use bytes::Bytes;
use picroom_domain::{Image, ImageId, StorageKey, UserId};
use picroom_service::{ImageRepository, PgImageRepository};
use picroom_worker::processor::ImageLookup;
use picroom_worker::{Job, JobError, JobQueue, JobResult};
use picroom_worker::{ProcessorDeps, RetryPolicy, WorkerPool};
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;
use time::OffsetDateTime;
use uuid::Uuid;

/// Image lookup backed by the image repository (Postgres), with a
/// convention-based fallback for environments without a DB repo (SQLite-only
/// or test runs).
struct DbImageLookup {
    repo: Option<Arc<dyn ImageRepository>>,
}

#[async_trait]
impl ImageLookup for DbImageLookup {
    async fn lookup(&self, id: ImageId) -> Result<Image, String> {
        match &self.repo {
            Some(repo) => repo
                .get(id)
                .await
                .map_err(|e| format!("db lookup {id}: {e}")),
            None => convention_lookup(id),
        }
    }
}

/// Fallback used only when no DB repository is configured. Synthesizes
/// metadata purely from the storage key convention `img/{id}.bin`.
fn convention_lookup(id: ImageId) -> Result<Image, String> {
    let key =
        StorageKey::parse(&format!("img/{}.bin", id.as_uuid())).map_err(|e| format!("key: {e}"))?;
    Ok(Image {
        id,
        owner_id: UserId(uuid::Uuid::nil()),
        team_id: None,
        key,
        content_type: "image/png".into(),
        bytes: 0,
        width: 0,
        height: 0,
        sha256: None,
        variants: vec![],
        created_at: time::OffsetDateTime::now_utc(),
    })
}

/// In-memory DLQ.
struct InMemoryDlq;

#[async_trait]
impl picroom_worker::DlqSink for InMemoryDlq {
    async fn push(&self, entry: picroom_worker::DlqEntry) -> Result<(), String> {
        tracing::error!(job_id = %entry.job_id, "DLQ");
        Ok(())
    }
}

/// Enum-dispatch queue so we can pass it to `WorkerPool`<Q, D>.
enum AnyQueue {
    Pg(picroom_worker::db_queue::PgJobQueue),
    Sqlite(picroom_worker::SqliteJobQueue),
}

#[async_trait]
impl JobQueue for AnyQueue {
    async fn enqueue(&self, job: Job) -> Result<(), JobError> {
        match self {
            Self::Pg(q) => q.enqueue(job).await,
            Self::Sqlite(q) => q.enqueue(job).await,
        }
    }
    async fn dequeue(&self) -> Result<Option<Job>, JobError> {
        match self {
            Self::Pg(q) => q.dequeue().await,
            Self::Sqlite(q) => q.dequeue().await,
        }
    }
    async fn complete(&self, id: Uuid, result: &JobResult) -> Result<(), JobError> {
        match self {
            Self::Pg(q) => q.complete(id, result).await,
            Self::Sqlite(q) => q.complete(id, result).await,
        }
    }
    async fn fail(&self, id: Uuid, error: &str) -> Result<(), JobError> {
        match self {
            Self::Pg(q) => q.fail(id, error).await,
            Self::Sqlite(q) => q.fail(id, error).await,
        }
    }
}

/// Runs the async image-processing worker pool.
pub async fn run(config: Option<PathBuf>, concurrency: usize) -> anyhow::Result<()> {
    let cfg = picroom_infra::load_config_from(config.as_deref())?;
    picroom_infra::init_logging(&cfg.logging.level, &cfg.logging.format);
    picroom_infra::init_metrics();
    // Security: workers also issue/accept JWT-adjacent state; refuse the
    // default secret in release builds (mirrors the API binary).
    picroom_infra::require_strong_jwt_secret(&cfg).map_err(|e| anyhow::anyhow!("{e}"))?;

    tracing::info!("picroom worker starting (concurrency={concurrency})");

    let deps = build_deps(&cfg).await?;

    let queue = match &deps.db {
        Some(DatabaseHandle::Pg(pool)) => {
            tracing::info!("job queue connected (PostgreSQL)");
            AnyQueue::Pg(picroom_worker::db_queue::PgJobQueue::new(pool.clone()))
        }
        Some(DatabaseHandle::Sqlite(pool)) => {
            tracing::info!("job queue connected (SQLite)");
            AnyQueue::Sqlite(picroom_worker::SqliteJobQueue::new(pool.clone()))
        }
        None => {
            tracing::warn!("no database available; worker has nothing to consume");
            return Ok(());
        }
    };

    let dlq = InMemoryDlq;

    let variant_repo: Option<Arc<dyn picroom_worker::VariantRepository + Send + Sync>> =
        match &deps.db {
            Some(DatabaseHandle::Pg(pool)) => Some(Arc::new(
                picroom_service::PgVariantRepository::new(pool.clone()),
            )),
            _ => None,
        };

    // Image metadata lookup: prefer the DB repository (real owner_id /
    // content_type / storage key), falling back to the key convention when no
    // repo is available (SQLite-only or test environments).
    let image_repo: Option<Arc<dyn ImageRepository>> = match &deps.db {
        Some(DatabaseHandle::Pg(pool)) => Some(Arc::new(PgImageRepository::new(pool.clone()))),
        _ => None,
    };
    let lookup: Arc<dyn ImageLookup + Send + Sync> = Arc::new(DbImageLookup { repo: image_repo });

    let deps_arc = Arc::new(ProcessorDeps {
        image_lookup: lookup,
        storage: deps.storage.clone(),
        dlq: Some(Arc::new(dlq) as Arc<dyn picroom_worker::DlqSink + Send + Sync>),
        variant_repo,
    });

    let pool = WorkerPool::new(
        Arc::new(queue),
        Arc::new(InMemoryDlq),
        RetryPolicy::default(),
        concurrency,
    );

    picroom_worker::pool::run_until(
        &pool,
        async {
            crate::shutdown::shutdown_signal().await;
        },
        move |job| {
            let deps = deps_arc.clone();
            async move { picroom_worker::ImageProcessor::process(&deps, job).await }
        },
    )
    .await;

    tracing::info!("worker shut down");
    Ok(())
}

#[allow(dead_code)]
fn _unused(_b: Bytes, _i: Image, _k: StorageKey, _d: Duration, _t: OffsetDateTime, _u: Uuid) {}
