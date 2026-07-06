//! S3-specific errors.

use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::Json;
use serde_json::json;

/// S3-compatible API errors.
#[derive(Debug, thiserror::Error)]
pub enum S3Error {
    /// 4xx client error.
    #[error("{0}")]
    BadRequest(String),

    /// Signature mismatch.
    #[error("signature mismatch (expected {expected}, got {got})")]
    SignatureMismatch {
        /// Expected signature.
        expected: String,
        /// Provided signature.
        got: String,
    },

    /// Object not found.
    #[error("not found: {0}")]
    NotFound(String),

    /// Internal error.
    #[error("internal: {0}")]
    Internal(String),
}

impl IntoResponse for S3Error {
    fn into_response(self) -> Response {
        let (status, code) = match &self {
            Self::BadRequest(_) => (StatusCode::BAD_REQUEST, "BadRequest"),
            Self::SignatureMismatch { .. } => (StatusCode::FORBIDDEN, "SignatureDoesNotMatch"),
            Self::NotFound(_) => (StatusCode::NOT_FOUND, "NoSuchKey"),
            Self::Internal(_) => (StatusCode::INTERNAL_SERVER_ERROR, "InternalError"),
        };
        let body = Json(json!({
            "Code": code,
            "Message": format!("{self}"),
        }));
        (status, body).into_response()
    }
}
