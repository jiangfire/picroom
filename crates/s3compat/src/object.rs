// SPDX-License-Identifier: MIT
// Copyright (c) 2026 Picroom Contributors

//! Object-level handlers — PUT/GET/HEAD/DELETE backed by `Storage`.

use crate::error::xml_error;
use crate::multipart::multipart_rejection;
use crate::S3State;
use axum::extract::{Path, RawQuery, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use bytes::Bytes;
use picroom_domain::StorageKey;
use sha2::{Digest, Sha256};
use std::path::Path as FsPath;
use std::str::FromStr;
use std::sync::Arc;

/// Validates the bucket name shape (R-15). Returns the failure response for a
/// malformed name.
fn bucket_invalid(bucket: &str) -> Option<Response> {
    crate::bucket::BucketName::from_str(bucket).err().map(|e| {
        xml_error(
            StatusCode::BAD_REQUEST,
            "InvalidBucketName",
            &format!("The specified bucket is not valid: {e}"),
        )
    })
}

/// The `NoSuchBucket` failure every object handler shares (R-15): a bucket
/// this deployment does not serve must not silently share one flat namespace.
fn no_such_bucket(bucket: &str) -> Response {
    xml_error(
        StatusCode::NOT_FOUND,
        "NoSuchBucket",
        &format!("The specified bucket does not exist: {bucket}"),
    )
}

/// Shared pre-flight: name shape + configured-bucket match.
fn bucket_guard<S: S3State>(state: &S, bucket: &str) -> Option<Response> {
    bucket_invalid(bucket).or_else(|| {
        state
            .expected_bucket()
            .filter(|expected| expected != bucket)
            .map(|_| no_such_bucket(bucket))
    })
}

/// Derives an S3-style quoted `ETag` (SHA-256 hex) from object bytes.
fn etag_of(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    let digest = hasher.finalize();
    format!("\"{digest:x}\"")
}

/// Maps a storage key's file extension to a MIME type (defaults to octet-stream).
fn content_type_of(key: &str) -> &'static str {
    match FsPath::new(key).extension().and_then(|e| e.to_str()) {
        Some("png") => "image/png",
        Some("jpg" | "jpeg") => "image/jpeg",
        Some("webp") => "image/webp",
        Some("avif") => "image/avif",
        Some("gif") => "image/gif",
        _ => "application/octet-stream",
    }
}

/// `GET /s3/:bucket/:key`
pub async fn get_object<S: S3State>(
    State(state): State<Arc<S>>,
    Path((bucket, key)): Path<(String, String)>,
) -> Response {
    if let Some(failure) = bucket_guard(state.as_ref(), &bucket) {
        return failure;
    }
    let storage_key = match StorageKey::parse(&key) {
        Ok(k) => k,
        Err(e) => return xml_error(StatusCode::BAD_REQUEST, "InvalidKey", &e.to_string()),
    };
    match state.storage().get(&storage_key).await {
        Ok(bytes) => (
            StatusCode::OK,
            [
                ("content-length", bytes.len().to_string()),
                ("content-type", content_type_of(&key).to_string()),
                ("etag", etag_of(&bytes)),
            ],
            bytes,
        )
            .into_response(),
        Err(picroom_storage::StorageError::NotFound(_)) => {
            xml_error(StatusCode::NOT_FOUND, "NoSuchKey", &key)
        }
        Err(e) => internal_error(e),
    }
}

/// `PUT /s3/:bucket/:key`
///
/// Rejects multipart-shaped requests (`partNumber`/`uploadId`) with `501`
/// *before* touching storage — routing them here must not overwrite an
/// existing object with a single fragment (R-01).
pub async fn put_object<S: S3State>(
    State(state): State<Arc<S>>,
    Path((bucket, key)): Path<(String, String)>,
    RawQuery(query): RawQuery,
    _headers: HeaderMap,
    body: Bytes,
) -> Response {
    if let Some(rejection) = multipart_rejection(query.as_deref()) {
        return rejection;
    }
    if let Some(failure) = bucket_guard(state.as_ref(), &bucket) {
        return failure;
    }
    let storage_key = match StorageKey::parse(&key) {
        Ok(k) => k,
        Err(e) => return xml_error(StatusCode::BAD_REQUEST, "InvalidKey", &e.to_string()),
    };
    let etag = etag_of(&body);
    match state.storage().put(&storage_key, body).await {
        Ok(()) => (StatusCode::OK, [("etag", etag)]).into_response(),
        Err(e) => internal_error(e),
    }
}

/// `HEAD /s3/:bucket/:key`
pub async fn head_object<S: S3State>(
    State(state): State<Arc<S>>,
    Path((bucket, key)): Path<(String, String)>,
) -> Response {
    if let Some(failure) = bucket_guard(state.as_ref(), &bucket) {
        return failure;
    }
    let storage_key = match StorageKey::parse(&key) {
        Ok(k) => k,
        Err(e) => return xml_error(StatusCode::BAD_REQUEST, "InvalidKey", &e.to_string()),
    };
    match state.storage().exists(&storage_key).await {
        Ok(true) => StatusCode::OK.into_response(),
        Ok(false) => xml_error(StatusCode::NOT_FOUND, "NoSuchKey", &key),
        Err(e) => internal_error(e),
    }
}

/// `DELETE /s3/:bucket/:key`
///
/// Rejects multipart abort requests (`uploadId`) with `501` *before* touching
/// storage — routing them here must not delete the real object (R-01).
pub async fn delete_object<S: S3State>(
    State(state): State<Arc<S>>,
    Path((bucket, key)): Path<(String, String)>,
    RawQuery(query): RawQuery,
) -> Response {
    if let Some(rejection) = multipart_rejection(query.as_deref()) {
        return rejection;
    }
    if let Some(failure) = bucket_guard(state.as_ref(), &bucket) {
        return failure;
    }
    let storage_key = match StorageKey::parse(&key) {
        Ok(k) => k,
        Err(e) => return xml_error(StatusCode::BAD_REQUEST, "InvalidKey", &e.to_string()),
    };
    match state.storage().delete(&storage_key).await {
        Ok(()) | Err(picroom_storage::StorageError::NotFound(_)) => {
            StatusCode::NO_CONTENT.into_response()
        }
        Err(e) => internal_error(e),
    }
}

/// Internal server error: log the real cause server-side, but return a generic
/// message to the client. Never leak storage paths, SQL errors, or stack detail
/// (the API path already does this; the S3 path must too).
fn internal_error<E: std::fmt::Display>(e: E) -> Response {
    tracing::error!("s3 object operation failed: {e}");
    xml_error(
        StatusCode::INTERNAL_SERVER_ERROR,
        "InternalError",
        "An internal error occurred",
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_util::TestState;
    use axum::body::to_bytes;
    use axum::extract::{Path, RawQuery, State};
    use axum::http::{HeaderMap, StatusCode};
    use bytes::Bytes;
    use picroom_domain::StorageKey;
    use std::sync::Arc;

    fn state() -> Arc<TestState> {
        Arc::new(TestState::new())
    }

    async fn body_string(response: Response) -> String {
        let bytes = to_bytes(response.into_body(), usize::MAX).await.unwrap();
        String::from_utf8_lossy(&bytes).into_owned()
    }

    #[test]
    fn etag_of_quotes_sha256_of_empty() {
        assert_eq!(
            etag_of(b""),
            "\"e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855\""
        );
        assert_ne!(etag_of(b"a"), etag_of(b"b"));
    }

    #[test]
    fn content_type_of_maps_extensions() {
        assert_eq!(content_type_of("a.png"), "image/png");
        assert_eq!(content_type_of("a.jpg"), "image/jpeg");
        assert_eq!(content_type_of("a.jpeg"), "image/jpeg");
        assert_eq!(content_type_of("a.webp"), "image/webp");
        assert_eq!(content_type_of("a.avif"), "image/avif");
        assert_eq!(content_type_of("a.gif"), "image/gif");
        assert_eq!(content_type_of("a.bin"), "application/octet-stream");
        assert_eq!(content_type_of("noext"), "application/octet-stream");
    }

    #[tokio::test]
    async fn put_then_get_roundtrip() {
        let st = state();
        let key = "img/1.png".to_string();
        let body = Bytes::from_static(b"hello");
        let resp = put_object(
            State(st.clone()),
            Path(("bucket".into(), key.clone())),
            RawQuery(None),
            HeaderMap::new(),
            body.clone(),
        )
        .await;
        assert_eq!(resp.status(), StatusCode::OK);

        let resp = get_object(State(st), Path(("bucket".into(), key))).await;
        assert_eq!(resp.status(), StatusCode::OK);
        assert_eq!(body_string(resp).await, "hello");
    }

    #[tokio::test]
    async fn get_missing_returns_not_found() {
        let st = state();
        let resp = get_object(State(st), Path(("bucket".into(), "nope.png".into()))).await;
        assert_eq!(resp.status(), StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn invalid_key_returns_bad_request() {
        let st = state();
        let resp = get_object(
            State(st),
            Path(("bucket".into(), "/leadingslash.png".into())),
        )
        .await;
        assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn get_internal_error_on_storage_failure() {
        let st = state();
        st.set_fail(true);
        let resp = get_object(State(st), Path(("bucket".into(), "x.png".into()))).await;
        assert_eq!(resp.status(), StatusCode::INTERNAL_SERVER_ERROR);
    }

    #[tokio::test]
    async fn put_internal_error_on_storage_failure() {
        let st = state();
        st.set_fail(true);
        let resp = put_object(
            State(st),
            Path(("bucket".into(), "x.png".into())),
            RawQuery(None),
            HeaderMap::new(),
            Bytes::from_static(b"x"),
        )
        .await;
        assert_eq!(resp.status(), StatusCode::INTERNAL_SERVER_ERROR);
    }

    #[tokio::test]
    async fn head_existing_returns_ok() {
        let st = state();
        let key = "img/2.jpg".to_string();
        st.storage()
            .put(
                &StorageKey::parse(&key).unwrap(),
                Bytes::from_static(b"data"),
            )
            .await
            .unwrap();
        let resp = head_object(State(st), Path(("bucket".into(), key))).await;
        assert_eq!(resp.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn head_missing_returns_not_found() {
        let st = state();
        let resp = head_object(State(st), Path(("bucket".into(), "missing.jpg".into()))).await;
        assert_eq!(resp.status(), StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn head_internal_error_on_storage_failure() {
        let st = state();
        st.set_fail(true);
        let resp = head_object(State(st), Path(("bucket".into(), "x.jpg".into()))).await;
        assert_eq!(resp.status(), StatusCode::INTERNAL_SERVER_ERROR);
    }

    #[tokio::test]
    async fn delete_existing_returns_no_content() {
        let st = state();
        let key = "img/3.webp".to_string();
        st.storage()
            .put(&StorageKey::parse(&key).unwrap(), Bytes::from_static(b"d"))
            .await
            .unwrap();
        let resp = delete_object(
            State(st.clone()),
            Path(("bucket".into(), key.clone())),
            RawQuery(None),
        )
        .await;
        assert_eq!(resp.status(), StatusCode::NO_CONTENT);
        assert!(!st
            .storage()
            .exists(&StorageKey::parse(&key).unwrap())
            .await
            .unwrap());
    }

    #[tokio::test]
    async fn delete_missing_returns_no_content() {
        let st = state();
        let resp = delete_object(
            State(st),
            Path(("bucket".into(), "ghost.webp".into())),
            RawQuery(None),
        )
        .await;
        assert_eq!(resp.status(), StatusCode::NO_CONTENT);
    }

    #[tokio::test]
    async fn delete_internal_error_on_storage_failure() {
        let st = state();
        st.set_fail(true);
        let resp = delete_object(
            State(st),
            Path(("bucket".into(), "x.webp".into())),
            RawQuery(None),
        )
        .await;
        assert_eq!(resp.status(), StatusCode::INTERNAL_SERVER_ERROR);
    }
    // --- R-01: multipart-shaped requests must not fall through to the
    // whole-object handlers (they used to overwrite/delete real data). ---

    #[tokio::test]
    async fn put_with_multipart_query_is_501_and_writes_nothing() {
        let st = state();
        let key = "img/r01-put.png".to_string();
        let resp = put_object(
            State(st.clone()),
            Path(("bucket".into(), key.clone())),
            RawQuery(Some("partNumber=1&uploadId=U".into())),
            HeaderMap::new(),
            Bytes::from_static(b"fragment"),
        )
        .await;
        assert_eq!(resp.status(), StatusCode::NOT_IMPLEMENTED);
        assert!(
            !st.storage()
                .exists(&StorageKey::parse(&key).unwrap())
                .await
                .unwrap(),
            "multipart PUT must not write any bytes"
        );
    }

    #[tokio::test]
    async fn delete_with_multipart_query_is_501_and_object_survives() {
        let st = state();
        let key = "img/r01-delete.png".to_string();
        st.storage()
            .put(
                &StorageKey::parse(&key).unwrap(),
                Bytes::from_static(b"precious"),
            )
            .await
            .unwrap();
        let resp = delete_object(
            State(st.clone()),
            Path(("bucket".into(), key.clone())),
            RawQuery(Some("uploadId=x".into())),
        )
        .await;
        assert_eq!(resp.status(), StatusCode::NOT_IMPLEMENTED);
        let resp = get_object(State(st), Path(("bucket".into(), key))).await;
        assert_eq!(
            resp.status(),
            StatusCode::OK,
            "object must still be readable"
        );
        assert_eq!(body_string(resp).await, "precious");
    }
}
