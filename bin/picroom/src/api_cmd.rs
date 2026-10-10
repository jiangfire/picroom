// SPDX-License-Identifier: MIT
// Copyright (c) 2026 Picroom Contributors

//! `picroom api` subcommand.

use crate::app::{build_deps, DatabaseHandle};
use picroom_api::AppState;
use picroom_service::{PgStoragePolicyRepository, StoragePolicyRepository};
use picroom_storage::StorageWriter;
use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::Arc;

/// Repositories only available on a PostgreSQL connection.
type PgRepos = (
    Option<Arc<dyn picroom_service::SessionRepository>>,
    Option<Arc<dyn picroom_service::ResourceAclRepository>>,
);

/// Runs the API server.
#[allow(clippy::too_many_lines)] // linear dependency wiring
pub async fn run(config: Option<PathBuf>, bind_override: Option<String>) -> anyhow::Result<()> {
    let cfg = picroom_infra::load_config_from(config.as_deref())?;
    picroom_admin::config_cmd::validate_config(&cfg)
        .map_err(|e| anyhow::anyhow!("invalid config: {e}"))?;
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

    let storage_writer: Arc<dyn StorageWriter + Send + Sync> =
        { Arc::new(picroom_api::StorageWriterFromArc(deps.storage.clone())) };

    // Per-user quota enforcement — backed by PostgreSQL; unlimited on SQLite.
    // The default cap mirrors `QuotaConfig::default_user_bytes` so the
    // operator-tunable default is honored (not the hardcoded `DEFAULT_QUOTA`).
    let quota = match &deps.db {
        Some(DatabaseHandle::Pg(pool)) => picroom_service::QuotaService::with_pool(pool.clone())
            .with_default_quota(cfg.quota.default_user_bytes),
        // Q-6: the SQLite dev path enforces quotas too — it must not
        // silently lose enforcement.
        Some(DatabaseHandle::Sqlite(pool)) => {
            picroom_service::QuotaService::with_sqlite_pool(pool.clone())
                .with_default_quota(cfg.quota.default_user_bytes)
        }
        None => picroom_service::QuotaService::new(),
    };

    // Construct UploadService with real audit + quota, honoring the
    // `[pipeline]` toggles: encode_avif/encode_webp gate which variant jobs
    // are enqueued, generate_thumbnail=false disables thumbnails (R-10).
    let mut upload =
        picroom_service::UploadService::new(storage_writer.clone(), deps.audit.clone())
            .with_quota(quota);
    if !cfg.pipeline.encode_avif {
        upload = upload.without_avif();
    }
    if !cfg.pipeline.encode_webp {
        upload = upload.without_webp();
    }
    if !cfg.pipeline.generate_thumbnail {
        upload = upload.with_thumbnails(Vec::new());
    }

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
    // Unified delete service: routes DELETE through storage + repo + audit.
    // Authorization (owner / ACL / RBAC) is enforced inside the service (D-7).
    // Session + ACL repositories are only available on PostgreSQL.
    let (session_repo, acl_repo): PgRepos = match &deps.db {
        Some(DatabaseHandle::Pg(pool)) => (
            Some(Arc::new(picroom_service::PgSessionRepository::new(
                pool.clone(),
            ))),
            Some(Arc::new(picroom_service::PgResourceAclRepository::new(
                pool.clone(),
            ))),
        ),
        _ => (None, None),
    };
    let storage_policy_repo: Option<Arc<dyn StoragePolicyRepository>> = match &deps.db {
        Some(DatabaseHandle::Pg(pool)) => {
            Some(Arc::new(PgStoragePolicyRepository::new(pool.clone())))
        }
        _ => None,
    };
    // Authorization coordinator: engine + ACL + team backends (engine-only in
    // dev mode, where global roles and ownership still apply).
    let authz = match (&acl_repo, &deps.team_repo) {
        (Some(acls), Some(teams)) => Arc::new(picroom_service::AuthzService::new(
            acls.clone(),
            teams.clone(),
        )),
        _ => Arc::new(picroom_service::AuthzService::without_backends()),
    };
    upload = upload.with_authz(&authz);
    let delete_service = deps.image_repo.as_ref().map(|repo| {
        Arc::new(
            picroom_service::DeleteService::new(
                storage_writer.clone(),
                repo.clone(),
                deps.audit.clone(),
            )
            .with_authz(&authz),
        )
    });

    let state = Arc::new(AppState {
        upload: Arc::new(upload),
        image_repo: deps.image_repo.clone(),
        user_repo: deps.user_repo.clone(),
        storage: deps.storage.clone(),
        audit: deps.audit.clone(),
        jwt,
        permissions: Arc::new(picroom_service::PermissionService::new()),
        team_repo: deps.team_repo.clone(),
        audit_reader: deps.audit_reader.clone(),
        delete_service,
        // S3 SigV4 enforcement is opt-in: set PICROOM_S3_ACCESS_KEY_ID +
        // PICROOM_S3_SECRET_ACCESS_KEY to require signed S3 requests.
        s3_credentials: read_s3_credentials(),
        // Bucket scoping (R-15): when the deployment names its bucket
        // (S3_BUCKET / PICROOM_S3_BUCKET), requests for any other bucket
        // answer NoSuchBucket instead of sharing the flat namespace.
        s3_bucket: std::env::var("PICROOM_S3_BUCKET")
            .or_else(|_| std::env::var("S3_BUCKET"))
            .ok()
            .filter(|b| !b.is_empty()),
        public_url_base: cfg.server.public_url_base.clone(),
        storage_policy_repo,
        oidc_providers: Arc::new(cfg.auth.oidc.providers.clone()),
        oidc_admin_emails: Arc::new(cfg.auth.oidc.admin_emails.clone().into_iter().collect()),
        cookie_secure: cfg.auth.oidc.secure_cookies,
        require_sessions: cfg.auth.require_sessions,
        session_repo,
        acl_repo,
        authz,
        // Brute-force protection for the unauthenticated auth endpoints.
        auth_rate_limiter: Arc::new(picroom_api::rate_limit::AuthRateLimiter::new(
            cfg.rate_limit.login_max_attempts,
            cfg.rate_limit.login_window_secs,
        )),
    });

    // Build router with body size limit.
    //
    // `DefaultBodyLimit` is what axum's extractors (`Bytes`, `Multipart`)
    // actually consult — its extractor-level default caps bodies at 2 MiB and
    // `RequestBodyLimitLayer` does not touch it (R-06), so `server.max_body_mb`
    // was unreachable. The transport layer stays as an outer backstop.
    anyhow::ensure!(
        cfg.server.max_body_mb > 0,
        "server.max_body_mb must be greater than 0"
    );
    let max_body_bytes = (cfg.server.max_body_mb as usize) * 1024 * 1024;
    let router = picroom_api::build_router(state)
        .layer(axum::extract::DefaultBodyLimit::max(max_body_bytes))
        .layer(tower_http::limit::RequestBodyLimitLayer::new(
            max_body_bytes,
        ));

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
