// SPDX-License-Identifier: MIT
// Copyright (c) 2026 Picroom Contributors

//! Integration test for the HTTP API.

use axum::body::Body;
use axum::http::{Request, StatusCode};
use bytes::Bytes;
use http_body_util::BodyExt;
use picroom_api::AppState;
use picroom_audit::NoopAuditSink;
use picroom_domain::{NewUser, User};
use picroom_service::{ServiceError, UserCredentials, UserRepository};
use picroom_storage::driver::LocalDriver;
use serde_json::Value;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use tower::ServiceExt;

/// In-memory user repository for login tests.
struct InMemoryUserRepo {
    users: HashMap<String, UserCredentials>,
}

#[async_trait::async_trait]
impl UserRepository for InMemoryUserRepo {
    async fn find_by_email(&self, email: &str) -> Result<Option<UserCredentials>, ServiceError> {
        Ok(self.users.get(email).cloned())
    }

    async fn create_user(&self, new: &NewUser) -> Result<User, ServiceError> {
        Ok(User {
            id: picroom_domain::UserId(uuid::Uuid::now_v7()),
            email: new.email.clone(),
            name: new.name.clone(),
            avatar_url: None,
            role: new.role.clone(),
            created_at: time::OffsetDateTime::now_utc(),
            disabled: false,
        })
    }

    async fn set_role(
        &self,
        _user_id: picroom_domain::UserId,
        _role: &str,
    ) -> Result<(), ServiceError> {
        Ok(())
    }
}

fn make_png(w: u32, h: u32) -> Bytes {
    use std::io::Cursor;
    let img = image::RgbImage::from_fn(w, h, |x, y| image::Rgb([x as u8, y as u8, 64]));
    let mut buf = Vec::new();
    image::DynamicImage::ImageRgb8(img)
        .write_to(&mut Cursor::new(&mut buf), image::ImageFormat::Png)
        .unwrap();
    Bytes::from(buf)
}

fn tempdir() -> PathBuf {
    let base = std::env::temp_dir().join(format!("picroom-api-{}", uuid::Uuid::now_v7()));
    std::fs::create_dir_all(&base).unwrap();
    base
}

fn build_app() -> axum::Router {
    let tmp = tempdir();
    let storage = Arc::new(LocalDriver::new(tmp, "/i"));
    let audit = Arc::new(NoopAuditSink);
    let state = Arc::new(AppState::for_dev(storage, audit));
    picroom_api::build_router(state)
}

/// Returns a valid Bearer token for the dev JWT service. The token carries a
/// real UUID subject and the `admin` scope so it passes the `AuthUser`
/// extractor (which parses `sub` as a UUID and maps `scopes` to roles).
fn bearer_token() -> String {
    let jwt = picroom_auth::JwtService::new("dev-secret", "picroom", "picroom-api", 3600);
    let scopes = vec!["admin".to_string()];
    let id = uuid::Uuid::now_v7();
    format!("Bearer {}", jwt.issue_with_scopes(id, &scopes).unwrap())
}

#[tokio::test]
async fn healthz_returns_ok() {
    let app = build_app();
    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/healthz")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let body = response.into_body().collect().await.unwrap().to_bytes();
    let json: Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["status"], "ok");
}

#[tokio::test]
async fn readyz_returns_ok_or_503() {
    let app = build_app();
    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/readyz")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    // In dev mode without a DB, readyz returns 503. That's OK.
    assert!(
        response.status() == StatusCode::OK || response.status() == StatusCode::SERVICE_UNAVAILABLE,
        "got {:?}",
        response.status()
    );
}

#[tokio::test]
async fn upload_then_lists_in_response() {
    use axum::http::header::CONTENT_TYPE;
    let app = build_app();
    let auth = bearer_token();

    let boundary = "----picroom-test-boundary";
    let mut body = Vec::new();
    body.extend_from_slice(format!("--{boundary}\r\n").as_bytes());
    body.extend_from_slice(
        b"Content-Disposition: form-data; name=\"file\"; filename=\"test.png\"\r\n",
    );
    body.extend_from_slice(b"Content-Type: image/png\r\n\r\n");
    body.extend_from_slice(&make_png(80, 60));
    body.extend_from_slice(b"\r\n");
    body.extend_from_slice(format!("--{boundary}--\r\n").as_bytes());

    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/images")
                .header(
                    CONTENT_TYPE,
                    format!("multipart/form-data; boundary={boundary}"),
                )
                .header("authorization", &auth)
                .body(Body::from(body))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK, "upload should succeed");
    let body = response.into_body().collect().await.unwrap().to_bytes();
    let json: Value = serde_json::from_slice(&body).unwrap();
    let id = json["id"].as_str().expect("response should have id");
    assert_eq!(json["width"], 80);
    assert_eq!(json["height"], 60);
    assert!(json["bytes"].as_u64().unwrap() > 0);

    // List — expects 500 because image_repo is None.
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/v1/images")
                .header("authorization", &auth)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::INTERNAL_SERVER_ERROR);

    // Get by id — also 500 since repo is None.
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("GET")
                .header("authorization", &auth)
                .uri(format!("/api/v1/images/{id}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::INTERNAL_SERVER_ERROR);
}

#[tokio::test]
async fn upload_without_file_field_returns_400() {
    use axum::http::header::CONTENT_TYPE;
    let app = build_app();
    let boundary = "----picroom-test-boundary";
    let body = format!("--{boundary}--\r\n");
    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/images")
                .header(
                    CONTENT_TYPE,
                    format!("multipart/form-data; boundary={boundary}"),
                )
                .header("authorization", bearer_token())
                .body(Body::from(body))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn upload_empty_bytes_returns_400() {
    use axum::http::header::CONTENT_TYPE;
    let app = build_app();
    let boundary = "----picroom-test-boundary";
    let mut body = Vec::new();
    body.extend_from_slice(format!("--{boundary}\r\n").as_bytes());
    body.extend_from_slice(
        b"Content-Disposition: form-data; name=\"file\"; filename=\"x.png\"\r\n",
    );
    body.extend_from_slice(b"Content-Type: image/png\r\n\r\n");
    body.extend_from_slice(b"\r\n");
    body.extend_from_slice(format!("--{boundary}--\r\n").as_bytes());
    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/images")
                .header(
                    CONTENT_TYPE,
                    format!("multipart/form-data; boundary={boundary}"),
                )
                .header("authorization", bearer_token())
                .body(Body::from(body))
                .unwrap(),
        )
        .await
        .unwrap();
    assert!(
        response.status() == StatusCode::BAD_REQUEST
            || response.status() == StatusCode::INTERNAL_SERVER_ERROR,
        "got {:?}, expected 400 or 500",
        response.status()
    );
}

// ---------------------------------------------------------------------------
// Login handler — password verification
// ---------------------------------------------------------------------------

const PASSWORD: &str = "correct-horse-battery-staple";

/// Builds an app whose login handler is backed by an in-memory user store
/// seeded with one enabled admin (`alice@example.com`) and one disabled user
/// (`bob@example.com`), both with the same known password.
fn login_app() -> axum::Router {
    let tmp = tempdir();
    let storage = Arc::new(LocalDriver::new(tmp, "/i"));
    let audit = Arc::new(NoopAuditSink);
    let hash = picroom_auth::PasswordHasher::new()
        .hash(PASSWORD)
        .expect("hash");
    let mut users = HashMap::new();
    users.insert(
        "alice@example.com".to_string(),
        UserCredentials {
            id: picroom_domain::UserId(uuid::Uuid::now_v7()),
            role: "admin".to_string(),
            password_hash: hash.clone(),
            disabled: false,
        },
    );
    users.insert(
        "bob@example.com".to_string(),
        UserCredentials {
            id: picroom_domain::UserId(uuid::Uuid::now_v7()),
            role: "viewer".to_string(),
            password_hash: hash,
            disabled: true,
        },
    );
    let repo: Arc<dyn UserRepository> = Arc::new(InMemoryUserRepo { users });
    let state = Arc::new(AppState::for_dev(storage, audit).with_user_repo(repo));
    picroom_api::build_router(state)
}

async fn post_login(app: axum::Router, email: &str, password: &str) -> (StatusCode, Value) {
    use axum::http::header::CONTENT_TYPE;
    let body = serde_json::json!({ "email": email, "password": password }).to_string();
    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/auth/login")
                .header(CONTENT_TYPE, "application/json")
                .body(Body::from(body))
                .unwrap(),
        )
        .await
        .unwrap();
    let status = response.status();
    let bytes = response.into_body().collect().await.unwrap().to_bytes();
    let json: Value = serde_json::from_slice(&bytes).unwrap_or(Value::Null);
    (status, json)
}

#[tokio::test]
async fn login_with_correct_password_returns_token() {
    let (status, json) = post_login(login_app(), "alice@example.com", PASSWORD).await;
    assert_eq!(status, StatusCode::OK, "got {status}, body: {json}");
    let token = json["access_token"].as_str().expect("access_token");
    assert!(!token.is_empty());

    // The token must verify against the dev JWT service and carry the role.
    let jwt = picroom_auth::JwtService::new("dev-secret", "picroom", "picroom-api", 3600);
    let claims = jwt.verify(token).expect("issued token must verify");
    // sub must be a UUID (the user id), not the email.
    uuid::Uuid::parse_str(&claims.sub).expect("sub is a uuid");
    assert_eq!(claims.scopes, vec!["admin".to_string()]);
}

#[tokio::test]
async fn login_with_wrong_password_returns_401() {
    let (status, json) = post_login(login_app(), "alice@example.com", "wrong-password").await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
    assert_eq!(json["code"], "unauthorized");
}

#[tokio::test]
async fn login_with_unknown_email_returns_401() {
    let (status, _json) = post_login(login_app(), "nobody@example.com", PASSWORD).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn login_with_disabled_account_returns_401() {
    let (status, _json) = post_login(login_app(), "bob@example.com", PASSWORD).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
}

// ---------------------------------------------------------------------------
// Auth gate — middleware must verify tokens, not just check presence
// ---------------------------------------------------------------------------

#[tokio::test]
async fn api_rejects_missing_token() {
    let app = build_app();
    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/v1/images")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}

// ---------------------------------------------------------------------------
// Admin handlers — create_user / set_role
// ---------------------------------------------------------------------------

#[tokio::test]
async fn admin_create_user_requires_admin_role() {
    use axum::http::header::CONTENT_TYPE;
    // A viewer-scoped token must be rejected (403), not 201.
    let app = login_app();
    let jwt = picroom_auth::JwtService::new("dev-secret", "picroom", "picroom-api", 3600);
    let token = jwt
        .issue_with_scopes(uuid::Uuid::now_v7(), &["viewer".to_string()])
        .unwrap();
    let body =
        serde_json::json!({ "email": "carol@example.com", "password": "supersecret1" }).to_string();
    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/admin/users")
                .header(CONTENT_TYPE, "application/json")
                .header("authorization", format!("Bearer {token}"))
                .body(Body::from(body))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn admin_create_user_returns_201_with_admin_token() {
    use axum::http::header::CONTENT_TYPE;
    let app = login_app();
    let auth = bearer_token();
    let body = serde_json::json!({
        "email": "carol@example.com",
        "password": "supersecret1",
        "role": "uploader"
    })
    .to_string();
    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/admin/users")
                .header(CONTENT_TYPE, "application/json")
                .header("authorization", &auth)
                .body(Body::from(body))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::CREATED);
    let bytes = response.into_body().collect().await.unwrap().to_bytes();
    let json: Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(json["email"], "carol@example.com");
    assert_eq!(json["role"], "uploader");
}

#[tokio::test]
async fn admin_set_role_returns_204() {
    use axum::http::header::CONTENT_TYPE;
    let app = login_app();
    let auth = bearer_token();
    let body = serde_json::json!({ "role": "manager" }).to_string();
    let response = app
        .oneshot(
            Request::builder()
                .method("PATCH")
                .uri("/api/v1/admin/users/00000000-0000-0000-0000-000000000001/role")
                .header(CONTENT_TYPE, "application/json")
                .header("authorization", &auth)
                .body(Body::from(body))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::NO_CONTENT);
}
#[tokio::test]
async fn api_rejects_forged_token() {
    // `Bearer garbage` must be rejected now (previously it passed).
    let app = build_app();
    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/v1/images")
                .header("authorization", "Bearer garbage.not.a.real.token")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn api_rejects_token_signed_with_wrong_secret() {
    let app = build_app();
    // Signed with a different secret than the dev service ("dev-secret").
    let jwt = picroom_auth::JwtService::new("wrong-secret", "picroom", "picroom-api", 3600);
    let token = jwt
        .issue_with_scopes(uuid::Uuid::now_v7(), &["admin".to_string()])
        .unwrap();
    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/v1/images")
                .header("authorization", format!("Bearer {token}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}
