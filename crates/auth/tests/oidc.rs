// SPDX-License-Identifier: MIT
// Copyright (c) 2026 Picroom Contributors

//! Integration tests for the OIDC HTTP client.
//!
//! Uses `wiremock` to stub the discovery + token + userinfo endpoints.

use jsonwebtoken::{encode, Algorithm, EncodingKey, Header};
use picroom_auth::oidc::{
    verify_id_token, HttpOidcClient, IdTokenClaims, OidcClient, OidcProvider,
};
use serde_json::json;
use std::time::Duration;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

const SECRET: &str = "shh-secret-very-long-enough-for-hs256";
const CLIENT_ID: &str = "picroom-test";

fn provider(issuer: &str) -> OidcProvider {
    OidcProvider {
        name: "test".into(),
        issuer: issuer.into(),
        client_id: CLIENT_ID.into(),
        client_secret: SECRET.into(),
        redirect_uri: "https://app.example.com/cb".into(),
        scopes: vec!["openid".into(), "email".into()],
        insecure_skip_verify: false,
    }
}

/// Mounts a discovery doc pointing all endpoints at the wiremock server.
async fn mount_discovery(server: &MockServer) {
    Mock::given(method("GET"))
        .and(path("/.well-known/openid-configuration"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "issuer": "https://op.example.com",
            "authorization_endpoint": format!("{}/authorize", server.uri()),
            "token_endpoint": format!("{}/token", server.uri()),
            "userinfo_endpoint": format!("{}/userinfo", server.uri()),
            "jwks_uri": format!("{}/jwks", server.uri()),
        })))
        .expect(1)
        .mount(server)
        .await;
}

fn make_id_token(issuer: &str, exp_offset_secs: i64) -> String {
    let header = Header::new(Algorithm::HS256);
    let now = time::OffsetDateTime::now_utc();
    let exp_dt = if exp_offset_secs >= 0 {
        now + Duration::from_secs(exp_offset_secs as u64)
    } else {
        now - Duration::from_secs((-exp_offset_secs) as u64)
    };
    let claims = IdTokenClaims {
        sub: "user-123".into(),
        iss: issuer.into(),
        aud: serde_json::Value::String(CLIENT_ID.into()),
        exp: exp_dt.unix_timestamp(),
        iat: now.unix_timestamp(),
        email: Some("alice@example.com".into()),
        name: Some("Alice".into()),
    };
    encode(
        &header,
        &claims,
        &EncodingKey::from_secret(SECRET.as_bytes()),
    )
    .unwrap()
}

#[tokio::test]
async fn discovers_and_returns_endpoints() {
    let server = MockServer::start().await;
    mount_discovery(&server).await;

    let client = HttpOidcClient::discover(provider(&server.uri()))
        .await
        .unwrap();
    assert_eq!(
        client.discovery().token_endpoint,
        format!("{}/token", server.uri())
    );
    assert_eq!(
        client.discovery().userinfo_endpoint.as_deref(),
        Some(format!("{}/userinfo", server.uri()).as_str())
    );
}

#[tokio::test]
async fn build_authorization_url_contains_required_params() {
    let server = MockServer::start().await;
    mount_discovery(&server).await;

    let client = HttpOidcClient::discover(provider(&server.uri()))
        .await
        .unwrap();
    let url = client.authorization_url("state-abc", "nonce-xyz").unwrap();
    assert!(url.contains("response_type=code"));
    assert!(url.contains(&format!("client_id={CLIENT_ID}")));
    assert!(url.contains("redirect_uri="));
    assert!(url.contains("scope=openid+email"));
    assert!(url.contains("state=state-abc"));
    assert!(url.contains("nonce=nonce-xyz"));
}

#[tokio::test]
async fn exchanges_code_for_tokens() {
    let server = MockServer::start().await;
    mount_discovery(&server).await;

    let id_token = make_id_token(&server.uri(), 60);

    Mock::given(method("POST"))
        .and(path("/token"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "access_token": "ACCESS-XYZ",
            "id_token": id_token,
            "refresh_token": "REFRESH-XYZ",
            "token_type": "Bearer",
            "expires_in": 3600,
        })))
        .expect(1)
        .mount(&server)
        .await;

    let client = HttpOidcClient::discover(provider(&server.uri()))
        .await
        .unwrap();
    let tokens = client.exchange_code("the-code").await.unwrap();
    assert_eq!(tokens.access_token, "ACCESS-XYZ");
    assert_eq!(tokens.refresh_token.as_deref(), Some("REFRESH-XYZ"));
    assert_eq!(tokens.token_type, "Bearer");

    // verify_id_token uses cfg.issuer as expected issuer; the tokens were
    // signed with that same issuer.
    let verified = verify_id_token(&client, &tokens.id_token).unwrap();
    assert_eq!(verified.sub, "user-123");
    assert_eq!(verified.email.as_deref(), Some("alice@example.com"));
    assert_eq!(verified.name.as_deref(), Some("Alice"));
}

#[tokio::test]
async fn rejects_expired_id_token() {
    let server = MockServer::start().await;
    mount_discovery(&server).await;

    let issuer = server.uri();
    let token = make_id_token(&issuer, -60);

    let client = HttpOidcClient::discover(provider(&server.uri()))
        .await
        .unwrap();
    let err = verify_id_token(&client, &token).unwrap_err();
    let msg = format!("{err}");
    assert!(
        msg.contains("expired") || msg.contains("ExpiredSignature"),
        "got {msg}"
    );
}

#[tokio::test]
async fn fetches_userinfo() {
    let server = MockServer::start().await;
    mount_discovery(&server).await;

    Mock::given(method("GET"))
        .and(path("/userinfo"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "sub": "u-1",
            "email": "bob@example.com",
            "name": "Bob",
        })))
        .expect(1)
        .mount(&server)
        .await;

    let client = HttpOidcClient::discover(provider(&server.uri()))
        .await
        .unwrap();
    let info = client.userinfo("ACCESS-XYZ").await.unwrap();
    assert_eq!(info.sub, "u-1");
    assert_eq!(info.email.as_deref(), Some("bob@example.com"));
    assert_eq!(info.name.as_deref(), Some("Bob"));
}

#[tokio::test]
async fn discovery_404_yields_error() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/.well-known/openid-configuration"))
        .respond_with(ResponseTemplate::new(404))
        .mount(&server)
        .await;

    let err = HttpOidcClient::discover(provider(&server.uri()))
        .await
        .unwrap_err();
    let msg = format!("{err}");
    assert!(msg.contains("discovery"), "got {msg}");
}
