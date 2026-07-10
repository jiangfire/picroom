// SPDX-License-Identifier: MIT
// Copyright (c) 2026 Picroom Contributors

//! Image handlers.

use crate::error::ApiError;
use crate::extractors::auth::AuthUser;
use crate::state::AppState;
use axum::extract::{Multipart, Path, State};
use axum::http::StatusCode;
use bytes::Bytes;
use picroom_auth::{PermissionAction, ResourceType};
use picroom_domain::{ImageId, TeamId};
use serde_json::json;
use std::sync::Arc;
use uuid::Uuid;

/// `POST /api/v1/images` — multipart upload.
///
/// Accepts a `file` field (binary) and an optional `team_id` form field.
/// The image is attributed to the authenticated user.
pub async fn upload(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
    mut multipart: Multipart,
) -> Result<axum::Json<serde_json::Value>, ApiError> {
    let mut file_bytes: Option<Bytes> = None;
    let mut content_type: Option<String> = None;
    let mut team_id: Option<Uuid> = None;

    while let Some(field) = multipart
        .next_field()
        .await
        .map_err(|e| ApiError::bad_request(format!("multipart error: {e}")))?
    {
        let name = field.name().unwrap_or("").to_string();
        match name.as_str() {
            "file" => {
                content_type = field.content_type().map(std::string::ToString::to_string);
                file_bytes = Some(
                    field
                        .bytes()
                        .await
                        .map_err(|e| ApiError::bad_request(format!("read file: {e}")))?,
                );
            }
            "team_id" => {
                let s = field
                    .text()
                    .await
                    .map_err(|e| ApiError::bad_request(format!("read team_id: {e}")))?;
                team_id = Some(
                    Uuid::parse_str(&s)
                        .map_err(|e| ApiError::bad_request(format!("invalid team_id: {e}")))?,
                );
            }
            _ => {
                // Skip unknown fields.
            }
        }
    }

    let bytes = file_bytes.ok_or_else(|| ApiError::bad_request("missing 'file' field"))?;
    let mime = content_type.unwrap_or_else(|| "application/octet-stream".to_string());

    // Attribute the upload to the authenticated principal (never the dev user).
    let actor = auth.user_id;

    let mut image = match state.upload.upload(actor, &mime, bytes).await {
        Ok(i) => i,
        Err(e) => {
            let s = format!("{e}");
            if s.contains("empty")
                || s.contains("exceeds")
                || s.contains("unsupported")
                || s.contains("probe")
            {
                return Err(ApiError::bad_request(s));
            }
            return Err(ApiError::from(e));
        }
    };

    // Associate the upload with a team when one was supplied.
    image.team_id = team_id.map(TeamId);

    // Persist metadata if a repo is configured.
    if let Some(repo) = &state.image_repo {
        if let Err(e) = repo.insert(&image).await {
            tracing::error!("repo insert failed: {e}");
            // Image is in storage; surface a 500.
            return Err(ApiError::internal(format!("insert failed: {e}")));
        }
    }

    Ok(axum::Json(json!({
        "id": image.id.to_string(),
        "bytes": image.bytes,
        "width": image.width,
        "height": image.height,
        "content_type": image.content_type,
        "team_id": image.team_id.map(|t| t.to_string()),
        "created_at": image.created_at,
    })))
}

/// `GET /api/v1/images` — paginated list.
///
/// Non-admins can only see their own images. Admins may pass an `owner`
/// query parameter to list another user's images.
pub async fn list(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
    axum::extract::Query(params): axum::extract::Query<ListParams>,
) -> Result<axum::Json<serde_json::Value>, ApiError> {
    let Some(repo) = &state.image_repo else {
        return Err(ApiError::internal("image repo not configured"));
    };
    use picroom_domain::PageReq;
    let page = PageReq {
        limit: params.limit.unwrap_or(50).clamp(1, 200),
        cursor: params.cursor.clone(),
    };
    // Default to the caller; users who may manage images (manager/admin via
    // RBAC) may override to view another owner.
    let can_list_others = state
        .permissions
        .check(&auth.roles, ResourceType::Image, PermissionAction::Update)
        .is_ok();
    let owner = match params.owner {
        Some(owner) if can_list_others => owner,
        _ => auth.user_id.as_uuid(),
    };
    let images = repo
        .list_for_owner(owner, page)
        .await
        .map_err(ApiError::from)?;

    let items: Vec<_> = images
        .items
        .into_iter()
        .map(|i| {
            json!({
                "id": i.id.to_string(),
                "content_type": i.content_type,
                "bytes": i.bytes,
                "width": i.width,
                "height": i.height,
                "team_id": i.team_id.map(|t| t.to_string()),
                "created_at": i.created_at,
            })
        })
        .collect();

    Ok(axum::Json(json!({
        "items": items,
        "has_more": images.has_more,
        "next_cursor": images.next_cursor,
    })))
}

/// Query parameters for list.
#[derive(Debug, Default, serde::Deserialize)]
pub struct ListParams {
    /// Owner user id (defaults to dev user).
    pub owner: Option<Uuid>,
    /// Page size.
    pub limit: Option<u32>,
    /// Pagination cursor.
    pub cursor: Option<String>,
}

/// `GET /api/v1/images/:id` — fetch image metadata.
pub async fn get(
    State(state): State<Arc<AppState>>,
    Path(id): Path<Uuid>,
    auth: AuthUser,
) -> Result<axum::Json<serde_json::Value>, ApiError> {
    let Some(repo) = &state.image_repo else {
        return Err(ApiError::internal("image repo not configured"));
    };
    let image = repo.get(ImageId(id)).await.map_err(ApiError::from)?;
    // IDOR check: only the owner, or a principal permitted to manage images
    // (manager/admin via RBAC), may view this image.
    if auth.user_id != image.owner_id
        && state
            .permissions
            .check(&auth.roles, ResourceType::Image, PermissionAction::Update)
            .is_err()
    {
        return Err(ApiError::forbidden("not allowed"));
    }
    Ok(axum::Json(json!({
        "id": image.id.to_string(),
        "content_type": image.content_type,
        "bytes": image.bytes,
        "width": image.width,
        "height": image.height,
        "owner_id": image.owner_id.to_string(),
        "team_id": image.team_id.map(|t| t.to_string()),
        "created_at": image.created_at,
    })))
}

/// `DELETE /api/v1/images/:id` — delete an image.
pub async fn delete(
    State(state): State<Arc<AppState>>,
    Path(id): Path<Uuid>,
    auth: AuthUser,
) -> Result<StatusCode, ApiError> {
    let Some(repo) = &state.image_repo else {
        return Err(ApiError::internal("image repo not configured"));
    };
    let image = repo.get(ImageId(id)).await.map_err(ApiError::from)?;
    // IDOR check: only the owner, or a principal permitted to delete images
    // (manager/admin via RBAC), may delete this image.
    if auth.user_id != image.owner_id
        && state
            .permissions
            .check(&auth.roles, ResourceType::Image, PermissionAction::Delete)
            .is_err()
    {
        return Err(ApiError::forbidden("not allowed"));
    }
    // Route deletion through the unified DeleteService (storage + DB + audit).
    // The already-fetched `image` is passed in so we don't look it up twice.
    match &state.delete_service {
        Some(svc) => svc.delete(image).await.map_err(ApiError::from)?,
        None => {
            // Defensive fallback for environments without a DB-backed service.
            if let Err(e) = state.storage.delete(&image.key).await {
                tracing::warn!("storage delete failed: {e}");
            }
        }
    }
    Ok(StatusCode::NO_CONTENT)
}
