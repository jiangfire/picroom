// SPDX-License-Identifier: MIT
// Copyright (c) 2026 Picroom Contributors

//! S3-specific errors.

use axum::http::{header, StatusCode};
use axum::response::{IntoResponse, Response};

/// S3-compatible API errors.
#[derive(Debug, thiserror::Error)]
pub enum S3Error {
    /// 4xx client error.
    #[error("{0}")]
    BadRequest(String),

    /// Signature mismatch. Deliberately carries no payload so the response
    /// cannot leak the expected signature to an attacker.
    #[error("signature mismatch")]
    SignatureMismatch,

    /// Object not found.
    #[error("not found: {0}")]
    NotFound(String),

    /// Internal error.
    #[error("internal: {0}")]
    Internal(String),
}

/// Minimal XML-escape for text content.
fn xml_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

impl IntoResponse for S3Error {
    fn into_response(self) -> Response {
        let (status, code) = match &self {
            Self::BadRequest(_) => (StatusCode::BAD_REQUEST, "BadRequest"),
            Self::SignatureMismatch => (StatusCode::FORBIDDEN, "SignatureDoesNotMatch"),
            Self::NotFound(_) => (StatusCode::NOT_FOUND, "NoSuchKey"),
            Self::Internal(_) => (StatusCode::INTERNAL_SERVER_ERROR, "InternalError"),
        };
        // AWS clients (aws-cli, rclone, PicGo) parse XML errors, not JSON.
        let body = format!(
            "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<Error><Code>{code}</Code><Message>{msg}</Message></Error>",
            msg = xml_escape(&self.to_string())
        );
        (status, [(header::CONTENT_TYPE, "application/xml")], body).into_response()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::http::header;

    #[test]
    fn bad_request_maps_to_400_with_xml() {
        let resp = S3Error::BadRequest("nope".into()).into_response();
        assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
        assert_eq!(
            resp.headers().get(header::CONTENT_TYPE).unwrap(),
            "application/xml"
        );
    }

    #[test]
    fn signature_mismatch_maps_to_403() {
        let resp = S3Error::SignatureMismatch.into_response();
        assert_eq!(resp.status(), StatusCode::FORBIDDEN);
        assert_eq!(
            resp.headers().get(header::CONTENT_TYPE).unwrap(),
            "application/xml"
        );
    }

    #[test]
    fn not_found_maps_to_404() {
        let resp = S3Error::NotFound("x".into()).into_response();
        assert_eq!(resp.status(), StatusCode::NOT_FOUND);
    }

    #[test]
    fn internal_maps_to_500() {
        let resp = S3Error::Internal("boom".into()).into_response();
        assert_eq!(resp.status(), StatusCode::INTERNAL_SERVER_ERROR);
    }

    #[test]
    fn xml_escape_handles_special_chars() {
        assert_eq!(xml_escape("a<b>c&d"), "a&lt;b&gt;c&amp;d");
        assert_eq!(xml_escape("plain"), "plain");
        assert_eq!(xml_escape("&<>"), "&amp;&lt;&gt;");
    }
}
