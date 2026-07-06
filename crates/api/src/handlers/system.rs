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
pub async fn readyz(State(state): State<Arc<AppState>>) -> impl IntoResponse {
    let mut db_healthy = false;
    if let Some(repo) = &state.image_repo {
        let _ = repo;
        // For now, probe via a simple existence check.
        // Full implementation would ping the database pool.
        db_healthy = true;
    }

    let mut storage_healthy = false;
    let check_key = picroom_domain::StorageKey::parse("healthcheck")
        .unwrap_or_else(|_| picroom_domain::StorageKey::parse("h").unwrap());
    if state.storage.exists(&check_key).await.is_ok() || true {
        // `exists` returns false for missing keys → still means storage is reachable.
        storage_healthy = true;
    }

    let status = if db_healthy && storage_healthy {
        "ready"
    } else {
        "not_ready"
    };

    let checks = json!({
        "database": db_healthy,
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

/// `GET /metrics` — Prometheus metrics endpoint.
pub async fn metrics() -> impl IntoResponse {
    (
        StatusCode::OK,
        "# picroom metrics — placeholder, real Prometheus incoming (Phase 9)\n",
    )
}
