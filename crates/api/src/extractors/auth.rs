// SPDX-License-Identifier: MIT
// Copyright (c) 2026 Picroom Contributors

//! Auth extractors and middleware.

use axum::extract::FromRequestParts;
use axum::http::request::Parts;
use axum::http::StatusCode;
use picroom_auth::Role;
use picroom_domain::UserId;
use uuid::Uuid;

/// Authenticated user extracted from a valid JWT.
#[derive(Debug, Clone)]
pub struct AuthUser {
    pub user_id: UserId,
    pub roles: Vec<Role>,
}

/// Extractor: reads `Authorization: Bearer <jwt>` and validates it.
///
/// Requires `S: JwtProvider`. Handlers that use this as a parameter will
/// automatically reject unauthenticated requests with 401.
#[axum::async_trait]
impl<S> FromRequestParts<S> for AuthUser
where
    S: Send + Sync + JwtProvider,
{
    type Rejection = (StatusCode, &'static str);

    async fn from_request_parts(parts: &mut Parts, state: &S) -> Result<Self, Self::Rejection> {
        let jwt_service = state.jwt_service();

        let token = parts
            .headers
            .get("authorization")
            .and_then(|v| v.to_str().ok())
            .and_then(|v| v.strip_prefix("Bearer "))
            .ok_or((StatusCode::UNAUTHORIZED, "missing token"))?;

        let claims = jwt_service
            .verify(token)
            .map_err(|_| (StatusCode::UNAUTHORIZED, "invalid token"))?;

        let roles: Vec<Role> = claims
            .scopes
            .iter()
            .filter_map(|s| match s.as_str() {
                "admin" => Some(Role::Admin),
                "manager" => Some(Role::Manager),
                "uploader" => Some(Role::Uploader),
                "viewer" => Some(Role::Viewer),
                _ => None,
            })
            .collect();

        let user_id = UserId(
            Uuid::parse_str(&claims.sub).map_err(|_| (StatusCode::UNAUTHORIZED, "invalid sub"))?,
        );

        Ok(Self { user_id, roles })
    }
}

/// Trait for providing JWT service from `AppState`.
pub trait JwtProvider {
    fn jwt_service(&self) -> &picroom_auth::JwtService;
}

/// Auth middleware: requires a **valid** `Authorization: Bearer <jwt>` on
/// `/api/v1/*` except `/api/v1/auth/*`.
///
/// Unlike a presence-only check, this verifies the token signature and expiry
/// against the configured [`JwtService`], so `Bearer garbage` is rejected with
/// `401`. Handlers may additionally use the [`AuthUser`] extractor to obtain
/// the authenticated principal.
pub async fn require_auth<S>(
    axum::extract::State(state): axum::extract::State<S>,
    req: axum::http::Request<axum::body::Body>,
    next: axum::middleware::Next,
) -> Result<axum::response::Response, StatusCode>
where
    S: JwtProvider,
{
    let path = req.uri().path();
    // Login/logout and other auth-flow routes are public.
    if path.starts_with("/api/v1/auth/") {
        return Ok(next.run(req).await);
    }
    if path.starts_with("/api/v1/") {
        let token = req
            .headers()
            .get("authorization")
            .and_then(|v| v.to_str().ok())
            .and_then(|v| v.strip_prefix("Bearer "))
            .ok_or(StatusCode::UNAUTHORIZED)?;
        // Reject forged/expired tokens at the gate.
        state
            .jwt_service()
            .verify(token)
            .map_err(|_| StatusCode::UNAUTHORIZED)?;
    }
    Ok(next.run(req).await)
}
