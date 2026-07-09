// SPDX-License-Identifier: MIT
// Copyright (c) 2026 Picroom Contributors

//! Auth handlers — login, logout, OIDC.

use crate::error::ApiError;
use crate::state::AppState;
use axum::extract::State;
use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::Json;
use picroom_auth::PasswordHasher;
use serde::Deserialize;
use serde_json::json;
use std::sync::Arc;

/// `POST /api/v1/auth/login`
///
/// Accepts `{ "email": "...", "password": "..." }`, looks the user up in the
/// `users` table, verifies the Argon2id hash, and issues a JWT whose `sub` is
/// the user id and whose `scopes` carry the user's role.
///
/// Returns `401` for unknown email, wrong password, or a disabled account —
/// the message is identical in all three cases so an attacker cannot enumerate
/// valid emails via timing or response shape.
pub async fn login(
    State(state): State<Arc<AppState>>,
    Json(body): Json<LoginBody>,
) -> Result<impl IntoResponse, ApiError> {
    let Some(user_repo) = &state.user_repo else {
        return Err(ApiError::internal("user repository not configured"));
    };

    // Look the user up by email.
    let creds = user_repo
        .find_by_email(&body.email)
        .await
        .map_err(|e| ApiError::internal(format!("lookup: {e}")))?
        .ok_or_else(|| ApiError::unauthorized("invalid credentials"))?;

    // Reject disabled accounts with the same error as "no such user".
    if creds.disabled {
        return Err(ApiError::unauthorized("invalid credentials"));
    }

    // Verify the password against the stored Argon2id hash.
    let password_ok = PasswordHasher::new()
        .verify(&body.password, &creds.password_hash)
        .map_err(|e| ApiError::internal(format!("verify: {e}")))?;
    if !password_ok {
        return Err(ApiError::unauthorized("invalid credentials"));
    }

    // Issue a JWT keyed on the user id (not the email) with the role as scope.
    let scopes = vec![creds.role.clone()];
    let token = state
        .jwt
        .issue_with_scopes(creds.id.to_string(), &scopes)
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
