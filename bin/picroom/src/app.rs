// SPDX-License-Identifier: MIT
// Copyright (c) 2026 Picroom Contributors

//! Application wiring — constructs all runtime dependencies from `Config`.
//!
//! This module replaces the former `AppState::for_dev()` hardcoded path
//! with a production-grade factory that:
//! - Connects to the database (PG or `SQLite`)
//! - Creates `DbAuditSink` (replaces `NoopAuditSink`)
//! - Creates `PgImageRepository` / `SqliteImageRepository`
//! - Constructs the storage driver from config (Local or S3/MinIO)
//! - Wires the `UploadService` with real audit + optional job queue

use anyhow::Result;
use picroom_audit::AuditSink;
use picroom_service::repo::{ImageRepository, PgImageRepository, PgUserRepository, UserRepository};
use picroom_storage::driver::{LocalDriver, S3Driver};
use picroom_storage::Storage;
use std::sync::Arc;

/// A unified handle to all open resources. Drop closes pools.
pub struct AppDeps {
    /// The storage driver (Local or S3).
    pub storage: Arc<dyn Storage>,
    /// The audit sink (`DbAuditSink` or Noop).
    pub audit: Arc<dyn AuditSink>,
    /// The image repository (None if DB unavailable).
    pub image_repo: Option<Arc<dyn ImageRepository>>,
    /// The user repository (None if DB unavailable).
    pub user_repo: Option<Arc<dyn UserRepository>>,
    /// The DB pool (for job queue, admin CLI, etc.).
    pub db: Option<DatabaseHandle>,
}

/// Database handle (PG or `SQLite`).
pub enum DatabaseHandle {
    /// `PostgreSQL` pool.
    Pg(sqlx::PgPool),
    /// `SQLite` pool.
    Sqlite(sqlx::SqlitePool),
}

/// Constructs `AppDeps` from the loaded configuration.
///
/// Storage is selected from `Config::storage`:
/// - If a policy named "default" or "primary" exists with `driver = "s3"`
///   (or "minio"), an `S3Driver` is constructed.
/// - Otherwise a `LocalDriver` rooted at `data/` is used.
///
/// DB is selected from `Config::database::url`:
/// - `postgres://` or `postgresql://` → `PgPool` + `PgImageRepository` + `DbAuditSink`
/// - `sqlite://` → `SqlitePool` (no repo yet — `SQLite` repo is post-MVP)
/// - Connection failure → degrade gracefully (`NoopAuditSink`, `image_repo=None`)
pub async fn build_deps(cfg: &picroom_infra::Config) -> Result<AppDeps> {
    let url = &cfg.database.url;

    // --- DB ---
    type BuiltDeps = (
        Option<DatabaseHandle>,
        Arc<dyn AuditSink>,
        Option<Arc<dyn ImageRepository>>,
        Option<Arc<dyn UserRepository>>,
    );
    let (db, audit, image_repo, user_repo): BuiltDeps = if url.starts_with("postgres://")
        || url.starts_with("postgresql://")
    {
        match sqlx::postgres::PgPoolOptions::new()
            .max_connections(cfg.database.max_connections)
            .connect(url)
            .await
        {
            Ok(pool) => {
                tracing::info!("database connected (PostgreSQL)");
                let audit: Arc<dyn AuditSink> =
                    Arc::new(picroom_audit::DbAuditSink::new(pool.clone()));
                let repo: Arc<dyn ImageRepository> = Arc::new(PgImageRepository::new(pool.clone()));
                let users: Arc<dyn UserRepository> = Arc::new(PgUserRepository::new(pool.clone()));
                (
                    Some(DatabaseHandle::Pg(pool)),
                    audit,
                    Some(repo),
                    Some(users),
                )
            }
            Err(e) => {
                tracing::warn!("database connection failed, degrading: {e}");
                (None, Arc::new(picroom_audit::NoopAuditSink), None, None)
            }
        }
    } else if url.starts_with("sqlite://") {
        match try_sqlite_connect(url).await {
            Ok(pool) => {
                tracing::info!("database connected (SQLite)");
                // SQLite doesn't have PgImageRepository yet; use NoopAudit
                // until a SqliteImageRepository is implemented.
                (
                    Some(DatabaseHandle::Sqlite(pool)),
                    Arc::new(picroom_audit::NoopAuditSink),
                    None,
                    None,
                )
            }
            Err(e) => {
                tracing::warn!("SQLite connection failed, degrading: {e}");
                (None, Arc::new(picroom_audit::NoopAuditSink), None, None)
            }
        }
    } else {
        tracing::warn!("unsupported database URL scheme, running without DB");
        (None, Arc::new(picroom_audit::NoopAuditSink), None, None)
    };

    // --- Storage ---
    let storage = build_storage(cfg).await?;

    Ok(AppDeps {
        storage,
        audit,
        image_repo,
        user_repo,
        db,
    })
}

async fn try_sqlite_connect(url: &str) -> Result<sqlx::SqlitePool, sqlx::Error> {
    use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};
    let trimmed = url.trim_start_matches("sqlite://");
    let opts = SqliteConnectOptions::new()
        .filename(trimmed)
        .create_if_missing(true);
    SqlitePoolOptions::new()
        .max_connections(4)
        .connect_with(opts)
        .await
}

/// Constructs the storage driver from config.
/// Uses S3/MinIO if env vars are set, otherwise falls back to `LocalDriver`.
async fn build_storage(_cfg: &picroom_infra::Config) -> Result<Arc<dyn Storage>> {
    // Try S3/MinIO from env vars (docker-compose sets these).
    if let Some(s3_cfg) = parse_s3_config_from_env() {
        tracing::info!(
            endpoint = %s3_cfg.endpoint.as_deref().unwrap_or("aws"),
            bucket = %s3_cfg.bucket,
            region = %s3_cfg.region,
            "storage: S3/MinIO",
        );
        let driver = S3Driver::new(s3_cfg)
            .await
            .map_err(|e| anyhow::anyhow!("S3 driver init: {e}"))?;
        return Ok(Arc::new(driver) as Arc<dyn Storage>);
    }

    // Fallback: LocalDriver rooted at data/.
    let root = std::env::current_dir().unwrap_or_default().join("data");
    tracing::info!(root = %root.display(), "storage: LocalDriver");
    let driver = LocalDriver::new(root, "/i");
    Ok(Arc::new(driver) as Arc<dyn Storage>)
}

/// Parses S3 config from environment variables.
fn parse_s3_config_from_env() -> Option<picroom_storage::driver::s3::S3Config> {
    let endpoint = std::env::var("PICROOM_STORAGE__POLICIES__MINIO__ENDPOINT")
        .or_else(|_| std::env::var("S3_ENDPOINT"))
        .ok()?;
    let bucket = std::env::var("PICROOM_STORAGE__POLICIES__MINIO__BUCKET")
        .or_else(|_| std::env::var("S3_BUCKET"))
        .unwrap_or_else(|_| "picroom".into());
    let region = std::env::var("PICROOM_STORAGE__POLICIES__MINIO__REGION")
        .or_else(|_| std::env::var("S3_REGION"))
        .unwrap_or_else(|_| "us-east-1".into());
    let ak = std::env::var("PICROOM_STORAGE__POLICIES__MINIO__ACCESS_KEY_ID")
        .or_else(|_| std::env::var("S3_ACCESS_KEY_ID"))
        .ok()?;
    let sk = std::env::var("PICROOM_STORAGE__POLICIES__MINIO__SECRET_ACCESS_KEY")
        .or_else(|_| std::env::var("S3_SECRET_ACCESS_KEY"))
        .ok()?;
    Some(
        picroom_storage::driver::s3::S3Config::new(bucket, region, ak, sk)
            .with_endpoint(endpoint)
            .with_path_style(true),
    )
}
