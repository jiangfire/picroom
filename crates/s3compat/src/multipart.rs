// SPDX-License-Identifier: MIT
// Copyright (c) 2026 Picroom Contributors

//! Multipart upload handlers.
//!
//! Full multipart support is post-MVP. Rather than fake success (which caused
//! silent data loss — clients received `200` but no bytes were ever stored),
//! every multipart operation returns an explicit S3 XML error so well-behaved
//! clients (`aws-cli`, `rclone`, `PicGo`) can fall back to a single `PUT`.

use crate::error::xml_error;
use crate::S3State;
use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::response::Response;
use std::sync::Arc;

/// Guard: when the query string carries multipart-upload parameters, return
/// the documented `501 NotImplemented` **before the caller touches any object
/// state**. `PUT …?partNumber=1&uploadId=U` and `DELETE …?uploadId=U` must
/// never fall through to the whole-object handlers — a fall-through PUT writes
/// the fragment as the entire object and a fall-through DELETE destroys it.
///
/// `None` means "not a multipart request; proceed". Part of `multipart.rs`
/// (not `object.rs`) so the only routed POST handler and the object-handler
/// guards stay in one place (ADR-0004: clients fall back to a single `PUT`).
pub(crate) fn multipart_rejection(query: Option<&str>) -> Option<Response> {
    let is_multipart = query?.split('&').any(|pair| {
        let name = pair.split_once('=').map(|(k, _)| k).unwrap_or(pair);
        matches!(name, "uploadId" | "partNumber")
    });
    is_multipart.then(|| {
        xml_error(
            StatusCode::NOT_IMPLEMENTED,
            "NotImplemented",
            "Multipart upload is not supported by this Picroom build; use a single PUT.",
        )
    })
}

/// `POST /s3/:bucket/:key?uploads` — initiate multipart.
pub async fn create_multipart<S: S3State>(
    State(_state): State<Arc<S>>,
    Path((_bucket, _key)): Path<(String, String)>,
) -> Response {
    xml_error(
        StatusCode::NOT_IMPLEMENTED,
        "NotImplemented",
        "Multipart upload is not supported by this Picroom build; use a single PUT.",
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::body::to_bytes;

    async fn body_string(response: Response) -> String {
        let bytes = to_bytes(response.into_body(), usize::MAX).await.unwrap();
        String::from_utf8_lossy(&bytes).into_owned()
    }

    #[test]
    fn rejection_fires_on_upload_id() {
        assert!(multipart_rejection(Some("uploadId=abc")).is_some());
    }

    #[test]
    fn rejection_fires_on_part_number() {
        assert!(multipart_rejection(Some("partNumber=1&uploadId=U")).is_some());
    }

    #[test]
    fn rejection_fires_on_bare_parameter() {
        assert!(multipart_rejection(Some("uploadId")).is_some());
    }

    #[test]
    fn rejection_ignores_lookalike_parameters() {
        assert!(multipart_rejection(Some("xuploadId=1")).is_none());
        assert!(multipart_rejection(Some("partNumberX=1")).is_none());
        assert!(multipart_rejection(Some("uploads")).is_none());
    }

    #[test]
    fn no_query_means_no_rejection() {
        assert!(multipart_rejection(None).is_none());
        assert!(multipart_rejection(Some("")).is_none());
    }

    #[test]
    fn rejection_body_is_501_xml() {
        let resp = multipart_rejection(Some("uploadId=U")).unwrap();
        assert_eq!(resp.status(), StatusCode::NOT_IMPLEMENTED);
    }

    #[tokio::test]
    async fn create_multipart_is_501() {
        let resp = create_multipart(
            State(crate::test_util::TestState::new().into()),
            Path(("b".into(), "k".into())),
        )
        .await;
        assert_eq!(resp.status(), StatusCode::NOT_IMPLEMENTED);
        let body = body_string(resp).await;
        assert!(body.contains("NotImplemented"));
        assert!(body.contains("use a single PUT"));
    }
}
