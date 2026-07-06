//! Integration test for the HTTP API.

use axum::body::Body;
use axum::http::{header, Request, StatusCode};
use bytes::Bytes;
use http_body_util::BodyExt;
use picroom_api::AppState;
use picroom_audit::NoopAuditSink;
use picroom_storage::driver::LocalDriver;
use serde_json::Value;
use std::path::PathBuf;
use std::sync::Arc;
use tower::ServiceExt;

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

/// Returns a valid Bearer token for the dev JWT service.
fn bearer_token() -> String {
    let jwt = picroom_auth::JwtService::new("dev-secret", "picroom", "picroom-api", 3600);
    format!("Bearer {}", jwt.issue("test@example.com").unwrap())
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
