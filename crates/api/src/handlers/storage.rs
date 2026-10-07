// SPDX-License-Identifier: MIT
// Copyright (c) 2026 Picroom Contributors

//! Storage-policy admin handlers.

use crate::error::ApiError;
use crate::extractors::auth::AuthUser;
use crate::state::AppState;
use axum::extract::State;
use axum::http::StatusCode;
use axum::Json;
use picroom_audit::{AuditAction, AuditEvent};
use picroom_auth::{PermissionAction, ResourceType};
use picroom_service::StoragePolicy;
use serde::Deserialize;
use std::sync::Arc;
use time::OffsetDateTime;
use uuid::Uuid;

/// Request body for `POST /api/v1/admin/storage/policies`.
#[derive(Debug, Deserialize)]
pub struct CreatePolicyBody {
    /// Policy name (unique).
    pub name: String,
    /// Driver kind (`local` / `s3` / `oss` / `cos` / `qiniu` / `minio`).
    pub driver: String,
    /// Driver-specific config object.
    #[serde(default = "default_config")]
    pub config: serde_json::Value,
    /// Mark as the default policy.
    #[serde(default)]
    pub is_default: bool,
}

fn default_config() -> serde_json::Value {
    serde_json::json!({})
}

/// `GET /api/v1/admin/storage/policies` — list storage policies (admin-only).
///
/// Returns `{ "items": [] }` when no DB-backed policy store is configured
/// (e.g. dev mode), so the client always gets a stable shape.
pub async fn list_policies(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
) -> Result<Json<serde_json::Value>, ApiError> {
    state
        .permissions
        .check(
            &auth.roles,
            ResourceType::StoragePolicy,
            PermissionAction::Admin,
        )
        .map_err(ApiError::from)?;

    let Some(repo) = &state.storage_policy_repo else {
        return Ok(Json(serde_json::json!({ "items": [] })));
    };
    let policies = repo.list().await.map_err(ApiError::from)?;
    let items = serde_json::to_value(&policies).map_err(|e| ApiError::internal(e.to_string()))?;
    Ok(Json(serde_json::json!({ "items": items })))
}

/// `POST /api/v1/admin/storage/policies` — create a storage policy (admin-only).
pub async fn create_policy(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
    Json(body): Json<CreatePolicyBody>,
) -> Result<(StatusCode, Json<serde_json::Value>), ApiError> {
    state
        .permissions
        .check(
            &auth.roles,
            ResourceType::StoragePolicy,
            PermissionAction::Admin,
        )
        .map_err(ApiError::from)?;

    if body.name.trim().is_empty() || body.driver.trim().is_empty() {
        return Err(ApiError::bad_request("name and driver are required"));
    }
    // The driver enum is closed in the API contract (R-24): only local and
    // s3 backends exist in v1.
    if !matches!(body.driver.as_str(), "local" | "s3") {
        return Err(ApiError::bad_request(format!(
            "unsupported driver '{}' (expected 'local' or 's3')",
            body.driver
        )));
    }

    let repo = state
        .storage_policy_repo
        .as_ref()
        .ok_or_else(|| ApiError::not_implemented("storage policy repository not configured"))?;

    let policy = StoragePolicy {
        name: body.name.clone(),
        driver: body.driver.clone(),
        config: body.config,
        is_default: body.is_default,
    };
    repo.create(&policy).await.map_err(ApiError::from)?;

    let event = AuditEvent {
        id: Uuid::now_v7(),
        timestamp: OffsetDateTime::now_utc(),
        actor_id: Some(auth.user_id.as_uuid()),
        actor_label: None,
        action: AuditAction::StoragePolicyCreate,
        target_type: "storage_policy".into(),
        target_id: Some(body.name.clone()),
        ip: None,
        user_agent: None,
        metadata: serde_json::json!({ "driver": body.driver, "is_default": body.is_default }),
    };
    state
        .audit
        .record(&event)
        .await
        .map_err(|e| ApiError::internal(e.to_string()))?;

    Ok((
        StatusCode::CREATED,
        Json(serde_json::json!({
            "name": policy.name,
            "driver": policy.driver,
            "is_default": policy.is_default,
        })),
    ))
}
