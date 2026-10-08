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
use picroom_domain::{PageReq, Team, TeamId, UserId};
use serde::Deserialize;
use serde_json::json;
use std::sync::Arc;
use time::OffsetDateTime;
use uuid::Uuid;

/// Query parameters for `GET /api/v1/teams`.
#[derive(Debug, Default, Deserialize)]
pub struct TeamListParams {
    /// Page size (1-200).
    pub limit: Option<u32>,
    /// Continuation cursor from a previous page.
    pub cursor: Option<String>,
}

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

/// Roles a team manager may assign. Team-level `admin` is deliberately not
/// assignable: `Role::Admin` in `team_roles` would grant everything inside
/// the team scope, which a mere team manager must not hand out.
const ASSIGNABLE_TEAM_ROLES: &[&str] = &["viewer", "uploader", "manager"];

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
///
/// Visible to the caller's members and to principals holding `Team/Read`
/// (manager/admin); anyone else gets 404 — an unscoped read would let any
/// authenticated user enumerate every team (R-13).
pub async fn get(
    State(state): State<Arc<AppState>>,
    Path(id): Path<Uuid>,
    auth: AuthUser,
) -> Result<Json<serde_json::Value>, ApiError> {
    let repo = state
        .team_repo
        .as_ref()
        .ok_or_else(|| ApiError::not_implemented("teams storage not configured"))?;
    let is_member = matches!(
        repo.member_role(TeamId(id), auth.user_id).await,
        Ok(Some(_))
    );
    let can_read_all = state
        .permissions
        .check(&auth.roles, ResourceType::Team, PermissionAction::Read)
        .is_ok();
    if !is_member && !can_read_all {
        // 404, not 403 — do not reveal other teams' existence.
        return Err(ApiError::not_found("team not found"));
    }
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

/// `GET /api/v1/teams` — list teams.
///
/// Returns the caller's teams; principals holding `Team/Read`
/// (manager/admin) may see the full roster of teams (R-13).
pub async fn list(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
    axum::extract::Query(params): axum::extract::Query<TeamListParams>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let repo = state
        .team_repo
        .as_ref()
        .ok_or_else(|| ApiError::not_implemented("teams storage not configured"))?;
    let can_read_all = state
        .permissions
        .check(&auth.roles, ResourceType::Team, PermissionAction::Read)
        .is_ok();
    // R-25: bounded queries — clamp the page like /images. The query params
    // are honored so clients can actually page past the first window.
    let page = PageReq {
        limit: params.limit.unwrap_or(100).clamp(1, 200),
        cursor: params.cursor,
    };
    let teams = if can_read_all {
        repo.list(page).await.map_err(ApiError::from)?
    } else {
        repo.list_for_user(auth.user_id, page)
            .await
            .map_err(ApiError::from)?
    };
    let items: Vec<serde_json::Value> = teams
        .items
        .iter()
        .map(|t| {
            json!({
                "id": t.id.to_string(),
                "name": t.name,
                "slug": t.slug,
                "description": t.description,
                "storage_policy": t.storage_policy,
                "created_at": t.created_at,
            })
        })
        .collect();
    Ok(Json(json!({
        "items": items,
        "has_more": teams.has_more,
        "next_cursor": teams.next_cursor,
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
    let repo = state
        .team_repo
        .as_ref()
        .ok_or_else(|| ApiError::not_implemented("teams storage not configured"))?;
    // Allowed: global `Team/Update` (manager/admin) or a team-level
    // `manager`/`admin` member. Everyone else is an IDOR risk (R-13).
    let global_allowed = state
        .permissions
        .check(&auth.roles, ResourceType::Team, PermissionAction::Update)
        .is_ok();
    let team_role = repo
        .member_role(TeamId(id), auth.user_id)
        .await
        .map_err(ApiError::from)?;
    let team_allowed = matches!(team_role.as_deref(), Some("manager" | "admin"));
    if !global_allowed && !team_allowed {
        return Err(ApiError::forbidden("not allowed"));
    }
    // Validate the role server-side: an unknown string would die in the DB
    // CHECK as a 500, and team-level `admin` is not assignable (see
    // ASSIGNABLE_TEAM_ROLES).
    if !ASSIGNABLE_TEAM_ROLES.contains(&body.role.as_str()) {
        return Err(ApiError::bad_request(format!(
            "invalid team role '{}' (expected one of: {})",
            body.role,
            ASSIGNABLE_TEAM_ROLES.join(", ")
        )));
    }
    repo.add_member(TeamId(id), body.user_id, &body.role)
        .await
        .map_err(ApiError::from)?;
    record_team_event(&state, AuditAction::TeamMemberAdd, id.to_string(), &auth).await;

    Ok(StatusCode::NO_CONTENT)
}

/// `GET /api/v1/teams/:id/members` — list the members of a team.
///
/// Same visibility rule as `GET /teams/:id`: members and `Team/Read`
/// holders only (R-13).
pub async fn list_members(
    State(state): State<Arc<AppState>>,
    Path(id): Path<Uuid>,
    auth: AuthUser,
) -> Result<Json<serde_json::Value>, ApiError> {
    let repo = state
        .team_repo
        .as_ref()
        .ok_or_else(|| ApiError::not_implemented("teams storage not configured"))?;
    let is_member = matches!(
        repo.member_role(TeamId(id), auth.user_id).await,
        Ok(Some(_))
    );
    let can_read_all = state
        .permissions
        .check(&auth.roles, ResourceType::Team, PermissionAction::Read)
        .is_ok();
    if !is_member && !can_read_all {
        return Err(ApiError::not_found("team not found"));
    }
    let members = repo
        .list_members(
            TeamId(id),
            PageReq {
                limit: 500,
                cursor: None,
            },
        )
        .await
        .map_err(ApiError::from)?;
    let items: Vec<serde_json::Value> = members
        .items
        .iter()
        .map(|m| {
            json!({
                "user_id": m.user_id.to_string(),
                "role": m.role,
                "joined_at": m.joined_at,
            })
        })
        .collect();
    Ok(Json(json!({
        "items": items,
        "has_more": members.has_more,
        "next_cursor": members.next_cursor,
    })))
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
