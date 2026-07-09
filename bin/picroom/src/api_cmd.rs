// SPDX-License-Identifier: MIT
// Copyright (c) 2026 Picroom Contributors

//! `picroom api` subcommand.

use crate::app::{build_deps, DatabaseHandle};
use picroom_api::AppState;
use picroom_storage::StorageWriter;
use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::Arc;

/// Runs the API server.
pub async fn run(config: Option<PathBuf>, bind_override: Option<String>) -> anyhow::Result<()> {
    let cfg = picroom_infra::load_config_from(config.as_deref())?;
    picroom_infra::init_logging(&cfg.logging.level, &cfg.logging.format);
    picroom_infra::init_metrics();

    // Security: never log the full DB URL (it may contain credentials).
    tracing::info!(
        db_scheme = schema_of(&cfg.database.url),
        "database configured"
    );

    let bind_addr = bind_override.unwrap_or_else(|| cfg.server.bind_addr.clone());
    let addr: SocketAddr = bind_addr.parse()?;

    // Build all dependencies from config.
    let deps = build_deps(&cfg).await?;

    // Construct UploadService with real audit + optional job queue.
    let storage_writer: Arc<dyn StorageWriter + Send + Sync> = {
        // We wrap storage in a StorageWriter adapter (Arc<S> → StorageWriter).
        // Since Storage super-impls StorageWriter, we use the adapter from state.rs.
        Arc::new(picroom_api::StorageWriterFromArc(deps.storage.clone()))
    };
    let mut upload = picroom_service::UploadService::new(storage_writer, deps.audit.clone());

    // Optionally wire job queue.
    if let Some(db) = &deps.db {
        match db {
            DatabaseHandle::Pg(pool) => {
                let q: Arc<dyn picroom_worker::JobQueue + Send + Sync> =
                    Arc::new(picroom_worker::db_queue::PgJobQueue::new(pool.clone()));
                upload = upload.with_job_queue(q);
                tracing::info!("job queue connected (PostgreSQL)");
            }
            DatabaseHandle::Sqlite(pool) => {
                let q: Arc<dyn picroom_worker::JobQueue + Send + Sync> =
                    Arc::new(picroom_worker::SqliteJobQueue::new(pool.clone()));
                upload = upload.with_job_queue(q);
                tracing::info!("job queue connected (SQLite)");
            }
        }
    }

    // Build AppState with JWT service.
    // Security: refuse to start with the default dev secret in release mode.
    picroom_infra::require_strong_jwt_secret(&cfg).map_err(|e| anyhow::anyhow!("{e}"))?;
    let jwt = Arc::new(picroom_auth::JwtService::new(
        cfg.auth.jwt_secret.clone(),
        cfg.auth.jwt_issuer.clone(),
        cfg.auth.jwt_audience.clone(),
        cfg.auth.jwt_ttl_secs,
    ));
    let state = Arc::new(AppState {
        upload: Arc::new(upload),
        image_repo: deps.image_repo.clone(),
        user_repo: deps.user_repo.clone(),
        storage: deps.storage.clone(),
        audit: deps.audit.clone(),
        jwt,
        // S3 SigV4 enforcement is opt-in: set PICROOM_S3_ACCESS_KEY_ID +
        // PICROOM_S3_SECRET_ACCESS_KEY to require signed S3 requests.
        s3_credentials: read_s3_credentials(),
    });

    // Build router with body size limit.
    let max_body_bytes = (cfg.server.max_body_mb as usize) * 1024 * 1024;
    let router = picroom_api::build_router(state).layer(
        tower_http::limit::RequestBodyLimitLayer::new(max_body_bytes),
    );

    tracing::info!("picroom api listening on {addr}");

    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(listener, router)
        .with_graceful_shutdown(crate::shutdown::shutdown_signal())
        .await?;

    Ok(())
}

/// Extracts just the scheme from a URL for safe logging.
fn schema_of(url: &str) -> &str {
    if let Some(idx) = url.find("://") {
        &url[..idx]
    } else {
        "unknown"
    }
}

/// Reads an optional S3 client credential from the environment. When both
/// `PICROOM_S3_ACCESS_KEY_ID` and `PICROOM_S3_SECRET_ACCESS_KEY` are present,
/// the S3 endpoint verifies `SigV4` signatures against them.
fn read_s3_credentials() -> Option<picroom_s3compat::S3Credential> {
    let access_key = std::env::var("PICROOM_S3_ACCESS_KEY_ID").ok()?;
    let secret = std::env::var("PICROOM_S3_SECRET_ACCESS_KEY").ok()?;
    if access_key.is_empty() || secret.is_empty() {
        return None;
    }
    Some(picroom_s3compat::S3Credential { access_key, secret })
}
