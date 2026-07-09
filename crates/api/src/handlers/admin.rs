// SPDX-License-Identifier: MIT
// Copyright (c) 2026 Picroom Contributors

//! Admin handlers.

use crate::error::ApiError;
use crate::extractors::auth::AuthUser;
use crate::state::AppState;
use axum::extract::{Query, State};
use axum::http::StatusCode;
use axum::Json;
use picroom_audit::AuditEvent;
use picroom_auth::{PermissionAction, ResourceType};
use serde::Deserialize;
use std::sync::Arc;
use time::OffsetDateTime;

/// `POST /api/v1/admin/users` (admin-only — see RBAC gate below).
pub async fn create_user(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
) -> Result<StatusCode, ApiError> {
    state
        .permissions
        .check(&auth.roles, ResourceType::System, PermissionAction::Admin)
        .map_err(ApiError::from)?;
    Err(ApiError::not_implemented("create_user"))
}

/// `PATCH /api/v1/admin/users/:id/role` (admin-only — see RBAC gate below).
pub async fn set_role(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
) -> Result<StatusCode, ApiError> {
    state
        .permissions
        .check(&auth.roles, ResourceType::System, PermissionAction::Admin)
        .map_err(ApiError::from)?;
    Err(ApiError::not_implemented("set_role"))
}

/// Query parameters for `GET /api/v1/audit`.
#[derive(Debug, Deserialize)]
pub struct AuditQuery {
    /// Max number of events to return.
    pub limit: Option<i64>,
    /// Return only events strictly before this RFC3339 timestamp (cursor).
    pub before: Option<String>,
}

/// `GET /api/v1/audit` — read the audit log (admin-only).
///
/// Reads from the append-only `audit_events` table via the configured
/// [`picroom_audit::AuditReader`].
pub async fn audit(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
    Query(params): Query<AuditQuery>,
) -> Result<Json<Vec<AuditEvent>>, ApiError> {
    state
        .permissions
        .check(&auth.roles, ResourceType::Audit, PermissionAction::Read)
        .map_err(ApiError::from)?;

    let reader = state
        .audit_reader
        .as_ref()
        .ok_or_else(|| ApiError::internal("audit reader not configured"))?;

    let limit = params.limit.unwrap_or(50).clamp(1, 500);
    let before = params.before.as_deref().and_then(|s| {
        OffsetDateTime::parse(s, &time::format_description::well_known::Rfc3339).ok()
    });

    let events = reader
        .list(limit, before)
        .await
        .map_err(|e| ApiError::internal(e.to_string()))?;
    Ok(Json(events))
}
