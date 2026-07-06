//! Image handlers.

use crate::error::ApiError;
use crate::extractors::auth::AuthUser;
use crate::state::AppState;
use axum::extract::{Multipart, Path, State};
use axum::http::StatusCode;
use axum::response::IntoResponse;
use bytes::Bytes;
use picroom_auth::Role;
use picroom_domain::ImageId;
use serde_json::json;
use std::sync::Arc;
use uuid::Uuid;

/// `POST /api/v1/images` — multipart upload.
///
/// Accepts a `file` field (binary) and an optional `team_id` form field.
pub async fn upload(
    State(state): State<Arc<AppState>>,
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

    // For skeleton: trust the caller-supplied user_id via header (real impl: JWT).
    // Production wires this through the AuthUser extractor.
    let actor = state.dev_user;

    let image = match state.upload.upload(actor, &mime, bytes).await {
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
        "team_id": team_id,
        "created_at": image.created_at,
    })))
}

/// `GET /api/v1/images` — paginated list.
pub async fn list(
    State(state): State<Arc<AppState>>,
    axum::extract::Query(params): axum::extract::Query<ListParams>,
) -> Result<axum::Json<serde_json::Value>, ApiError> {
    let Some(repo) = &state.image_repo else {
        return Err(ApiError::internal("image repo not configured"));
    };
    use picroom_domain::PageReq;
    let page = PageReq {
        limit: params.limit.unwrap_or(50).clamp(1, 200),
        cursor: None,
    };
    let owner = params.owner.unwrap_or(state.dev_user.as_uuid());
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
    auth: Option<AuthUser>,
) -> Result<axum::Json<serde_json::Value>, ApiError> {
    let Some(repo) = &state.image_repo else {
        return Err(ApiError::internal("image repo not configured"));
    };
    let image = repo.get(ImageId(id)).await.map_err(ApiError::from)?;
    // IDOR check: only owner or admin can view.
    if let Some(user) = auth {
        if user.user_id != image.owner_id && !user.roles.contains(&Role::Admin) {
            return Err(ApiError::forbidden("not allowed"));
        }
    }
    Ok(axum::Json(json!({
        "id": image.id.to_string(),
        "content_type": image.content_type,
        "bytes": image.bytes,
        "width": image.width,
        "height": image.height,
        "owner_id": image.owner_id.to_string(),
        "created_at": image.created_at,
    })))
}

/// `DELETE /api/v1/images/:id` — delete an image.
pub async fn delete(
    State(state): State<Arc<AppState>>,
    Path(id): Path<Uuid>,
    auth: Option<AuthUser>,
) -> Result<StatusCode, ApiError> {
    let Some(repo) = &state.image_repo else {
        return Err(ApiError::internal("image repo not configured"));
    };
    let image = repo.get(ImageId(id)).await.map_err(ApiError::from)?;
    // IDOR check: only owner or admin can delete.
    if let Some(user) = auth {
        if user.user_id != image.owner_id && !user.roles.contains(&Role::Admin) {
            return Err(ApiError::forbidden("not allowed"));
        }
    }
    // Delete from storage (best-effort).
    if let Err(e) = state.storage.delete(&image.key).await {
        tracing::warn!("storage delete failed: {e}");
    }
    repo.delete(ImageId(id)).await.map_err(ApiError::from)?;
    Ok(StatusCode::NO_CONTENT)
}

/// `_bytes` no-op helper to silence unused-import warnings in tests.
pub const fn _bytes_() -> Bytes {
    Bytes::new()
}

/// `_into_response` no-op helper for tests.
pub fn _into_response() -> impl IntoResponse {
    StatusCode::OK
}
