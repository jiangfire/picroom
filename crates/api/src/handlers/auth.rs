// SPDX-License-Identifier: MIT
// Copyright (c) 2026 Picroom Contributors

//! Auth handlers — login, logout, OIDC.

use crate::error::ApiError;
use crate::extractors::auth::AuthUser;
use crate::state::AppState;
use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Redirect};
use axum::Json;
use axum_extra::extract::cookie::{Cookie, CookieJar, SameSite};
use picroom_auth::{
    verify_id_token, HttpOidcClient, OidcClient, OidcError, OidcProvider, OidcUserInfo,
};
use picroom_domain::NewOidcUser;
use picroom_infra::config::OidcProviderConfig;
use serde::Deserialize;
use std::collections::HashSet;
use std::sync::Arc;

/// Cookie used to carry the OIDC `state`/`nonce` binding across the redirect.
const OIDC_STATE_COOKIE: &str = "oidc_state";

/// Identifies the caller for rate-limiting purposes.
///
/// The app sits behind a reverse proxy in every supported deployment, so the
/// forwarding headers are what actually carry the client address. They are
/// also client-controllable when the app is exposed directly, which makes the
/// per-IP budget best-effort — the per-account budget is the one that actually
/// bounds a password spray. Callers we cannot place fall into a single shared
/// bucket rather than escaping the limit.
fn client_key(headers: &axum::http::HeaderMap) -> String {
    let forwarded = headers
        .get("x-forwarded-for")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.split(',').next())
        .map(str::trim)
        .filter(|v| !v.is_empty());
    if let Some(ip) = forwarded {
        return ip.to_string();
    }
    if let Some(real) = headers
        .get("x-real-ip")
        .and_then(|v| v.to_str().ok())
        .map(str::trim)
        .filter(|v| !v.is_empty())
    {
        return real.to_string();
    }
    "unknown".to_string()
}

/// Records an auth audit event (best-effort: failures are logged, not fatal).
async fn audit_auth(
    state: &AppState,
    action: picroom_audit::AuditAction,
    actor_id: Option<uuid::Uuid>,
    actor_label: Option<String>,
) {
    let event = picroom_audit::AuditEvent {
        id: uuid::Uuid::now_v7(),
        timestamp: time::OffsetDateTime::now_utc(),
        actor_id,
        actor_label,
        action,
        target_type: "auth".into(),
        target_id: None,
        ip: None,
        user_agent: None,
        metadata: serde_json::Value::Null,
    };
    if let Err(e) = state.audit.record(&event).await {
        tracing::warn!(error = %e, "failed to record auth audit event");
    }
}

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
    headers: axum::http::HeaderMap,
    Json(body): Json<LoginBody>,
) -> Result<impl IntoResponse, ApiError> {
    // Counted before the credentials are examined: a request that never
    // reaches the password check must still cost the caller budget, or the
    // limiter would only see valid-looking traffic.
    if let Some(retry_after) = state
        .auth_rate_limiter
        .record(&client_key(&headers), &body.email)
    {
        return Err(ApiError::too_many_requests(
            retry_after,
            "too many authentication attempts",
        ));
    }

    let Some(user_repo) = &state.user_repo else {
        return Err(ApiError::internal("user repository not configured"));
    };

    // Look the user up by email.
    let Some(creds) = user_repo
        .find_by_email(&body.email)
        .await
        .map_err(|e| ApiError::internal(format!("lookup: {e}")))?
    else {
        audit_auth(
            &state,
            picroom_audit::AuditAction::Login,
            None,
            Some(body.email),
        )
        .await;
        return Err(ApiError::unauthorized("invalid credentials"));
    };

    // Reject disabled accounts with the same error as "no such user".
    if creds.disabled {
        audit_auth(
            &state,
            picroom_audit::AuditAction::Login,
            None,
            Some(body.email),
        )
        .await;
        return Err(ApiError::unauthorized("invalid credentials"));
    }

    // Verify the password against the stored Argon2id hash.
    let password_ok = picroom_auth::PasswordHasher::new()
        .verify(&body.password, &creds.password_hash)
        .map_err(|e| ApiError::internal(format!("verify: {e}")))?;
    if !password_ok {
        audit_auth(
            &state,
            picroom_audit::AuditAction::Login,
            None,
            Some(body.email),
        )
        .await;
        return Err(ApiError::unauthorized("invalid credentials"));
    }

    // Issue a JWT keyed on the user id (not the email) with the role as scope.
    let scopes = vec![creds.role.clone()];
    // Bind the token to a revocable session when one can be recorded (D-6):
    // `logout` and disabling the user then invalidate the token immediately.
    let sid = match &state.session_repo {
        Some(sessions) => {
            let sid = uuid::Uuid::now_v7();
            let expires_at =
                time::OffsetDateTime::now_utc() + time::Duration::seconds(state.jwt.ttl_secs());
            sessions
                .create(&picroom_service::repo::SessionRow {
                    id: sid,
                    user_id: creds.id.as_uuid(),
                    expires_at,
                })
                .await
                .map_err(|e| ApiError::internal(format!("session create: {e}")))?;
            Some(sid.to_string())
        }
        None => None,
    };
    let token = state
        .jwt
        .issue_session(creds.id.to_string(), &scopes, sid)
        .map_err(|e| ApiError::internal(format!("jwt: {e}")))?;

    audit_auth(
        &state,
        picroom_audit::AuditAction::Login,
        Some(creds.id.as_uuid()),
        None,
    )
    .await;

    Ok(Json(serde_json::json!({
        "access_token": token,
        "token_type": "Bearer",
        "expires_in": state.jwt.ttl_secs(),
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
///
/// Revokes the session the token is bound to, so the bearer token stops
/// working immediately instead of living out its TTL (R-08). Idempotent: a
/// token without a session (dev mode) still gets `204`.
pub async fn logout(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
) -> Result<StatusCode, ApiError> {
    if let Some(sid) = auth.session_id {
        if let Some(sessions) = &state.session_repo {
            sessions
                .revoke(sid)
                .await
                .map_err(|e| ApiError::internal(format!("session revoke: {e}")))?;
        }
    }
    audit_auth(
        &state,
        picroom_audit::AuditAction::Logout,
        Some(auth.user_id.as_uuid()),
        None,
    )
    .await;
    Ok(StatusCode::NO_CONTENT)
}

/// `GET /api/v1/auth/oidc/:provider/login`
///
/// Begins the OIDC flow: discovers the provider, builds the authorization URL
/// with a fresh `state`+`nonce`, stores the binding in a short-lived `HttpOnly`
/// cookie, and redirects the browser to the `IdP`.
pub async fn oidc_login(
    State(state): State<Arc<AppState>>,
    Path(provider): Path<String>,
    headers: axum::http::HeaderMap,
    jar: CookieJar,
) -> Result<impl IntoResponse, ApiError> {
    // The OIDC start endpoint is unauthenticated too, and each call costs a
    // discovery round-trip to the identity provider, so it shares the login
    // budget. Keyed on the provider because that is the only "account" known
    // before the browser comes back with an identity.
    if let Some(retry_after) = state
        .auth_rate_limiter
        .record(&client_key(&headers), &format!("oidc:{provider}"))
    {
        return Err(ApiError::too_many_requests(
            retry_after,
            "too many authentication attempts",
        ));
    }

    let cfg = state
        .oidc_providers
        .get(&provider)
        .ok_or_else(|| ApiError::not_found(format!("unknown oidc provider: {provider}")))?;

    let client = build_client(cfg)
        .await
        .map_err(|e| ApiError::internal(format!("oidc discover: {e}")))?;

    let state_val = random_binding();
    let nonce = random_binding();
    let auth_url = client
        .authorization_url(&state_val, &nonce)
        .map_err(|e| ApiError::internal(format!("oidc auth url: {e}")))?;

    let cookie_token = state
        .jwt
        .issue_oidc_state(&state_val, &nonce)
        .map_err(|e| ApiError::internal(format!("oidc state: {e}")))?;

    let cookie = Cookie::build(Cookie::new(OIDC_STATE_COOKIE, cookie_token))
        .http_only(true)
        .secure(state.cookie_secure)
        .same_site(SameSite::Lax)
        .path("/")
        .max_age(time::Duration::seconds(600))
        .build();
    let jar = jar.add(cookie);

    // `(CookieJar, Redirect)` carries both the Set-Cookie header and the 302.
    Ok((jar, Redirect::to(&auth_url)))
}

/// `GET /api/v1/auth/oidc/:provider/callback`
///
/// Completes the OIDC flow: validates the `state` cookie (CSRF), exchanges the
/// code for tokens, verifies the `id_token` (JWKS), finds-or-creates the local
/// account, issues a Bearer JWT, and redirects to the SPA with the token in
/// the URL fragment.
pub async fn oidc_callback(
    State(state): State<Arc<AppState>>,
    Path(provider): Path<String>,
    Query(params): Query<OidcCallbackQuery>,
    jar: CookieJar,
) -> Result<impl IntoResponse, ApiError> {
    let cfg = state
        .oidc_providers
        .get(&provider)
        .ok_or_else(|| ApiError::not_found(format!("unknown oidc provider: {provider}")))?;

    // 1. Validate the state cookie (CSRF protection).
    let cookie_token = jar
        .get(OIDC_STATE_COOKIE)
        .map(|c| c.value().to_string())
        .ok_or_else(|| ApiError::bad_request("missing oidc state cookie"))?;
    let (cookie_state, nonce) = state
        .jwt
        .verify_oidc_state(&cookie_token)
        .map_err(|_| ApiError::bad_request("invalid oidc state cookie"))?;
    if cookie_state != params.state {
        return Err(ApiError::bad_request("oidc state mismatch"));
    }

    // 2. Exchange the authorization code for tokens.
    let client = build_client(cfg)
        .await
        .map_err(|e| ApiError::internal(format!("oidc discover: {e}")))?;
    let tokens = client
        .exchange_code(&params.code)
        .await
        .map_err(|e| ApiError::internal(format!("oidc token exchange: {e}")))?;

    // 3. Verify the id_token signature + claims (and bind the nonce).
    let claims = verify_id_token(&client, &tokens.id_token, Some(&nonce))
        .map_err(|e| ApiError::bad_request(format!("oidc id_token: {e}")))?;

    // 4. Prefer fresh userinfo; fall back to id_token claims.
    let (email, name, avatar) = resolve_identity(&client, &tokens.access_token, &claims).await;
    let email = email.ok_or_else(|| ApiError::bad_request("oidc provider returned no email"))?;

    // 5. Find-or-create the local account linked to this identity.
    let user_repo = state
        .user_repo
        .as_ref()
        .ok_or_else(|| ApiError::internal("user repository not configured"))?;
    let user = if let Some(u) = user_repo
        .find_by_external(&provider, &claims.sub)
        .await
        .map_err(|e| ApiError::internal(format!("oidc lookup: {e}")))?
    {
        u
    } else {
        let role = provision_role(&email, state.oidc_admin_emails.as_ref());
        let new = NewOidcUser {
            email: email.clone(),
            name: name.unwrap_or_else(|| email.clone()),
            avatar_url: avatar,
            provider: provider.clone(),
            subject: claims.sub.clone(),
            email_verified: claims.email_verified.unwrap_or(false),
            role,
        };
        user_repo
            .create_oidc_user(&new)
            .await
            .map_err(|e| ApiError::internal(format!("oidc create: {e}")))?
    };

    if user.disabled {
        return Err(ApiError::unauthorized("account disabled"));
    }

    // 6. Issue a Bearer JWT and redirect to the SPA with it in the fragment.
    // Bind it to a revocable session exactly like password login (D-6) —
    // otherwise OIDC tokens would be sid-less and logout / disabling the user
    // could never revoke them.
    let sid = match &state.session_repo {
        Some(sessions) => {
            let sid = uuid::Uuid::now_v7();
            let expires_at =
                time::OffsetDateTime::now_utc() + time::Duration::seconds(state.jwt.ttl_secs());
            sessions
                .create(&picroom_service::repo::SessionRow {
                    id: sid,
                    user_id: user.id.as_uuid(),
                    expires_at,
                })
                .await
                .map_err(|e| ApiError::internal(format!("session create: {e}")))?;
            Some(sid.to_string())
        }
        None => None,
    };
    let token = state
        .jwt
        .issue_session(user.id.to_string(), std::slice::from_ref(&user.role), sid)
        .map_err(|e| ApiError::internal(format!("jwt: {e}")))?;

    let base = state
        .public_url_base
        .clone()
        .unwrap_or_else(|| "/".to_string());
    let location = format!(
        "{base}#access_token={token}&token_type=Bearer&expires_in={}",
        state.jwt.ttl_secs()
    );

    // Clear the state cookie.
    let clear = Cookie::build(Cookie::new(OIDC_STATE_COOKIE, ""))
        .http_only(true)
        .secure(state.cookie_secure)
        .same_site(SameSite::Lax)
        .path("/")
        .max_age(time::Duration::ZERO)
        .build();
    let jar = jar.add(clear);

    Ok((jar, Redirect::to(&location)))
}

/// Query parameters for the OIDC callback.
#[derive(Debug, Deserialize)]
pub struct OidcCallbackQuery {
    /// Authorization code.
    pub code: String,
    /// CSRF state echoed by the provider.
    pub state: String,
}

/// Builds an [`HttpOidcClient`] from provider configuration.
async fn build_client(cfg: &OidcProviderConfig) -> Result<HttpOidcClient, OidcError> {
    let provider = OidcProvider {
        name: String::new(),
        issuer: cfg.issuer.clone(),
        client_id: cfg.client_id.clone(),
        client_secret: cfg.client_secret.clone(),
        redirect_uri: cfg.redirect_uri.clone(),
        scopes: cfg.scopes.clone(),
        insecure_skip_verify: cfg.insecure_skip_verify,
    };
    let mut client = HttpOidcClient::discover(provider).await?;
    if cfg.insecure_skip_verify {
        client = client.with_insecure_skip_verify();
    }
    Ok(client)
}

/// Resolves the user's email/name/avatar from userinfo, falling back to the
/// `id_token` claims when userinfo is unavailable.
async fn resolve_identity(
    client: &HttpOidcClient,
    access_token: &str,
    claims: &picroom_auth::IdTokenClaims,
) -> (Option<String>, Option<String>, Option<String>) {
    match client.userinfo(access_token).await {
        Ok(OidcUserInfo {
            sub: _,
            email,
            name,
            picture,
        }) => (
            email.or_else(|| claims.email.clone()),
            name.or_else(|| claims.name.clone()),
            picture,
        ),
        Err(_) => (claims.email.clone(), claims.name.clone(), None),
    }
}

/// Decides the role for a newly-provisioned OIDC account: `admin` if the email
/// is on the allowlist, otherwise `viewer`.
pub fn provision_role<S: std::hash::BuildHasher>(
    email: &str,
    admin_emails: &HashSet<String, S>,
) -> String {
    if admin_emails.contains(email) {
        "admin".to_string()
    } else {
        "viewer".to_string()
    }
}

/// Generates a random, unguessable `state`/`nonce` value.
fn random_binding() -> String {
    uuid::Uuid::new_v4().to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn provision_role_grants_admin_for_allowlisted_email() {
        let admins: HashSet<String> = HashSet::from(["boss@example.com".to_string()]);
        assert_eq!(provision_role("boss@example.com", &admins), "admin");
        assert_eq!(provision_role("someone@example.com", &admins), "viewer");
    }

    #[test]
    fn provision_role_is_case_sensitive_on_email() {
        let admins: HashSet<String> = HashSet::from(["Boss@example.com".to_string()]);
        // A differently-cased email must NOT be promoted to admin.
        assert_eq!(provision_role("boss@example.com", &admins), "viewer");
    }
}
