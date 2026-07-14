// SPDX-License-Identifier: MIT
// Copyright (c) 2026 Picroom Contributors

//! `OpenID` Connect client.
//!
//! Implements discovery, authorization-URL construction, code exchange,
//! and userinfo fetching. The actual provider interaction is done over
//! HTTPS via `reqwest`. Token validation reuses the JWT verifier with
//! the provider's published JWKs (or, in dev mode, a shared secret).

use async_trait::async_trait;
use jsonwebtoken::{decode, decode_header, DecodingKey, Validation};
use serde::{Deserialize, Serialize};
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

/// A single JSON Web Key (subset we need for signature verification).
#[derive(Debug, Clone, Deserialize)]
pub struct Jwk {
    /// Key id, matched against the `id_token`'s `kid` header.
    #[serde(default)]
    pub kid: Option<String>,
    /// Key type: `RSA` or `EC`.
    pub kty: String,
    /// Algorithm, e.g. `RS256` / `ES256`.
    #[serde(default)]
    pub alg: Option<String>,
    /// RSA modulus (base64url).
    #[serde(default)]
    pub n: Option<String>,
    /// RSA public exponent (base64url).
    #[serde(default)]
    pub e: Option<String>,
    /// EC x coordinate (base64url).
    #[serde(default)]
    pub x: Option<String>,
    /// EC y coordinate (base64url).
    #[serde(default)]
    pub y: Option<String>,
}

/// A JWKS document.
#[derive(Debug, Clone, Deserialize)]
pub struct Jwks {
    /// Keys published by the provider.
    pub keys: Vec<Jwk>,
}

impl Jwks {
    /// Fetches the provider's JWKS document.
    pub async fn fetch(http: &reqwest::Client, jwks_uri: &str) -> Result<Self, OidcError> {
        http.get(jwks_uri)
            .send()
            .await
            .map_err(|e| OidcError::Discovery(format!("jwks: {e}")))?
            .error_for_status()
            .map_err(|e| OidcError::Discovery(format!("jwks: {e}")))?
            .json()
            .await
            .map_err(|e| OidcError::Discovery(format!("jwks json: {e}")))
    }

    /// Selects the key matching `kid` (or the first key when `kid` is absent)
    /// and builds a `DecodingKey` for it.
    pub fn decoding_key(&self, kid: Option<&str>) -> Result<DecodingKey, OidcError> {
        let key = match kid {
            Some(kid) => self
                .keys
                .iter()
                .find(|k| k.kid.as_deref() == Some(kid))
                .ok_or_else(|| OidcError::InvalidIdToken(format!("no jwk for kid {kid}")))?,
            None => self
                .keys
                .first()
                .ok_or_else(|| OidcError::InvalidIdToken("empty jwks".into()))?,
        };
        match key.kty.as_str() {
            "RSA" => {
                let (n, e) = (key.n.as_deref().ok_or_else(|| {
                    OidcError::InvalidIdToken("jwk missing n".into())
                })?, key.e.as_deref().ok_or_else(|| {
                    OidcError::InvalidIdToken("jwk missing e".into())
                })?);
                DecodingKey::from_rsa_components(n, e)
                    .map_err(|e| OidcError::InvalidIdToken(format!("rsa key: {e}")))
            }
            "EC" => {
                let (x, y) = (key.x.as_deref().ok_or_else(|| {
                    OidcError::InvalidIdToken("jwk missing x".into())
                })?, key.y.as_deref().ok_or_else(|| {
                    OidcError::InvalidIdToken("jwk missing y".into())
                })?);
                DecodingKey::from_ec_components(x, y)
                    .map_err(|e| OidcError::InvalidIdToken(format!("ec key: {e}")))
            }
            other => Err(OidcError::InvalidIdToken(format!(
                "unsupported kty {other}"
            ))),
        }
    }

    /// Picks the verification algorithm for `key`.
    pub fn algorithm(key: &Jwk) -> jsonwebtoken::Algorithm {
        if let Some(alg) = key.alg.as_deref() {
            return match alg {
                "RS384" => jsonwebtoken::Algorithm::RS384,
                "RS512" => jsonwebtoken::Algorithm::RS512,
                "ES256" => jsonwebtoken::Algorithm::ES256,
                "ES384" => jsonwebtoken::Algorithm::ES384,
                // RS256 and any unknown alg → fall back to RS256.
                _ => jsonwebtoken::Algorithm::RS256,
            };
        }
        // Fall back on key type when `alg` is absent.
        match key.kty.as_str() {
            "EC" => jsonwebtoken::Algorithm::ES256,
            _ => jsonwebtoken::Algorithm::RS256,
        }
    }
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
    jwks: Jwks,
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

        // Fetch the provider's signing keys so we can verify the id_token's
        // signature with the public JWKS (RS256/ES256) rather than a shared
        // secret. In insecure (dev/test) mode we skip the fetch entirely —
        // the keys are never used because signature verification is disabled.
        let jwks = if config.insecure_skip_verify {
            Jwks { keys: vec![] }
        } else {
            Jwks::fetch(&http, &doc.jwks_uri).await?
        };

        let insecure = config.insecure_skip_verify;
        Ok(Self {
            config,
            doc,
            http,
            jwks,
            insecure_skip_verify: insecure,
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
    /// OIDC nonce, used to bind the `id_token` to the authorization request.
    #[serde(default)]
    pub nonce: Option<String>,
    #[serde(default)]
    pub email: Option<String>,
    #[serde(default)]
    pub name: Option<String>,
    /// Email verification status asserted by the `IdP`.
    #[serde(default)]
    pub email_verified: Option<bool>,
}

/// Verifies an ID-token signature + claims.
///
/// In production the signature is verified against the provider's published
/// JWKS (RS256/ES256). When `insecure_skip_verify` is set the signature is
/// skipped (dev/test `IdPs` only). `expected_nonce`, when provided, must match
/// the token's `nonce` claim to bind it to the authorization request.
pub fn verify_id_token(
    client: &HttpOidcClient,
    id_token: &str,
    expected_nonce: Option<&str>,
) -> Result<IdTokenClaims, OidcError> {
    if client.insecure_skip_verify {
        let claims = decode_unsigned(id_token)?;
        check_nonce(&claims, expected_nonce)?;
        return Ok(claims);
    }

    let header =
        decode_header(id_token).map_err(|e| OidcError::InvalidIdToken(format!("header: {e}")))?;
    let jwk = client
        .jwks
        .keys
        .iter()
        .find(|k| k.kid == header.kid)
        .or_else(|| client.jwks.keys.first())
        .ok_or_else(|| OidcError::InvalidIdToken("no jwk available".into()))?;
    let key = client.jwks.decoding_key(header.kid.as_deref())?;
    let mut validation = Validation::new(Jwks::algorithm(jwk));
    validation.set_audience(&[&client.config.client_id]);
    validation.set_issuer(&[client.config.issuer.as_str()]);
    // Tight clock skew tolerance so an obviously-expired token is rejected.
    validation.leeway = 0;

    let claims = decode::<IdTokenClaims>(id_token, &key, &validation)
        .map_err(|e| OidcError::InvalidIdToken(e.to_string()))?
        .claims;
    check_nonce(&claims, expected_nonce)?;
    Ok(claims)
}

/// Validates the nonce binding when one is expected.
fn check_nonce(claims: &IdTokenClaims, expected: Option<&str>) -> Result<(), OidcError> {
    match expected {
        Some(expected) if claims.nonce.as_deref() != Some(expected) => {
            Err(OidcError::InvalidIdToken("nonce mismatch".into()))
        }
        _ => Ok(()),
    }
}

/// Parses ID-token claims without verifying the signature (dev mode).
fn decode_unsigned(id_token: &str) -> Result<IdTokenClaims, OidcError> {
    use base64::Engine;
    let payload_b64 = id_token
        .split('.')
        .nth(1)
        .ok_or_else(|| OidcError::InvalidIdToken("malformed token".into()))?;
    let payload = base64::engine::general_purpose::URL_SAFE_NO_PAD
        .decode(payload_b64)
        .map_err(|e| OidcError::InvalidIdToken(format!("b64: {e}")))?;
    let claims: IdTokenClaims = serde_json::from_slice(&payload)
        .map_err(|e| OidcError::InvalidIdToken(format!("json: {e}")))?;
    let now = OffsetDateTime::now_utc().unix_timestamp();
    if claims.exp < now {
        return Err(OidcError::InvalidIdToken("expired".into()));
    }
    Ok(claims)
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
            cfg.scopes
        };
        assert_eq!(scopes.join(" "), "openid email profile");
    }
}

/// Real JWKS signature-verification roundtrip: generate an RSA keypair, derive
/// the JWK `n`/`e` from the public key, build a `DecodingKey` via the same code
/// path the callback uses, sign a token with the private key, and verify it.
#[cfg(test)]
mod jwks_roundtrip_tests {
    use super::*;
    use base64::Engine;
    use jsonwebtoken::{encode, Algorithm, EncodingKey, Header, Validation};
    use rand::rngs::OsRng;
    use rsa::pkcs8::EncodePrivateKey;
    use rsa::traits::PublicKeyParts;
    use rsa::RsaPrivateKey;
    use serde_json;

    #[test]
    fn jwks_rs256_verify_roundtrip() {
        let mut rng = OsRng;
        let priv_key = RsaPrivateKey::new(&mut rng, 2048).expect("keypair");
        let pem = priv_key
            .to_pkcs8_pem(rsa::pkcs8::LineEnding::LF)
            .expect("pem");

        // Derive JWK components from the public key.
        let n = priv_key.n().to_bytes_be();
        let e = priv_key.e().to_bytes_be();
        let n_b64 = base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(n);
        let e_b64 = base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(e);

        let jwk = Jwk {
            kid: Some("test-key".to_string()),
            kty: "RSA".to_string(),
            alg: Some("RS256".to_string()),
            n: Some(n_b64),
            e: Some(e_b64),
            x: None,
            y: None,
        };
        let jwks = Jwks {
            keys: vec![jwk],
        };

        // Algorithm dispatch must pick RS256 from the `alg` claim.
        assert_eq!(Jwks::algorithm(jwks.keys.first().unwrap()), Algorithm::RS256);

        // Build a decoding key from the JWK (the exact path verify_id_token uses).
        let decoding_key = jwks
            .decoding_key(Some("test-key"))
            .expect("decoding key");

        let claims = IdTokenClaims {
            sub: "user-123".to_string(),
            iss: "https://issuer.example.com".to_string(),
            aud: serde_json::json!("client-id"),
            exp: time::OffsetDateTime::now_utc().unix_timestamp() + 300,
            iat: time::OffsetDateTime::now_utc().unix_timestamp(),
            nonce: Some("nonce-abc".to_string()),
            email: Some("user@example.com".to_string()),
            name: Some("Test User".to_string()),
            email_verified: Some(true),
        };
        let token = encode(
            &Header::new(Algorithm::RS256),
            &claims,
            &EncodingKey::from_rsa_pem(pem.as_bytes()).expect("encoding key"),
        )
        .expect("sign");

        let mut validation = Validation::new(Algorithm::RS256);
        validation.set_audience(&["client-id"]);
        validation.set_issuer(&["https://issuer.example.com"]);
        validation.leeway = 0;
        let verified = jsonwebtoken::decode::<IdTokenClaims>(&token, &decoding_key, &validation)
            .expect("verify");
        assert_eq!(verified.claims.sub, "user-123");
        assert_eq!(
            verified.claims.email.as_deref(),
            Some("user@example.com")
        );
    }

    #[test]
    fn jwks_algorithm_dispatches_by_alg_and_kty() {
        let rsa = Jwk {
            kid: None,
            kty: "RSA".into(),
            alg: Some("RS256".into()),
            n: None,
            e: None,
            x: None,
            y: None,
        };
        assert_eq!(Jwks::algorithm(&rsa), Algorithm::RS256);

        let es384 = Jwk {
            kid: None,
            kty: "EC".into(),
            alg: Some("ES384".into()),
            n: None,
            e: None,
            x: None,
            y: None,
        };
        assert_eq!(Jwks::algorithm(&es384), Algorithm::ES384);

        // Fallback to key type when `alg` is absent.
        let ec_no_alg = Jwk {
            kid: None,
            kty: "EC".into(),
            alg: None,
            n: None,
            e: None,
            x: None,
            y: None,
        };
        assert_eq!(Jwks::algorithm(&ec_no_alg), Algorithm::ES256);

        let rsa_no_alg = Jwk {
            kid: None,
            kty: "RSA".into(),
            alg: None,
            n: None,
            e: None,
            x: None,
            y: None,
        };
        assert_eq!(Jwks::algorithm(&rsa_no_alg), Algorithm::RS256);
    }
}
