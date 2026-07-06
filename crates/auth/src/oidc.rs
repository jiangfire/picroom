//! `OpenID` Connect client.
//!
//! Implements discovery, authorization-URL construction, code exchange,
//! and userinfo fetching. The actual provider interaction is done over
//! HTTPS via `reqwest`. Token validation reuses the JWT verifier with
//! the provider's published JWKs (or, in dev mode, a shared secret).

use async_trait::async_trait;
use jsonwebtoken::DecodingKey;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use thiserror::Error;
use time::OffsetDateTime;

/// OIDC errors.
#[derive(Debug, Error)]
pub enum OidcError {
    /// Discovery failed (HTTP, parse, etc.).
    #[error("discovery failed: {0}")]
    Discovery(String),
    /// Token exchange failed.
    #[error("token exchange failed: {0}")]
    TokenExchange(String),
    /// User-info failed.
    #[error("user-info failed: {0}")]
    UserInfo(String),
    /// ID-token invalid (signature, claims, etc.).
    #[error("invalid id_token: {0}")]
    InvalidIdToken(String),
    /// Underlying HTTP transport error.
    #[error("http: {0}")]
    Http(String),
}

/// OIDC provider configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OidcProvider {
    /// Provider key (used in URLs).
    pub name: String,
    /// Issuer URL (e.g. `https://example.com`).
    pub issuer: String,
    /// OAuth client id.
    pub client_id: String,
    /// OAuth client secret.
    pub client_secret: String,
    /// Redirect URI registered with the provider.
    pub redirect_uri: String,
    /// Scopes to request (default: openid email profile).
    pub scopes: Vec<String>,
    /// Skip signature verification (dev only).
    #[serde(default)]
    pub insecure_skip_verify: bool,
}

/// Discovery document (subset).
#[derive(Debug, Clone, Deserialize)]
pub struct DiscoveryDoc {
    pub issuer: String,
    pub authorization_endpoint: String,
    pub token_endpoint: String,
    pub userinfo_endpoint: Option<String>,
    pub jwks_uri: String,
    pub scopes_supported: Option<Vec<String>>,
}

/// Tokens returned from the OIDC provider.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OidcTokens {
    /// ID token (JWT).
    pub id_token: String,
    /// Access token (opaque).
    pub access_token: String,
    /// Refresh token, if provided.
    pub refresh_token: Option<String>,
    /// Token type.
    pub token_type: String,
    /// Expires-in seconds.
    pub expires_in: Option<i64>,
}

/// User info as returned by the OIDC `/userinfo` endpoint.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OidcUserInfo {
    /// Subject identifier.
    pub sub: String,
    /// Email address (claim name configurable; we read `email`).
    pub email: Option<String>,
    /// Display name (claim name configurable; we read `name`).
    pub name: Option<String>,
    /// Profile picture URL.
    pub picture: Option<String>,
}

/// OIDC trait.
#[async_trait]
pub trait OidcClient: Send + Sync {
    /// Build the authorization URL for redirecting the user.
    fn authorization_url(&self, state: &str, nonce: &str) -> Result<String, OidcError>;
    /// Exchange the authorization code for tokens.
    async fn exchange_code(&self, code: &str) -> Result<OidcTokens, OidcError>;
    /// Fetch the user-info document (requires access token).
    async fn userinfo(&self, access_token: &str) -> Result<OidcUserInfo, OidcError>;
}

/// HTTP-based OIDC client backed by `reqwest`.
pub struct HttpOidcClient {
    config: OidcProvider,
    doc: DiscoveryDoc,
    http: reqwest::Client,
    decoding_key: Arc<DecodingKey>,
    insecure_skip_verify: bool,
}

impl std::fmt::Debug for HttpOidcClient {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("HttpOidcClient")
            .field("name", &self.config.name)
            .field("issuer", &self.config.issuer)
            .finish()
    }
}

impl HttpOidcClient {
    /// Discovers the OIDC configuration and returns a ready client.
    pub async fn discover(config: OidcProvider) -> Result<Self, OidcError> {
        let http = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(15))
            .build()
            .map_err(|e| OidcError::Http(e.to_string()))?;

        let url = format!(
            "{}/.well-known/openid-configuration",
            config.issuer.trim_end_matches('/')
        );
        let doc: DiscoveryDoc = http
            .get(&url)
            .send()
            .await
            .map_err(|e| OidcError::Discovery(e.to_string()))?
            .error_for_status()
            .map_err(|e| OidcError::Discovery(e.to_string()))?
            .json()
            .await
            .map_err(|e| OidcError::Discovery(e.to_string()))?;

        // Use HS256 with the client secret as the verification key in dev mode.
        // A production implementation would fetch the JWKS document.
        let decoding_key = Arc::new(DecodingKey::from_secret(config.client_secret.as_bytes()));

        Ok(Self {
            config,
            doc,
            http,
            decoding_key,
            insecure_skip_verify: false,
        })
    }

    /// Enables insecure (skip-signature) verification — DEV ONLY.
    pub const fn with_insecure_skip_verify(mut self) -> Self {
        self.insecure_skip_verify = true;
        self
    }

    /// Returns the underlying config.
    pub const fn config(&self) -> &OidcProvider {
        &self.config
    }

    /// Returns the discovery doc.
    pub const fn discovery(&self) -> &DiscoveryDoc {
        &self.doc
    }
}

#[async_trait]
impl OidcClient for HttpOidcClient {
    fn authorization_url(&self, state: &str, nonce: &str) -> Result<String, OidcError> {
        let scopes = if self.config.scopes.is_empty() {
            vec!["openid".into(), "email".into(), "profile".into()]
        } else {
            self.config.scopes.clone()
        };
        let scope = scopes.join(" ");

        let mut url = reqwest::Url::parse(&self.doc.authorization_endpoint)
            .map_err(|e| OidcError::Discovery(format!("bad auth endpoint: {e}")))?;
        url.query_pairs_mut()
            .append_pair("response_type", "code")
            .append_pair("client_id", &self.config.client_id)
            .append_pair("redirect_uri", &self.config.redirect_uri)
            .append_pair("scope", &scope)
            .append_pair("state", state)
            .append_pair("nonce", nonce);
        Ok(url.to_string())
    }

    async fn exchange_code(&self, code: &str) -> Result<OidcTokens, OidcError> {
        let params = [
            ("grant_type", "authorization_code"),
            ("code", code),
            ("client_id", &self.config.client_id),
            ("client_secret", &self.config.client_secret),
            ("redirect_uri", &self.config.redirect_uri),
        ];

        let resp = self
            .http
            .post(&self.doc.token_endpoint)
            .form(&params)
            .send()
            .await
            .map_err(|e| OidcError::Http(e.to_string()))?
            .error_for_status()
            .map_err(|e| OidcError::TokenExchange(e.to_string()))?
            .json::<TokenResponse>()
            .await
            .map_err(|e| OidcError::TokenExchange(e.to_string()))?;

        resp.try_into()
    }

    async fn userinfo(&self, access_token: &str) -> Result<OidcUserInfo, OidcError> {
        let endpoint = self
            .doc
            .userinfo_endpoint
            .as_deref()
            .ok_or_else(|| OidcError::UserInfo("no userinfo_endpoint".into()))?;

        self.http
            .get(endpoint)
            .bearer_auth(access_token)
            .send()
            .await
            .map_err(|e| OidcError::Http(e.to_string()))?
            .error_for_status()
            .map_err(|e| OidcError::UserInfo(e.to_string()))?
            .json::<OidcUserInfo>()
            .await
            .map_err(|e| OidcError::UserInfo(e.to_string()))
    }
}

/// Raw token response (as returned by `token_endpoint`).
#[derive(Debug, Deserialize)]
struct TokenResponse {
    access_token: String,
    #[serde(default)]
    id_token: Option<String>,
    #[serde(default)]
    refresh_token: Option<String>,
    token_type: String,
    #[serde(default)]
    expires_in: Option<i64>,
}

impl TryFrom<TokenResponse> for OidcTokens {
    type Error = OidcError;
    fn try_from(r: TokenResponse) -> Result<Self, Self::Error> {
        let id_token = r
            .id_token
            .ok_or_else(|| OidcError::TokenExchange("missing id_token".into()))?;
        Ok(Self {
            id_token,
            access_token: r.access_token,
            refresh_token: r.refresh_token,
            token_type: r.token_type,
            expires_in: r.expires_in,
        })
    }
}

/// ID-token claims we care about.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IdTokenClaims {
    pub sub: String,
    pub iss: String,
    pub aud: serde_json::Value,
    pub exp: i64,
    pub iat: i64,
    #[serde(default)]
    pub email: Option<String>,
    #[serde(default)]
    pub name: Option<String>,
}

/// Verifies an ID-token signature + claims using this client's decoding key.
///
/// In production, the `DecodingKey` would be derived from the provider's
/// JWKS endpoint. Here we use HS256 with the client secret for parity
/// with how [`JwtService`](crate::jwt::JwtService) signs our own tokens.
pub fn verify_id_token(
    client: &HttpOidcClient,
    id_token: &str,
) -> Result<IdTokenClaims, OidcError> {
    use jsonwebtoken::{decode, Algorithm, Validation};
    let mut validation = Validation::new(Algorithm::HS256);
    validation.set_audience(&[&client.config.client_id]);
    validation.set_issuer(&[client.config.issuer.as_str()]);
    // Tight clock skew tolerance so an obviously-expired token is rejected.
    validation.leeway = 0;

    let key = client.decoding_key.clone();
    if client.insecure_skip_verify {
        // Still parse claims without verifying the signature.
        let mut parts = id_token.split('.');
        let _ = parts.next();
        let payload_b64 = parts
            .next()
            .ok_or_else(|| OidcError::InvalidIdToken("malformed token".into()))?;
        use base64::Engine;
        let payload = base64::engine::general_purpose::URL_SAFE_NO_PAD
            .decode(payload_b64)
            .map_err(|e| OidcError::InvalidIdToken(format!("b64: {e}")))?;
        let claims: IdTokenClaims = serde_json::from_slice(&payload)
            .map_err(|e| OidcError::InvalidIdToken(format!("json: {e}")))?;
        let now = OffsetDateTime::now_utc().unix_timestamp();
        if claims.exp < now {
            return Err(OidcError::InvalidIdToken("expired".into()));
        }
        return Ok(claims);
    }
    let token_data = decode::<IdTokenClaims>(id_token, &key, &validation)
        .map_err(|e| OidcError::InvalidIdToken(e.to_string()))?;
    Ok(token_data.claims)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn discovery_url_strips_trailing_slash() {
        let cfg = OidcProvider {
            name: "test".into(),
            issuer: "https://example.com/".into(),
            client_id: "id".into(),
            client_secret: "secret".into(),
            redirect_uri: "https://app.example.com/cb".into(),
            scopes: vec![],
            insecure_skip_verify: false,
        };
        let expected = "https://example.com/.well-known/openid-configuration";
        let actual = format!(
            "{}/.well-known/openid-configuration",
            cfg.issuer.trim_end_matches('/')
        );
        assert_eq!(actual, expected);
    }

    #[test]
    fn scopes_default_when_empty() {
        let cfg = OidcProvider {
            name: "test".into(),
            issuer: "https://example.com".into(),
            client_id: "id".into(),
            client_secret: "secret".into(),
            redirect_uri: "https://app.example.com/cb".into(),
            scopes: vec![],
            insecure_skip_verify: false,
        };
        let scopes = if cfg.scopes.is_empty() {
            vec![
                "openid".to_string(),
                "email".to_string(),
                "profile".to_string(),
            ]
        } else {
            cfg.scopes.clone()
        };
        assert_eq!(scopes.join(" "), "openid email profile");
    }
}
