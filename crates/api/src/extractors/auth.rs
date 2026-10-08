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
    /// Session the token is bound to (`sid` claim), when the token carries
    /// one. `logout` and the disable-user cascade revoke these.
    pub session_id: Option<Uuid>,
}

impl AuthUser {
    /// Projects the authenticated principal onto the RBAC [`Actor`].
    pub fn actor(&self) -> picroom_auth::Actor {
        picroom_auth::Actor::with_roles(self.user_id.as_uuid(), self.roles.clone())
    }
}

/// Trait for providing JWT service from `AppState`.
pub trait JwtProvider {
    fn jwt_service(&self) -> &picroom_auth::JwtService;

    /// The session repository, when one is configured. Tokens carrying a
    /// `sid` are only accepted while their session row is live; `None`
    /// (dev mode, no DB) skips that check.
    fn session_repo(&self) -> Option<&std::sync::Arc<dyn picroom_service::SessionRepository>> {
        None
    }

    /// Whether sid-less tokens are refused when a session repository is
    /// configured (`[auth].require_sessions`). Default `false`: pre-session
    /// tokens stay valid for one TTL window (D-6).
    fn require_sessions(&self) -> bool {
        false
    }
}

/// Extractor: reads `Authorization: Bearer <jwt>` and validates it.
///
/// Requires `S: JwtProvider`. Handlers that use this as a parameter will
/// automatically reject unauthenticated requests with 401.
///
/// When the token carries a `sid` and a session repository is configured, the
/// session must still exist and be unrevoked — that is what makes `logout`
/// and disabling a user effective before the JWT expires (R-08, R-20, D-6).
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

        // Session binding: a sid-bearing token must resolve to a live session.
        // With `[auth].require_sessions`, tokens WITHOUT a sid are refused
        // outright (the D-6 compat window is closed).
        if state.require_sessions() && state.session_repo().is_some() && claims.sid.is_none() {
            return Err((StatusCode::UNAUTHORIZED, "session required"));
        }
        if let Some(sid_text) = &claims.sid {
            if let Some(sessions) = state.session_repo() {
                let sid = Uuid::parse_str(sid_text)
                    .map_err(|_| (StatusCode::UNAUTHORIZED, "invalid session"))?;
                match sessions.get_active(sid).await {
                    Ok(Some(session)) => {
                        if session.user_id != user_id.0 {
                            return Err((StatusCode::UNAUTHORIZED, "invalid session"));
                        }
                    }
                    Ok(None) => return Err((StatusCode::UNAUTHORIZED, "session revoked")),
                    Err(_) => return Err((StatusCode::UNAUTHORIZED, "session check failed")),
                }
            }
        }

        let session_id = claims.sid.and_then(|s| Uuid::parse_str(&s).ok());

        Ok(Self {
            user_id,
            roles,
            session_id,
        })
    }
}

/// Auth middleware: requires a **valid** `Authorization: Bearer <jwt>` on
/// `/api/v1/*` except `/api/v1/auth/*`.
///
/// Unlike a presence-only check, this verifies the token signature and expiry
/// against the configured [`JwtService`], so `Bearer garbage` is rejected with
/// `401`. Handlers may additionally use the [`AuthUser`] extractor to obtain
/// the authenticated principal (which also re-checks session revocation).
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
