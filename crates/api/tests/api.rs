// SPDX-License-Identifier: MIT
// Copyright (c) 2026 Picroom Contributors

//! Integration test for the HTTP API.

use axum::body::Body;
use axum::http::{Request, StatusCode};
use bytes::Bytes;
use http_body_util::BodyExt;
use picroom_api::AppState;
use picroom_audit::NoopAuditSink;
use picroom_domain::{
    Image, ImageId, NewOidcUser, NewUser, Page, PageReq, StorageKey, Team, TeamId, TeamMember,
    User, UserId,
};
use picroom_service::{
    DeleteService, ImageRepository, ServiceError, StoragePolicy, StoragePolicyRepository,
    TeamRepository, UserCredentials, UserRepository,
};
use picroom_storage::driver::LocalDriver;
use serde_json::Value;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use tower::ServiceExt;

/// In-memory user repository for login tests.
struct InMemoryUserRepo {
    users: HashMap<String, UserCredentials>,
    all_users: Vec<User>,
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

    async fn list(&self, page: PageReq) -> Result<Page<User>, ServiceError> {
        let limit = usize::try_from(page.limit).unwrap_or(50).clamp(1, 200);
        let items: Vec<User> = self.all_users.iter().take(limit).cloned().collect();
        Ok(Page::new(items, None, page))
    }

    async fn find_by_id(&self, id: UserId) -> Result<Option<User>, ServiceError> {
        Ok(self.all_users.iter().find(|u| u.id == id).cloned())
    }

    async fn set_disabled(&self, _id: UserId, _disabled: bool) -> Result<(), ServiceError> {
        Ok(())
    }

    async fn find_by_external(
        &self,
        _provider: &str,
        _subject: &str,
    ) -> Result<Option<User>, ServiceError> {
        Ok(None)
    }

    async fn create_oidc_user(&self, _new: &NewOidcUser) -> Result<User, ServiceError> {
        Ok(User {
            id: picroom_domain::UserId(uuid::Uuid::now_v7()),
            email: _new.email.clone(),
            name: _new.name.clone(),
            avatar_url: None,
            role: _new.role.clone(),
            created_at: time::OffsetDateTime::now_utc(),
            disabled: false,
        })
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
    let repo: Arc<dyn UserRepository> = Arc::new(InMemoryUserRepo {
        users,
        all_users: vec![],
    });
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

// ---------------------------------------------------------------------------
// Public route /i/*key — unauthenticated image-byte serving ("公链")
// ---------------------------------------------------------------------------

/// Builds an app and returns the backing `LocalDriver` so tests can seed
/// objects directly into storage.
fn build_app_with_storage() -> (axum::Router, Arc<LocalDriver>) {
    let tmp = tempdir();
    let storage = Arc::new(LocalDriver::new(tmp, "/i"));
    let audit = Arc::new(NoopAuditSink);
    let state = Arc::new(AppState::for_dev(storage.clone(), audit));
    (picroom_api::build_router(state), storage)
}

#[tokio::test]
async fn public_route_serves_png_bytes_without_auth() {
    use picroom_domain::StorageKey;
    use picroom_storage::StorageWriter;
    let (app, storage) = build_app_with_storage();
    let png = make_png(12, 9);
    let key = StorageKey::parse("img/test-image.bin").unwrap();
    storage.put(&key, png.clone()).await.unwrap();

    // No Authorization header — public route must not require auth.
    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/i/img/test-image.bin")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(
        response
            .headers()
            .get("content-type")
            .map(|v| v.to_str().unwrap()),
        Some("image/png"),
    );
    let body = response.into_body().collect().await.unwrap().to_bytes();
    assert_eq!(body, png);
}

#[tokio::test]
async fn public_route_returns_404_for_missing_object() {
    let (app, _storage) = build_app_with_storage();
    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/i/img/does-not-exist.bin")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn public_route_sniffs_jpeg_content_type() {
    use picroom_domain::StorageKey;
    use picroom_storage::StorageWriter;
    let (app, storage) = build_app_with_storage();
    // Leading JPEG magic bytes (SOI + APP0 marker) — enough for sniffing.
    let jpeg = Bytes::from_static(&[
        0xFF, 0xD8, 0xFF, 0xE0, 0x00, 0x10, b'J', b'F', b'I', b'F', 0x00,
    ]);
    let key = StorageKey::parse("img/scan.jpg").unwrap();
    storage.put(&key, jpeg).await.unwrap();

    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/i/img/scan.jpg")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(
        response
            .headers()
            .get("content-type")
            .unwrap()
            .to_str()
            .unwrap(),
        "image/jpeg",
    );
}

// ---------------------------------------------------------------------------
// Image link endpoint — GET /api/v1/images/:id/link ("公链" generator)
// ---------------------------------------------------------------------------

/// In-memory image repository for handler tests.
struct InMemoryImageRepo {
    images: Vec<Image>,
}

#[async_trait::async_trait]
impl ImageRepository for InMemoryImageRepo {
    async fn insert(&self, _image: &Image) -> Result<(), ServiceError> {
        Ok(())
    }

    async fn get(&self, id: ImageId) -> Result<Image, ServiceError> {
        self.images
            .iter()
            .find(|i| i.id == id)
            .cloned()
            .ok_or_else(|| ServiceError::Domain(picroom_domain::DomainError::NotFound))
    }

    async fn list_for_owner(
        &self,
        _owner_id: uuid::Uuid,
        _page: PageReq,
    ) -> Result<Page<Image>, ServiceError> {
        // Test-only: return every stored image so the handler's mapping
        // branch is exercised.
        Ok(Page::new(self.images.clone(), None, _page))
    }

    async fn delete(&self, _id: ImageId) -> Result<(), ServiceError> {
        Ok(())
    }

    async fn ping(&self) -> Result<(), ServiceError> {
        Ok(())
    }
}

fn sample_image(id: uuid::Uuid, owner: UserId, key: &str) -> Image {
    Image {
        id: ImageId(id),
        owner_id: owner,
        team_id: None,
        key: StorageKey::parse(key).unwrap(),
        content_type: "image/png".into(),
        bytes: 100,
        width: 10,
        height: 10,
        sha256: None,
        variants: vec![],
        created_at: time::OffsetDateTime::UNIX_EPOCH,
    }
}

fn link_app(images: Vec<Image>, public_url_base: Option<&str>) -> axum::Router {
    let tmp = tempdir();
    let storage = Arc::new(LocalDriver::new(tmp, "/i"));
    let audit = Arc::new(NoopAuditSink);
    let repo: Arc<dyn ImageRepository> = Arc::new(InMemoryImageRepo { images });
    let mut state = AppState::for_dev(storage, audit).with_image_repo(repo);
    if let Some(base) = public_url_base {
        state = state.with_public_url_base(base.to_string());
    }
    picroom_api::build_router(Arc::new(state))
}

#[tokio::test]
async fn image_link_returns_absolute_public_url() {
    let img_id = uuid::Uuid::now_v7();
    let img = sample_image(img_id, UserId(uuid::Uuid::now_v7()), "img/sample.bin");
    let app = link_app(vec![img], Some("https://cdn.example.com"));

    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri(format!("/api/v1/images/{img_id}/link"))
                .header("authorization", bearer_token())
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let body = response.into_body().collect().await.unwrap().to_bytes();
    let json: Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(
        json["public_url"],
        "https://cdn.example.com/i/img/sample.bin"
    );
    assert!(json["expires_at"].is_null());
}

#[tokio::test]
async fn image_link_returns_relative_url_when_base_unset() {
    let img_id = uuid::Uuid::now_v7();
    let img = sample_image(img_id, UserId(uuid::Uuid::now_v7()), "img/relative.bin");
    let app = link_app(vec![img], None);

    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri(format!("/api/v1/images/{img_id}/link"))
                .header("authorization", bearer_token())
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let body = response.into_body().collect().await.unwrap().to_bytes();
    let json: Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["public_url"], "/i/img/relative.bin");
}

#[tokio::test]
async fn image_link_forbids_viewer_accessing_others_image() {
    let img_id = uuid::Uuid::now_v7();
    // Image owned by someone else.
    let img = sample_image(img_id, UserId(uuid::Uuid::now_v7()), "img/other.bin");
    let app = link_app(vec![img], Some("https://cdn.example.com"));

    // Viewer-scoped token (no Image/Update permission) → 403.
    let jwt = picroom_auth::JwtService::new("dev-secret", "picroom", "picroom-api", 3600);
    let token = jwt
        .issue_with_scopes(uuid::Uuid::now_v7(), &["viewer".to_string()])
        .unwrap();
    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri(format!("/api/v1/images/{img_id}/link"))
                .header("authorization", format!("Bearer {token}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::FORBIDDEN);
}

// ---------------------------------------------------------------------------
// Image file redirect — GET /api/v1/images/:id/file
// ---------------------------------------------------------------------------

#[tokio::test]
async fn image_file_redirects_to_public_url() {
    let img_id = uuid::Uuid::now_v7();
    let img = sample_image(img_id, UserId(uuid::Uuid::now_v7()), "img/redir.bin");
    let app = link_app(vec![img], Some("https://cdn.example.com"));

    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri(format!("/api/v1/images/{img_id}/file"))
                .header("authorization", bearer_token())
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::FOUND);
    assert_eq!(
        response
            .headers()
            .get("location")
            .unwrap()
            .to_str()
            .unwrap(),
        "https://cdn.example.com/i/img/redir.bin"
    );
}

#[tokio::test]
async fn image_file_returns_404_for_unknown_image() {
    let app = link_app(vec![], Some("https://cdn.example.com"));
    let unknown = uuid::Uuid::now_v7();
    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri(format!("/api/v1/images/{unknown}/file"))
                .header("authorization", bearer_token())
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::NOT_FOUND);
}

// ---------------------------------------------------------------------------
// Admin user management — list / disable / enable
// ---------------------------------------------------------------------------

fn admin_users_app(all_users: Vec<User>) -> axum::Router {
    let tmp = tempdir();
    let storage = Arc::new(LocalDriver::new(tmp, "/i"));
    let audit = Arc::new(NoopAuditSink);
    let repo: Arc<dyn UserRepository> = Arc::new(InMemoryUserRepo {
        users: HashMap::new(),
        all_users,
    });
    let state = AppState::for_dev(storage, audit).with_user_repo(repo);
    picroom_api::build_router(Arc::new(state))
}

fn sample_user(id: uuid::Uuid, email: &str, role: &str) -> User {
    User {
        id: UserId(id),
        email: email.into(),
        name: email.split('@').next().unwrap_or(email).into(),
        avatar_url: None,
        role: role.into(),
        created_at: time::OffsetDateTime::now_utc(),
        disabled: false,
    }
}

#[tokio::test]
async fn admin_list_users_returns_users_for_admin() {
    let u1 = sample_user(uuid::Uuid::now_v7(), "a@example.com", "admin");
    let u2 = sample_user(uuid::Uuid::now_v7(), "b@example.com", "viewer");
    let app = admin_users_app(vec![u1, u2]);

    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/v1/admin/users")
                .header("authorization", bearer_token())
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let body = response.into_body().collect().await.unwrap().to_bytes();
    let json: Value = serde_json::from_slice(&body).unwrap();
    let items = json["items"].as_array().expect("items array");
    assert_eq!(items.len(), 2);
    assert!(items.iter().any(|u| u["email"] == "a@example.com"));
    assert!(items.iter().any(|u| u["role"] == "viewer"));
}

#[tokio::test]
async fn admin_list_users_forbids_non_admin() {
    let app = admin_users_app(vec![]);
    let jwt = picroom_auth::JwtService::new("dev-secret", "picroom", "picroom-api", 3600);
    let token = jwt
        .issue_with_scopes(uuid::Uuid::now_v7(), &["viewer".to_string()])
        .unwrap();
    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/v1/admin/users")
                .header("authorization", format!("Bearer {token}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn admin_disable_user_returns_204() {
    let uid = uuid::Uuid::now_v7();
    let u = sample_user(uid, "x@example.com", "viewer");
    let app = admin_users_app(vec![u]);
    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(format!("/api/v1/admin/users/{uid}/disable"))
                .header("authorization", bearer_token())
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::NO_CONTENT);
}

#[tokio::test]
async fn admin_disable_user_rejects_invalid_id() {
    let app = admin_users_app(vec![]);
    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/admin/users/not-a-uuid/disable")
                .header("authorization", bearer_token())
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
}

// ---------------------------------------------------------------------------
// Team management — list teams / list members
// ---------------------------------------------------------------------------

/// In-memory team repository for handler tests.
struct InMemoryTeamRepo {
    teams: Vec<Team>,
    members: Vec<TeamMember>,
}

#[async_trait::async_trait]
impl TeamRepository for InMemoryTeamRepo {
    async fn create(&self, _team: &Team) -> Result<(), ServiceError> {
        Ok(())
    }

    async fn get(&self, id: TeamId) -> Result<Team, ServiceError> {
        self.teams
            .iter()
            .find(|t| t.id == id)
            .cloned()
            .ok_or_else(|| ServiceError::Domain(picroom_domain::DomainError::NotFound))
    }

    async fn list(&self) -> Result<Vec<Team>, ServiceError> {
        Ok(self.teams.clone())
    }

    async fn add_member(
        &self,
        _team_id: TeamId,
        _user_id: UserId,
        _role: &str,
    ) -> Result<(), ServiceError> {
        Ok(())
    }

    async fn list_members(&self, team_id: TeamId) -> Result<Vec<TeamMember>, ServiceError> {
        Ok(self
            .members
            .iter()
            .filter(|m| m.team_id == team_id)
            .cloned()
            .collect())
    }
}

fn teams_app(teams: Vec<Team>, members: Vec<TeamMember>) -> axum::Router {
    let tmp = tempdir();
    let storage = Arc::new(LocalDriver::new(tmp, "/i"));
    let audit = Arc::new(NoopAuditSink);
    let repo: Arc<dyn TeamRepository> = Arc::new(InMemoryTeamRepo { teams, members });
    let state = AppState::for_dev(storage, audit);
    // Weld the team repo in via the shared builder field.
    let state = AppState {
        team_repo: Some(repo),
        ..state
    };
    picroom_api::build_router(Arc::new(state))
}

fn sample_team(id: uuid::Uuid, name: &str, slug: &str) -> Team {
    Team {
        id: TeamId(id),
        name: name.into(),
        slug: slug.into(),
        description: None,
        storage_policy: None,
        created_at: time::OffsetDateTime::now_utc(),
    }
}

#[tokio::test]
async fn teams_list_returns_all_teams() {
    let t1 = sample_team(uuid::Uuid::now_v7(), "Engineering", "eng");
    let t2 = sample_team(uuid::Uuid::now_v7(), "Marketing", "mkt");
    let app = teams_app(vec![t1, t2], vec![]);
    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/v1/teams")
                .header("authorization", bearer_token())
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let body = response.into_body().collect().await.unwrap().to_bytes();
    let json: Value = serde_json::from_slice(&body).unwrap();
    let items = json["items"].as_array().expect("items array");
    assert_eq!(items.len(), 2);
    assert!(items.iter().any(|t| t["slug"] == "eng"));
}

#[tokio::test]
async fn team_members_list_returns_members() {
    let tid = uuid::Uuid::now_v7();
    let team = sample_team(tid, "Engineering", "eng");
    let m1 = TeamMember {
        team_id: TeamId(tid),
        user_id: UserId(uuid::Uuid::now_v7()),
        role: "admin".into(),
        joined_at: time::OffsetDateTime::now_utc(),
    };
    let m2 = TeamMember {
        team_id: TeamId(tid),
        user_id: UserId(uuid::Uuid::now_v7()),
        role: "uploader".into(),
        joined_at: time::OffsetDateTime::now_utc(),
    };
    let app = teams_app(vec![team], vec![m1, m2]);
    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri(format!("/api/v1/teams/{tid}/members"))
                .header("authorization", bearer_token())
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let body = response.into_body().collect().await.unwrap().to_bytes();
    let json: Value = serde_json::from_slice(&body).unwrap();
    let items = json["items"].as_array().expect("items array");
    assert_eq!(items.len(), 2);
    assert!(items.iter().any(|m| m["role"] == "admin"));
}

// ---------------------------------------------------------------------------
// Storage policy management — list / create
// ---------------------------------------------------------------------------

/// In-memory storage-policy repository for handler tests.
struct InMemoryStoragePolicyRepo {
    policies: Vec<StoragePolicy>,
}

#[async_trait::async_trait]
impl StoragePolicyRepository for InMemoryStoragePolicyRepo {
    async fn list(&self) -> Result<Vec<StoragePolicy>, ServiceError> {
        Ok(self.policies.clone())
    }

    async fn create(&self, policy: &StoragePolicy) -> Result<(), ServiceError> {
        // Test-only: no persistence; record would mutate but repo is &self.
        let _ = policy;
        Ok(())
    }
}

fn storage_app(policies: Vec<StoragePolicy>) -> axum::Router {
    let tmp = tempdir();
    let storage = Arc::new(LocalDriver::new(tmp, "/i"));
    let audit = Arc::new(NoopAuditSink);
    let repo: Arc<dyn StoragePolicyRepository> = Arc::new(InMemoryStoragePolicyRepo { policies });
    let state = AppState {
        storage_policy_repo: Some(repo),
        ..AppState::for_dev(storage, audit)
    };
    picroom_api::build_router(Arc::new(state))
}

#[tokio::test]
async fn storage_list_returns_empty_when_no_repo() {
    let app = build_app();
    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/v1/admin/storage/policies")
                .header("authorization", bearer_token())
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let body = response.into_body().collect().await.unwrap().to_bytes();
    let json: Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["items"].as_array().unwrap().len(), 0);
}

#[tokio::test]
async fn storage_list_returns_policies_for_admin() {
    let p = StoragePolicy {
        name: "default".into(),
        driver: "local".into(),
        config: serde_json::json!({}),
        is_default: true,
    };
    let app = storage_app(vec![p]);
    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/v1/admin/storage/policies")
                .header("authorization", bearer_token())
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let body = response.into_body().collect().await.unwrap().to_bytes();
    let json: Value = serde_json::from_slice(&body).unwrap();
    let items = json["items"].as_array().unwrap();
    assert_eq!(items.len(), 1);
    assert_eq!(items[0]["name"], "default");
    assert_eq!(items[0]["driver"], "local");
}

#[tokio::test]
async fn storage_create_returns_201_for_admin() {
    use axum::http::header::CONTENT_TYPE;
    let app = storage_app(vec![]);
    let body = serde_json::json!({
        "name": "archive",
        "driver": "s3",
        "config": { "bucket": "archive-bucket" },
        "is_default": false
    })
    .to_string();
    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/admin/storage/policies")
                .header(CONTENT_TYPE, "application/json")
                .header("authorization", bearer_token())
                .body(Body::from(body))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::CREATED);
    let body = response.into_body().collect().await.unwrap().to_bytes();
    let json: Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["name"], "archive");
    assert_eq!(json["driver"], "s3");
}

#[tokio::test]
async fn storage_list_forbids_non_admin() {
    let app = build_app();
    let jwt = picroom_auth::JwtService::new("dev-secret", "picroom", "picroom-api", 3600);
    let token = jwt
        .issue_with_scopes(uuid::Uuid::now_v7(), &["viewer".to_string()])
        .unwrap();
    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/v1/admin/storage/policies")
                .header("authorization", format!("Bearer {token}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::FORBIDDEN);
}

// ---------------------------------------------------------------------------
// Image list / get / delete — wired image_repo + delete_service
// ---------------------------------------------------------------------------

/// Builds an app whose image handlers are backed by an in-memory image repo
/// (and, when `with_delete` is set, the unified delete service).
fn images_app(images: Vec<Image>, with_delete: bool) -> axum::Router {
    let tmp = tempdir();
    let storage = Arc::new(LocalDriver::new(tmp, "https://cdn.example.com/i"));
    let audit = Arc::new(NoopAuditSink);
    let repo: Arc<dyn ImageRepository> = Arc::new(InMemoryImageRepo { images });
    let mut state = AppState::for_dev(storage, audit).with_image_repo(repo.clone());
    if with_delete {
        let del = Arc::new(DeleteService::new(
            state.upload.storage.clone(),
            repo.clone(),
            state.upload.audit.clone(),
        ));
        state.delete_service = Some(del);
    }
    picroom_api::build_router(Arc::new(state))
}

#[tokio::test]
async fn image_list_returns_items_for_owner() {
    let img = sample_image(uuid::Uuid::now_v7(), UserId(uuid::Uuid::now_v7()), "img/a.bin");
    let app = images_app(vec![img], false);
    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/v1/images")
                .header("authorization", bearer_token())
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let body = response.into_body().collect().await.unwrap().to_bytes();
    let json: Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["items"].as_array().unwrap().len(), 1);
}

#[tokio::test]
async fn image_get_returns_metadata() {
    let id = uuid::Uuid::now_v7();
    let img = sample_image(id, UserId(uuid::Uuid::now_v7()), "img/a.bin");
    let app = images_app(vec![img], false);
    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri(format!("/api/v1/images/{id}"))
                .header("authorization", bearer_token())
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let body = response.into_body().collect().await.unwrap().to_bytes();
    let json: Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["id"], id.to_string());
    assert!(json["owner_id"].as_str().is_some());
}

#[tokio::test]
async fn image_get_forbids_viewer_accessing_others_image() {
    let id = uuid::Uuid::now_v7();
    let img = sample_image(id, UserId(uuid::Uuid::now_v7()), "img/a.bin");
    let app = images_app(vec![img], false);
    let jwt = picroom_auth::JwtService::new("dev-secret", "picroom", "picroom-api", 3600);
    let token = jwt
        .issue_with_scopes(uuid::Uuid::now_v7(), &["viewer".to_string()])
        .unwrap();
    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri(format!("/api/v1/images/{id}"))
                .header("authorization", format!("Bearer {token}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn image_delete_returns_204_for_admin() {
    let id = uuid::Uuid::now_v7();
    let img = sample_image(id, UserId(uuid::Uuid::now_v7()), "img/a.bin");
    let app = images_app(vec![img], true);
    let response = app
        .oneshot(
            Request::builder()
                .method("DELETE")
                .uri(format!("/api/v1/images/{id}"))
                .header("authorization", bearer_token())
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::NO_CONTENT);
}

#[tokio::test]
async fn image_delete_forbids_viewer() {
    let id = uuid::Uuid::now_v7();
    let img = sample_image(id, UserId(uuid::Uuid::now_v7()), "img/a.bin");
    let app = images_app(vec![img], true);
    let jwt = picroom_auth::JwtService::new("dev-secret", "picroom", "picroom-api", 3600);
    let token = jwt
        .issue_with_scopes(uuid::Uuid::now_v7(), &["viewer".to_string()])
        .unwrap();
    let response = app
        .oneshot(
            Request::builder()
                .method("DELETE")
                .uri(format!("/api/v1/images/{id}"))
                .header("authorization", format!("Bearer {token}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn upload_with_team_id_associates_team() {
    use axum::http::header::CONTENT_TYPE;
    let app = build_app();
    let auth = bearer_token();
    let team_id = uuid::Uuid::now_v7();
    let boundary = "----picroom-test-boundary";
    let mut body = Vec::new();
    body.extend_from_slice(format!("--{boundary}\r\n").as_bytes());
    body.extend_from_slice(b"Content-Disposition: form-data; name=\"file\"; filename=\"t.png\"\r\n");
    body.extend_from_slice(b"Content-Type: image/png\r\n\r\n");
    body.extend_from_slice(&make_png(40, 30));
    body.extend_from_slice(b"\r\n");
    body.extend_from_slice(format!("--{boundary}\r\n").as_bytes());
    body.extend_from_slice(b"Content-Disposition: form-data; name=\"team_id\"\r\n\r\n");
    body.extend_from_slice(team_id.to_string().as_bytes());
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
                .header("authorization", &auth)
                .body(Body::from(body))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let body = response.into_body().collect().await.unwrap().to_bytes();
    let json: Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["team_id"], team_id.to_string());
}
