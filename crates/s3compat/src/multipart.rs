// SPDX-License-Identifier: MIT
// Copyright (c) 2026 Picroom Contributors

//! Multipart upload handlers.
//!
//! Full multipart support is post-MVP. Rather than fake success (which caused
//! silent data loss — clients received `200` but no bytes were ever stored),
//! every multipart operation returns an explicit S3 XML error so well-behaved
//! clients (`aws-cli`, `rclone`, `PicGo`) can fall back to a single `PUT`.

use crate::S3State;
use axum::extract::{Path, State};
use axum::http::{header, StatusCode};
use axum::response::{IntoResponse, Response};
use std::sync::Arc;

/// XML error body for "multipart not implemented".
fn not_implemented_xml(bucket: &str, key: &str) -> String {
    format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n\
         <Error><Code>NotImplemented</Code>\
         <Message>Multipart upload is not supported by this Picroom build; use a single PUT.</Message>\
         <Bucket>{bucket}</Bucket><Key>{key}</Key></Error>"
    )
}

/// `POST /s3/:bucket/:key?uploads` — initiate multipart.
pub async fn create_multipart<S: S3State>(
    State(_state): State<Arc<S>>,
    Path((bucket, key)): Path<(String, String)>,
) -> Response {
    (
        StatusCode::NOT_IMPLEMENTED,
        [(header::CONTENT_TYPE, "application/xml")],
        not_implemented_xml(&bucket, &key),
    )
        .into_response()
}

/// `PUT /s3/:bucket/:key?partNumber=N&uploadId=U` — upload part.
pub async fn upload_part<S: S3State>(
    State(_state): State<Arc<S>>,
    Path((bucket, key)): Path<(String, String)>,
) -> Response {
    (
        StatusCode::NOT_IMPLEMENTED,
        [(header::CONTENT_TYPE, "application/xml")],
        not_implemented_xml(&bucket, &key),
    )
        .into_response()
}

/// `POST /s3/:bucket/:key?uploadId=U` — complete multipart.
pub async fn complete_multipart<S: S3State>(
    State(_state): State<Arc<S>>,
    Path((bucket, key)): Path<(String, String)>,
) -> Response {
    (
        StatusCode::NOT_IMPLEMENTED,
        [(header::CONTENT_TYPE, "application/xml")],
        not_implemented_xml(&bucket, &key),
    )
        .into_response()
}

/// `DELETE /s3/:bucket/:key?uploadId=U` — abort multipart.
pub async fn abort_multipart<S: S3State>(
    State(_state): State<Arc<S>>,
    Path((bucket, key)): Path<(String, String)>,
) -> Response {
    // Abort is idempotent; report NotImplemented for consistency.
    (
        StatusCode::NOT_IMPLEMENTED,
        [(header::CONTENT_TYPE, "application/xml")],
        not_implemented_xml(&bucket, &key),
    )
        .into_response()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_util::TestState;
    use axum::body::to_bytes;
    use axum::extract::{Path, State};
    use axum::http::StatusCode;
    use std::sync::Arc;

    fn state() -> Arc<TestState> {
        Arc::new(TestState::new())
    }

    async fn body_string(response: Response) -> String {
        let bytes = to_bytes(response.into_body(), usize::MAX).await.unwrap();
        String::from_utf8_lossy(&bytes).into_owned()
    }

    #[test]
    fn not_implemented_xml_includes_bucket_and_key() {
        let xml = not_implemented_xml("buck", "key.jpg");
        assert!(xml.contains("<Bucket>buck</Bucket>"));
        assert!(xml.contains("<Key>key.jpg</Key>"));
        assert!(xml.contains("NotImplemented"));
        assert!(xml.contains("use a single PUT"));
    }

    #[tokio::test]
    async fn create_multipart_is_501() {
        let resp = create_multipart(State(state()), Path(("b".into(), "k".into()))).await;
        assert_eq!(resp.status(), StatusCode::NOT_IMPLEMENTED);
        assert!(body_string(resp).await.contains("<Bucket>b</Bucket>"));
    }

    #[tokio::test]
    async fn upload_part_is_501() {
        let resp = upload_part(State(state()), Path(("b".into(), "k".into()))).await;
        assert_eq!(resp.status(), StatusCode::NOT_IMPLEMENTED);
    }

    #[tokio::test]
    async fn complete_multipart_is_501() {
        let resp = complete_multipart(State(state()), Path(("b".into(), "k".into()))).await;
        assert_eq!(resp.status(), StatusCode::NOT_IMPLEMENTED);
    }

    #[tokio::test]
    async fn abort_multipart_is_501() {
        let resp = abort_multipart(State(state()), Path(("b".into(), "k".into()))).await;
        assert_eq!(resp.status(), StatusCode::NOT_IMPLEMENTED);
    }
}
