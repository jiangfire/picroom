//! Admin handlers (skeleton).

use crate::error::ApiError;
use axum::extract::State;
use axum::http::StatusCode;
use std::sync::Arc;

use crate::state::AppState;

/// `POST /api/v1/admin/users`
pub async fn create_user(State(_state): State<Arc<AppState>>) -> Result<StatusCode, ApiError> {
    Err(ApiError::not_implemented("create_user"))
}

/// `PATCH /api/v1/admin/users/:id/role`
pub async fn set_role(State(_state): State<Arc<AppState>>) -> Result<StatusCode, ApiError> {
    Err(ApiError::not_implemented("set_role"))
}

/// `GET /api/v1/audit`
pub async fn audit(State(_state): State<Arc<AppState>>) -> Result<StatusCode, ApiError> {
    Err(ApiError::not_implemented("audit"))
}
