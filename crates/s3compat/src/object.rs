//! Object-level handlers — PUT/GET/HEAD/DELETE backed by `Storage`.

use crate::S3State;
use axum::extract::{Path, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use bytes::Bytes;
use picroom_domain::StorageKey;
use std::sync::Arc;

/// `GET /s3/:bucket/:key`
pub async fn get_object<S: S3State>(
    State(state): State<Arc<S>>,
    Path((_bucket, key)): Path<(String, String)>,
) -> Response {
    let storage_key = match StorageKey::parse(&key) {
        Ok(k) => k,
        Err(e) => return s3_xml_error(StatusCode::BAD_REQUEST, "InvalidKey", &e.to_string()),
    };
    match state.storage().get(&storage_key).await {
        Ok(bytes) => (
            StatusCode::OK,
            [("content-length", &bytes.len().to_string())],
            bytes,
        )
            .into_response(),
        Err(picroom_storage::StorageError::NotFound(_)) => {
            s3_xml_error(StatusCode::NOT_FOUND, "NoSuchKey", &key)
        }
        Err(e) => s3_xml_error(
            StatusCode::INTERNAL_SERVER_ERROR,
            "InternalError",
            &e.to_string(),
        ),
    }
}

/// `PUT /s3/:bucket/:key`
pub async fn put_object<S: S3State>(
    State(state): State<Arc<S>>,
    Path((_bucket, key)): Path<(String, String)>,
    _headers: HeaderMap,
    body: Bytes,
) -> Response {
    let storage_key = match StorageKey::parse(&key) {
        Ok(k) => k,
        Err(e) => return s3_xml_error(StatusCode::BAD_REQUEST, "InvalidKey", &e.to_string()),
    };
    match state.storage().put(&storage_key, body).await {
        Ok(()) => (StatusCode::OK, [("etag", "\"picroom\"")]).into_response(),
        Err(e) => s3_xml_error(
            StatusCode::INTERNAL_SERVER_ERROR,
            "InternalError",
            &e.to_string(),
        ),
    }
}

/// `HEAD /s3/:bucket/:key`
pub async fn head_object<S: S3State>(
    State(state): State<Arc<S>>,
    Path((_bucket, key)): Path<(String, String)>,
) -> Response {
    let storage_key = match StorageKey::parse(&key) {
        Ok(k) => k,
        Err(e) => return s3_xml_error(StatusCode::BAD_REQUEST, "InvalidKey", &e.to_string()),
    };
    match state.storage().exists(&storage_key).await {
        Ok(true) => StatusCode::OK.into_response(),
        Ok(false) => s3_xml_error(StatusCode::NOT_FOUND, "NoSuchKey", &key),
        Err(e) => s3_xml_error(
            StatusCode::INTERNAL_SERVER_ERROR,
            "InternalError",
            &e.to_string(),
        ),
    }
}

/// `DELETE /s3/:bucket/:key`
pub async fn delete_object<S: S3State>(
    State(state): State<Arc<S>>,
    Path((_bucket, key)): Path<(String, String)>,
) -> Response {
    let storage_key = match StorageKey::parse(&key) {
        Ok(k) => k,
        Err(e) => return s3_xml_error(StatusCode::BAD_REQUEST, "InvalidKey", &e.to_string()),
    };
    match state.storage().delete(&storage_key).await {
        Ok(()) | Err(picroom_storage::StorageError::NotFound(_)) => {
            StatusCode::NO_CONTENT.into_response()
        }
        Err(e) => s3_xml_error(
            StatusCode::INTERNAL_SERVER_ERROR,
            "InternalError",
            &e.to_string(),
        ),
    }
}

/// Builds an S3-compatible XML error response.
fn s3_xml_error(status: StatusCode, code: &str, message: &str) -> Response {
    let xml = format!(
        r#"<?xml version="1.0" encoding="UTF-8"?><Error><Code>{code}</Code><Message>{message}</Message><RequestId>picroom</RequestId></Error>"#
    );
    (status, [("content-type", "application/xml")], xml).into_response()
}
