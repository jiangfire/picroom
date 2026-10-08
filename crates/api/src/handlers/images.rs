// SPDX-License-Identifier: MIT
// Copyright (c) 2026 Picroom Contributors

//! Image handlers.

use crate::error::ApiError;
use crate::extractors::auth::AuthUser;
use crate::state::AppState;
use axum::extract::{Multipart, Path, State};
use axum::http::StatusCode;
use axum::response::IntoResponse;
use bytes::Bytes;
use picroom_auth::{PermissionAction, ResourceType};
use picroom_domain::{ImageId, TeamId};
use serde_json::json;
use std::sync::Arc;
use uuid::Uuid;

/// Walks a multipart error's source chain looking for a body-length-limit
/// failure. axum's `DefaultBodyLimit` and tower-http's `RequestBodyLimitLayer`
/// raise "length limit exceeded" (the `LengthLimitError` text); multer wraps
/// the transport error, so match the documented message rather than the type.
fn is_body_limit_error(err: &axum::extract::multipart::MultipartError) -> bool {
    let mut src: Option<&(dyn std::error::Error + 'static)> = Some(err);
    while let Some(e) = src {
        if e.to_string().contains("length limit exceeded") {
            return true;
        }
        src = e.source();
    }
    false
}

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

    // A body over the configured limit surfaces as a length-limit error inside
    // the multipart stream. Multer wraps the transport error, so walk the
    // source chain to recognise it and answer 413 instead of flattening every
    // parse failure into 400.
    while let Some(field) = multipart.next_field().await.map_err(|e| {
        if is_body_limit_error(&e) {
            ApiError::new(
                StatusCode::PAYLOAD_TOO_LARGE,
                "payload_too_large",
                "request body exceeds the configured limit",
            )
        } else {
            ApiError::bad_request(format!("multipart error: {e}"))
        }
    })? {
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

    // 1. Validate + persist bytes (no jobs yet). The service layer enforces
    // `Image/Create` — including team-scope validation for the `team_id`
    // field, which is attributed server-side (R-05, R-13, D-7).
    let actor = auth.actor();
    let image = match state
        .upload
        .stage(&actor, team_id.map(TeamId), &mime, bytes)
        .await
    {
        Ok(i) => i,
        Err(e) => {
            let s = format!("{e}");
            if s.contains("empty")
                || s.contains("exceeds")
                || s.contains("unsupported")
                || s.contains("corrupt")
            {
                return Err(ApiError::bad_request(s));
            }
            tracing::error!("upload failed: {e}");
            return Err(ApiError::from(e));
        }
    };

    // 2. Persist metadata if a repo is configured. On failure the stored
    // object is removed — an orphan blob nobody can address must not survive
    // a failed upload (R-19).
    if let Some(repo) = &state.image_repo {
        if let Err(e) = repo.insert(&image).await {
            tracing::error!("repo insert failed: {e}");
            if let Err(del) = state.storage.delete(&image.key).await {
                tracing::error!(key = %image.key.as_str(), error = %del, "orphan cleanup failed");
            }
            return Err(ApiError::internal(format!("insert failed: {e}")));
        }
    }

    // 3. Only after the row is committed, enqueue variant jobs — a worker that
    // claims a job must be able to load its `images` row (R-04).
    state.upload.enqueue_variants(&image).await;

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
    // Same engine as link/file: owner, team membership, ACL grant, or
    // Image/Read with no team scope in play (deny rows always win).
    authorize_image_read(&state, &auth, &image).await?;
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
    // Authorization happens inside the DeleteService (owner / ACL / RBAC, in
    // the spec §10.3 order) — the route keeps authentication only (D-7).
    // Without a DB-backed service the route-level gate below is the only
    // defense, so it stays as the dev-mode fallback.
    let authorized = auth.user_id == image.owner_id
        || state
            .permissions
            .check(&auth.roles, ResourceType::Image, PermissionAction::Delete)
            .is_ok();
    if let Some(svc) = &state.delete_service {
        svc.delete(&auth.actor(), image)
            .await
            .map_err(ApiError::from)?;
    } else {
        if !authorized {
            return Err(ApiError::forbidden("not allowed"));
        }
        // Defensive fallback for environments without a DB-backed service.
        if let Err(e) = state.storage.delete(&image.key).await {
            tracing::warn!("storage delete failed: {e}");
        }
    }
    Ok(StatusCode::NO_CONTENT)
}

/// `GET /api/v1/images/:id/link` — generate the public link ("公链") for an image.
///
/// Returns `{ "public_url": "…", "expires_at": null }`. The URL targets the
/// unauthenticated `/i/{key}` route. When `server.public_url_base` is
/// configured it is an absolute URL; otherwise it is a path-relative URL the
/// caller resolves against the server it is talking to.
///
/// Access uses the same IDOR gate as `GET /images/:id`: the owner, or any
/// principal with the `Image/Read` permission (viewer/uploader/manager/admin
/// via RBAC).
pub async fn link(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
) -> Result<axum::Json<serde_json::Value>, ApiError> {
    let Some(repo) = &state.image_repo else {
        return Err(ApiError::internal("image repo not configured"));
    };
    let image = repo.get(ImageId(id)).await.map_err(ApiError::from)?;
    // Full spec-§10.3 evaluation (deny rows, team scope, ACL grants) — the
    // old global-role check let any viewer read any image (agent review).
    authorize_image_read(&state, &auth, &image).await?;

    let public_url = public_url_for(state.public_url_base.as_deref(), &image.key);

    Ok(axum::Json(json!({
        "public_url": public_url,
        "expires_at": null,
    })))
}

/// `GET /api/v1/images/:id/file` — 302 redirect to the public object URL.
///
/// Same access gate as `link`. Convenient for browsers/clients that want to
/// follow a redirect straight to the bytes rather than reading a JSON link.
pub async fn file(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
) -> Result<axum::response::Response, ApiError> {
    let Some(repo) = &state.image_repo else {
        return Err(ApiError::internal("image repo not configured"));
    };
    let image = repo.get(ImageId(id)).await.map_err(ApiError::from)?;
    authorize_image_read(&state, &auth, &image).await?;

    let location = public_url_for(state.public_url_base.as_deref(), &image.key);
    Ok((
        axum::http::StatusCode::FOUND,
        [(axum::http::header::LOCATION, location.as_str())],
    )
        .into_response())
}

/// Authorizes a READ of `image` through `AuthzService`.
///
/// - Team-scoped images: `Image/Read` evaluated in the full spec-§10.3 order
///   (deny → owner → team role → ACL → global role → deny), so a viewer who
///   is a team member can read, and a deny row wins over everything.
/// - Personal images are owner-only (spec §10.2): a non-owner needs
///   `Image/Update` (manager/admin) or an ACL `update`/`admin` grant — the
///   old route gate (`owner || Image/Update`) preserved verbatim, now in the
///   engine so deny rows apply there too.
async fn authorize_image_read(
    state: &AppState,
    auth: &AuthUser,
    image: &picroom_domain::Image,
) -> Result<(), ApiError> {
    let action = if image.team_id.is_none() && image.owner_id != auth.user_id {
        PermissionAction::Update
    } else {
        PermissionAction::Read
    };
    state
        .authz
        .authorize(
            &auth.actor(),
            &picroom_auth::Resource::new(
                ResourceType::Image,
                image.id.as_uuid(),
                Some(image.owner_id.as_uuid()),
                image.team_id.map(|t| t.as_uuid()),
            ),
            action,
        )
        .await
        .map_err(ApiError::from)
}

/// Builds the public URL for a storage key.
///
/// Absolute when `base` is set (`{base}/i/{key}`), path-relative otherwise
/// (`/i/{key}`) so the caller can resolve it against the server it knows.
fn public_url_for(base: Option<&str>, key: &picroom_domain::StorageKey) -> String {
    match base {
        Some(b) => format!("{}/i/{}", b.trim_end_matches('/'), key.as_str()),
        None => format!("/i/{}", key.as_str()),
    }
}
