// SPDX-License-Identifier: MIT
// Copyright (c) 2026 Picroom Contributors

//! Admin handlers.

use crate::error::ApiError;
use crate::extractors::auth::AuthUser;
use crate::state::AppState;
use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::Json;
use picroom_audit::{AuditAction, AuditEvent};
use picroom_auth::{PasswordHasher, PermissionAction, ResourceType, Role};
use picroom_domain::{NewUser, PageReq, UserId};
use serde::Deserialize;
use std::str::FromStr;
use std::sync::Arc;
use time::OffsetDateTime;
use uuid::Uuid;

/// Request body for `POST /api/v1/admin/users`.
#[derive(Debug, Deserialize)]
pub struct CreateUserBody {
    /// Email address (used as login id).
    email: String,
    /// Plaintext password (hashed server-side with Argon2id).
    password: String,
    /// Optional display name (defaults to the email local-part).
    #[serde(default)]
    name: Option<String>,
    /// Optional initial role (defaults to `viewer`).
    #[serde(default)]
    role: Option<String>,
}

/// Request body for `PATCH /api/v1/admin/users/:id/role`.
#[derive(Debug, Deserialize)]
pub struct SetRoleBody {
    /// New role name (`viewer`/`uploader`/`manager`/`admin`).
    role: String,
}

/// `POST /api/v1/admin/users` — create a user (admin-only).
pub async fn create_user(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
    Json(body): Json<CreateUserBody>,
) -> Result<(StatusCode, Json<serde_json::Value>), ApiError> {
    state
        .permissions
        .check(&auth.roles, ResourceType::User, PermissionAction::Admin)
        .map_err(ApiError::from)?;

    let repo = state
        .user_repo
        .as_ref()
        .ok_or_else(|| ApiError::not_implemented("user repository not configured"))?;

    // Validate inputs.
    NewUser::validate_email(&body.email)
        .map_err(|e| ApiError::bad_request(format!("invalid email: {e}")))?;
    if body.password.len() < 8 {
        return Err(ApiError::bad_request(
            "password must be at least 8 characters",
        ));
    }
    let role = body.role.clone().unwrap_or_else(|| "viewer".to_string());
    let role = Role::from_str(&role).map_err(|_| ApiError::bad_request("invalid role"))?;

    let password_hash = PasswordHasher::new()
        .hash(&body.password)
        .map_err(|e| ApiError::internal(e.to_string()))?;

    let name = body
        .name
        .clone()
        .filter(|n| !n.is_empty())
        .unwrap_or_else(|| {
            body.email
                .split('@')
                .next()
                .unwrap_or(&body.email)
                .to_string()
        });

    let new = NewUser {
        email: body.email.clone(),
        name,
        password_hash,
        role: role.as_str().to_string(),
    };
    let user = repo.create_user(&new).await.map_err(ApiError::from)?;

    let event = AuditEvent {
        id: Uuid::now_v7(),
        timestamp: OffsetDateTime::now_utc(),
        actor_id: Some(auth.user_id.as_uuid()),
        actor_label: None,
        action: AuditAction::UserCreate,
        target_type: "user".into(),
        target_id: Some(user.id.to_string()),
        ip: None,
        user_agent: None,
        metadata: serde_json::json!({ "email": user.email, "role": user.role }),
    };
    state
        .audit
        .record(&event)
        .await
        .map_err(|e| ApiError::internal(e.to_string()))?;

    Ok((
        StatusCode::CREATED,
        Json(serde_json::json!({
            "id": user.id.to_string(),
            "email": user.email,
            "role": user.role,
        })),
    ))
}

/// `PATCH /api/v1/admin/users/:id/role` — change a user's role (admin-only).
pub async fn set_role(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
    Path(user_id): Path<String>,
    Json(body): Json<SetRoleBody>,
) -> Result<StatusCode, ApiError> {
    state
        .permissions
        .check(&auth.roles, ResourceType::User, PermissionAction::Admin)
        .map_err(ApiError::from)?;

    let repo = state
        .user_repo
        .as_ref()
        .ok_or_else(|| ApiError::not_implemented("user repository not configured"))?;

    let uid = UserId::from_str(&user_id).map_err(|_| ApiError::bad_request("invalid user id"))?;
    let role = Role::from_str(&body.role).map_err(|_| ApiError::bad_request("invalid role"))?;

    repo.set_role(uid, role.as_str())
        .await
        .map_err(ApiError::from)?;

    let event = AuditEvent {
        id: Uuid::now_v7(),
        timestamp: OffsetDateTime::now_utc(),
        actor_id: Some(auth.user_id.as_uuid()),
        actor_label: None,
        action: AuditAction::UserRoleChange,
        target_type: "user".into(),
        target_id: Some(uid.to_string()),
        ip: None,
        user_agent: None,
        metadata: serde_json::json!({ "role": role.as_str() }),
    };
    state
        .audit
        .record(&event)
        .await
        .map_err(|e| ApiError::internal(e.to_string()))?;

    Ok(StatusCode::NO_CONTENT)
}

/// Query parameters for `GET /api/v1/admin/users`.
#[derive(Debug, Default, Deserialize)]
pub struct ListUsersParams {
    /// Page size.
    pub limit: Option<u32>,
    /// Pagination cursor.
    pub cursor: Option<String>,
}

/// `GET /api/v1/admin/users` — list users (admin-only).
pub async fn list_users(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
    Query(params): Query<ListUsersParams>,
) -> Result<Json<serde_json::Value>, ApiError> {
    state
        .permissions
        .check(&auth.roles, ResourceType::User, PermissionAction::Admin)
        .map_err(ApiError::from)?;

    let repo = state
        .user_repo
        .as_ref()
        .ok_or_else(|| ApiError::not_implemented("user repository not configured"))?;

    let page = PageReq {
        limit: params.limit.unwrap_or(50).clamp(1, 200),
        cursor: params.cursor,
    };
    let result = repo.list(page).await.map_err(ApiError::from)?;
    let items: Vec<serde_json::Value> = result
        .items
        .iter()
        .map(|u| {
            serde_json::json!({
                "id": u.id.to_string(),
                "email": u.email,
                "name": u.name,
                "role": u.role,
                "disabled": u.disabled,
                "created_at": u.created_at,
            })
        })
        .collect();

    Ok(Json(serde_json::json!({
        "items": items,
        "has_more": result.has_more,
        "next_cursor": result.next_cursor,
    })))
}

/// `POST /api/v1/admin/users/:id/disable` — disable a user (admin-only).
pub async fn disable_user(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
    Path(user_id): Path<String>,
) -> Result<StatusCode, ApiError> {
    set_user_disabled(&state, &auth, &user_id, true, AuditAction::UserDisable).await
}

/// `POST /api/v1/admin/users/:id/enable` — re-enable a user (admin-only).
pub async fn enable_user(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
    Path(user_id): Path<String>,
) -> Result<StatusCode, ApiError> {
    set_user_disabled(&state, &auth, &user_id, false, AuditAction::UserEnable).await
}

/// Shared body for enable/disable: permission check, repo update, audit record.
async fn set_user_disabled(
    state: &AppState,
    auth: &AuthUser,
    user_id: &str,
    disabled: bool,
    action: AuditAction,
) -> Result<StatusCode, ApiError> {
    state
        .permissions
        .check(&auth.roles, ResourceType::User, PermissionAction::Admin)
        .map_err(ApiError::from)?;

    let repo = state
        .user_repo
        .as_ref()
        .ok_or_else(|| ApiError::not_implemented("user repository not configured"))?;

    let uid = UserId::from_str(user_id).map_err(|_| ApiError::bad_request("invalid user id"))?;
    repo.set_disabled(uid, disabled)
        .await
        .map_err(ApiError::from)?;

    // A disabled user must not keep working: revoke every live session so
    // their outstanding tokens die now, not at expiry (R-20, D-6). A failed
    // cascade is logged, not fatal — the flag itself is set.
    if disabled {
        if let Some(sessions) = &state.session_repo {
            if let Err(e) = sessions.revoke_all_for_user(uid.as_uuid()).await {
                tracing::error!(user_id = %uid, error = %e, "session cascade revoke failed");
            }
        }
    }

    let event = AuditEvent {
        id: Uuid::now_v7(),
        timestamp: OffsetDateTime::now_utc(),
        actor_id: Some(auth.user_id.as_uuid()),
        actor_label: None,
        action,
        target_type: "user".into(),
        target_id: Some(uid.to_string()),
        ip: None,
        user_agent: None,
        metadata: serde_json::json!({ "disabled": disabled }),
    };
    state
        .audit
        .record(&event)
        .await
        .map_err(|e| ApiError::internal(e.to_string()))?;

    Ok(StatusCode::NO_CONTENT)
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
