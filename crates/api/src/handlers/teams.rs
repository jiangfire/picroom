// SPDX-License-Identifier: MIT
// Copyright (c) 2026 Picroom Contributors

//! Team handlers.

use crate::error::ApiError;
use crate::extractors::auth::AuthUser;
use crate::state::AppState;
use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::Json;
use picroom_audit::{AuditAction, AuditEvent};
use picroom_auth::{PermissionAction, ResourceType};
use picroom_domain::{Team, TeamId, UserId};
use serde::Deserialize;
use serde_json::json;
use std::sync::Arc;
use time::OffsetDateTime;
use uuid::Uuid;

/// Request body for `POST /api/v1/teams`.
#[derive(Debug, Deserialize)]
pub struct CreateTeamBody {
    /// Display name.
    pub name: String,
    /// URL-safe slug.
    pub slug: String,
    /// Optional description.
    #[serde(default)]
    pub description: Option<String>,
}

/// Request body for `POST /api/v1/teams/:id/members`.
#[derive(Debug, Deserialize)]
pub struct AddMemberBody {
    /// User id to add.
    pub user_id: UserId,
    /// Role within the team.
    #[serde(default = "default_member_role")]
    pub role: String,
}

fn default_member_role() -> String {
    "uploader".into()
}

/// `POST /api/v1/teams` — create a team.
///
/// Any authenticated user may create a team (MVP). The action is recorded in
/// the audit log.
pub async fn create(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
    Json(body): Json<CreateTeamBody>,
) -> Result<(StatusCode, Json<serde_json::Value>), ApiError> {
    let repo = state
        .team_repo
        .as_ref()
        .ok_or_else(|| ApiError::not_implemented("teams storage not configured"))?;

    let team = Team {
        id: TeamId(Uuid::now_v7()),
        name: body.name,
        slug: body.slug,
        description: body.description,
        storage_policy: None,
        created_at: OffsetDateTime::now_utc(),
    };

    repo.create(&team).await.map_err(ApiError::from)?;
    record_team_event(&state, AuditAction::TeamCreate, team.id.to_string(), &auth).await;

    Ok((
        StatusCode::CREATED,
        Json(json!({ "id": team.id.to_string(), "slug": team.slug })),
    ))
}

/// `GET /api/v1/teams/:id` — fetch a team.
pub async fn get(
    State(state): State<Arc<AppState>>,
    Path(id): Path<Uuid>,
    _auth: AuthUser,
) -> Result<Json<serde_json::Value>, ApiError> {
    let repo = state
        .team_repo
        .as_ref()
        .ok_or_else(|| ApiError::not_implemented("teams storage not configured"))?;
    let team = repo.get(TeamId(id)).await.map_err(ApiError::from)?;
    Ok(Json(json!({
        "id": team.id.to_string(),
        "name": team.name,
        "slug": team.slug,
        "description": team.description,
        "storage_policy": team.storage_policy,
        "created_at": team.created_at,
    })))
}

/// `POST /api/v1/teams/:id/members` — add (or update) a member.
///
/// Requires the `Team::Update` permission (manager/admin via RBAC).
pub async fn add_member(
    State(state): State<Arc<AppState>>,
    Path(id): Path<Uuid>,
    auth: AuthUser,
    Json(body): Json<AddMemberBody>,
) -> Result<StatusCode, ApiError> {
    state
        .permissions
        .check(&auth.roles, ResourceType::Team, PermissionAction::Update)
        .map_err(ApiError::from)?;

    let repo = state
        .team_repo
        .as_ref()
        .ok_or_else(|| ApiError::not_implemented("teams storage not configured"))?;
    repo.add_member(TeamId(id), body.user_id, &body.role)
        .await
        .map_err(ApiError::from)?;
    record_team_event(&state, AuditAction::TeamMemberAdd, id.to_string(), &auth).await;

    Ok(StatusCode::NO_CONTENT)
}

/// Records a team-related audit event (best-effort; failures are logged, not fatal).
async fn record_team_event(
    state: &AppState,
    action: AuditAction,
    target_id: String,
    auth: &AuthUser,
) {
    let event = AuditEvent {
        id: Uuid::now_v7(),
        timestamp: OffsetDateTime::now_utc(),
        actor_id: Some(auth.user_id.as_uuid()),
        actor_label: None,
        action,
        target_type: "team".into(),
        target_id: Some(target_id),
        ip: None,
        user_agent: None,
        metadata: serde_json::Value::Null,
    };
    if let Err(e) = state.audit.record(&event).await {
        tracing::warn!(error = %e, "failed to record team audit event");
    }
}
