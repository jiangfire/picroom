// SPDX-License-Identifier: MIT
// Copyright (c) 2026 Picroom Contributors

//! System endpoints (health, readiness, metrics).

use axum::extract::State;
use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::Json;
use serde_json::json;
use std::sync::Arc;

use crate::state::AppState;

/// `GET /healthz` — liveness probe (always 200 if the process is alive).
pub async fn healthz() -> impl IntoResponse {
    (StatusCode::OK, Json(json!({"status": "ok"})))
}

/// `GET /readyz` — readiness probe (checks DB and storage).
///
/// Pings the database via `SELECT 1` when a repository is configured and
/// probes storage with `exists()`. When no DB is configured (single-node dev
/// without persistence) the database check is reported as not-applicable and
/// does not fail readiness.
pub async fn readyz(State(state): State<Arc<AppState>>) -> impl IntoResponse {
    let (db_healthy, db_reported) = match &state.image_repo {
        Some(repo) => (repo.ping().await.is_ok(), true),
        None => (true, false),
    };

    // `exists` returns Ok(false) for missing keys → still means storage is
    // reachable. The probe key is a known-valid literal.
    let check_key =
        picroom_domain::StorageKey::parse("healthcheck").expect("healthcheck is a valid key");
    let storage_healthy = state.storage.exists(&check_key).await.is_ok();

    let status = if db_healthy && storage_healthy {
        "ready"
    } else {
        "not_ready"
    };

    let checks = json!({
        "database": db_healthy,
        "database_configured": db_reported,
        "storage": storage_healthy,
    });

    let status_code = if status == "ready" {
        StatusCode::OK
    } else {
        StatusCode::SERVICE_UNAVAILABLE
    };

    (
        status_code,
        Json(json!({"status": status, "checks": checks})),
    )
}

/// `GET /metrics` — Prometheus exposition format.
pub async fn metrics() -> impl IntoResponse {
    (
        StatusCode::OK,
        [("content-type", "text/plain; version=0.0.4")],
        picroom_infra::render_metrics(),
    )
}
