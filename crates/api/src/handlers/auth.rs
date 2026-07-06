//! Auth handlers — login, logout, OIDC.

use crate::error::ApiError;
use crate::state::AppState;
use axum::extract::State;
use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::Json;
use serde::Deserialize;
use serde_json::json;
use std::sync::Arc;

/// `POST /api/v1/auth/login`
///
/// Accepts `{ "email": "...", "password": "..." }`.
/// Validates against the DB, issues a JWT.
pub async fn login(
    State(state): State<Arc<AppState>>,
    Json(body): Json<LoginBody>,
) -> Result<impl IntoResponse, ApiError> {
    // Look up the user from the configured image_repo or a future user repository.
    // For now, we stub login with a dev user and issue a JWT.
    //
    // Full implementation (post-MVP) queries the `users` table, verifies
    // the password hash via `PasswordHasher::verify`, and returns a JWT.
    //
    // See ADR-0007 P1-3.

    let token = state
        .jwt
        .issue(body.email)
        .map_err(|e| ApiError::internal(format!("jwt: {e}")))?;

    Ok(Json(json!({
        "access_token": token,
        "token_type": "Bearer",
        "expires_in": 3600,
    })))
}

/// Login request body.
#[derive(Debug, Deserialize)]
pub struct LoginBody {
    /// Email address.
    pub email: String,
    /// Password.
    pub password: String,
}

/// `POST /api/v1/auth/logout`
pub async fn logout() -> impl IntoResponse {
    StatusCode::NO_CONTENT
}

/// `POST /api/v1/auth/oidc/:provider/login`
pub async fn oidc_login() -> impl IntoResponse {
    StatusCode::NOT_IMPLEMENTED
}

/// `GET /api/v1/auth/oidc/:provider/callback`
pub async fn oidc_callback() -> impl IntoResponse {
    StatusCode::NOT_IMPLEMENTED
}

impl ApiError {
    /// Convenience constructor for 501 placeholder errors.
    pub fn not_implemented(_feature: &'static str) -> Self {
        Self::new(StatusCode::NOT_IMPLEMENTED, "not_implemented", "skeleton")
    }
}
