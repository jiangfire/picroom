//! Team handlers (skeleton).

use crate::error::ApiError;
use axum::extract::State;
use axum::http::StatusCode;
use std::sync::Arc;

use crate::state::AppState;

/// `POST /api/v1/teams`
pub async fn create(
    State(_state): State<Arc<AppState>>,
) -> Result<StatusCode, ApiError> {
    Err(ApiError::not_implemented("create"))
}

/// `GET /api/v1/teams/:id`
pub async fn get(
    State(_state): State<Arc<AppState>>,
) -> Result<StatusCode, ApiError> {
    Err(ApiError::not_implemented("get"))
}

/// `POST /api/v1/teams/:id/members`
pub async fn add_member(
    State(_state): State<Arc<AppState>>,
) -> Result<StatusCode, ApiError> {
    Err(ApiError::not_implemented("add_member"))
}