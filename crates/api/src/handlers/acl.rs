// SPDX-License-Identifier: MIT
// Copyright (c) 2026 Picroom Contributors

//! Per-resource ACL management endpoints (D-10).
//!
//! `GET`/`PUT` on `/api/v1/images/:id/acl` and `DELETE` for a single grant.
//! All three are guarded by ownership or `Image/Update` (manager/admin).
//! Grants on other resource types are modelled by the engine and repository
//! but intentionally have no endpoints yet (the table and evaluator are
//! resource-agnostic; see `plan-remediation-v1.md` D-10).

use crate::error::ApiError;
use crate::extractors::auth::AuthUser;
use crate::state::AppState;
use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::Json;
use picroom_auth::{AclEffect, AclSubject, PermissionAction, ResourceType};
use picroom_service::repo::{AclGrant, ResourceAclRepository};
use serde::Deserialize;
use serde_json::json;
use std::sync::Arc;
use uuid::Uuid;

/// Request body for `PUT /api/v1/images/:id/acl` — the full replacement set.
#[derive(Debug, Deserialize)]
pub struct ReplaceAclBody {
    /// The complete grant list; `PUT` is replace-semantics and idempotent.
    pub grants: Vec<GrantBody>,
}

/// One grant in a request or response body.
#[derive(Debug, Clone, Deserialize, serde::Serialize)]
pub struct GrantBody {
    /// `user` or `team`.
    pub subject_type: String,
    /// Subject id (UUID).
    pub subject_id: Uuid,
    /// `read`, `create`, `update`, `delete`, or `admin` (wildcard).
    pub permission: String,
    /// `allow` or `deny` (deny is the highest-priority rule).
    #[serde(default = "default_effect")]
    pub effect: String,
}

fn default_effect() -> String {
    "allow".into()
}

fn grant_to_body(g: &AclGrant) -> GrantBody {
    let (subject_type, subject_id) = match g.subject {
        AclSubject::User(id) => ("user".to_string(), id),
        AclSubject::Team(id) => ("team".to_string(), id),
    };
    GrantBody {
        subject_type,
        subject_id,
        permission: g.action.as_str().to_string(),
        effect: match g.effect {
            AclEffect::Allow => "allow".to_string(),
            AclEffect::Deny => "deny".to_string(),
        },
    }
}

fn body_to_grant(g: &GrantBody) -> Result<AclGrant, ApiError> {
    let subject = match g.subject_type.as_str() {
        "user" => AclSubject::User(g.subject_id),
        "team" => AclSubject::Team(g.subject_id),
        other => {
            return Err(ApiError::bad_request(format!(
                "invalid subject_type: {other} (expected 'user' or 'team')"
            )))
        }
    };
    let action = match g.permission.as_str() {
        "read" => PermissionAction::Read,
        "create" => PermissionAction::Create,
        "update" => PermissionAction::Update,
        "delete" => PermissionAction::Delete,
        "admin" => PermissionAction::Admin,
        other => {
            return Err(ApiError::bad_request(format!(
                "invalid permission: {other}"
            )))
        }
    };
    let effect = match g.effect.as_str() {
        "allow" => AclEffect::Allow,
        "deny" => AclEffect::Deny,
        other => {
            return Err(ApiError::bad_request(format!(
                "invalid effect: {other} (expected 'allow' or 'deny')"
            )))
        }
    };
    Ok(AclGrant {
        subject,
        action,
        effect,
    })
}

/// Loads the image and checks the caller may manage its ACL (owner or
/// `Image/Update`).
async fn authorize_acl_management(
    state: &AppState,
    auth: &AuthUser,
    image_id: Uuid,
) -> Result<picroom_domain::Image, ApiError> {
    let repo = state
        .image_repo
        .as_ref()
        .ok_or_else(|| ApiError::internal("image repo not configured"))?;
    let image = repo.get(picroom_domain::ImageId(image_id)).await?;

    let actor = auth.actor();
    state
        .authz
        .authorize(
            &actor,
            &picroom_auth::Resource::new(
                ResourceType::Image,
                image_id,
                Some(image.owner_id.as_uuid()),
                image.team_id.map(|t| t.as_uuid()),
            ),
            PermissionAction::Update,
        )
        .await
        .map_err(ApiError::from)?;
    Ok(image)
}

fn acl_repo(state: &AppState) -> Result<&Arc<dyn ResourceAclRepository>, ApiError> {
    state
        .acl_repo
        .as_ref()
        .ok_or_else(|| ApiError::not_implemented("ACL storage not configured"))
}

/// `GET /api/v1/images/:id/acl` — list the image's grants.
pub async fn list_acl(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
) -> Result<Json<serde_json::Value>, ApiError> {
    authorize_acl_management(&state, &auth, id).await?;
    let grants = acl_repo(&state)?
        .list_grants(ResourceType::Image.as_str(), id)
        .await
        .map_err(ApiError::from)?;
    let items: Vec<GrantBody> = grants.iter().map(grant_to_body).collect();
    Ok(Json(json!({ "items": items })))
}

/// `PUT /api/v1/images/:id/acl` — replace the full grant set (idempotent).
pub async fn replace_acl(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
    Json(body): Json<ReplaceAclBody>,
) -> Result<Json<serde_json::Value>, ApiError> {
    authorize_acl_management(&state, &auth, id).await?;
    let mut grants = Vec::with_capacity(body.grants.len());
    for g in &body.grants {
        grants.push(body_to_grant(g)?);
    }
    acl_repo(&state)?
        .replace_grants(ResourceType::Image.as_str(), id, &grants)
        .await
        .map_err(ApiError::from)?;
    let items: Vec<GrantBody> = grants.iter().map(grant_to_body).collect();
    Ok(Json(json!({ "items": items })))
}

/// `DELETE /api/v1/images/:id/acl/:subject_type/:subject_id` — remove every
/// grant for one subject on the image.
pub async fn revoke_acl(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
    Path((id, subject_type, subject_id)): Path<(Uuid, String, Uuid)>,
) -> Result<StatusCode, ApiError> {
    authorize_acl_management(&state, &auth, id).await?;
    let subject = match subject_type.as_str() {
        "user" => AclSubject::User(subject_id),
        "team" => AclSubject::Team(subject_id),
        other => {
            return Err(ApiError::bad_request(format!(
                "invalid subject_type: {other}"
            )))
        }
    };
    acl_repo(&state)?
        .revoke(ResourceType::Image.as_str(), id, subject)
        .await
        .map_err(ApiError::from)?;
    Ok(StatusCode::NO_CONTENT)
}
