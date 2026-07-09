// SPDX-License-Identifier: MIT
// Copyright (c) 2026 Picroom Contributors

//! Error → HTTP mapping.

use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::Json;
use serde_json::json;

/// Generic API error.
#[derive(Debug)]
pub struct ApiError {
    /// HTTP status code.
    pub status: StatusCode,
    /// Machine-readable code.
    pub code: &'static str,
    /// Human-readable message.
    pub message: String,
    /// Optional request id for tracing.
    pub request_id: Option<String>,
}

impl ApiError {
    /// Constructs a new error.
    pub fn new(status: StatusCode, code: &'static str, message: impl Into<String>) -> Self {
        Self {
            status,
            code,
            message: message.into(),
            request_id: None,
        }
    }

    /// 400 Bad Request.
    pub fn bad_request(message: impl Into<String>) -> Self {
        Self::new(StatusCode::BAD_REQUEST, "bad_request", message)
    }

    /// 401 Unauthorized.
    pub fn unauthorized(message: impl Into<String>) -> Self {
        Self::new(StatusCode::UNAUTHORIZED, "unauthorized", message)
    }

    /// 403 Forbidden.
    pub fn forbidden(message: impl Into<String>) -> Self {
        Self::new(StatusCode::FORBIDDEN, "forbidden", message)
    }

    /// 404 Not Found.
    pub fn not_found(message: impl Into<String>) -> Self {
        Self::new(StatusCode::NOT_FOUND, "not_found", message)
    }

    /// 413 Payload Too Large.
    pub fn quota_exceeded(message: impl Into<String>) -> Self {
        Self::new(StatusCode::PAYLOAD_TOO_LARGE, "quota_exceeded", message)
    }

    /// 500 Internal Server Error.
    ///
    /// The detail is **logged server-side only**; clients always receive a
    /// generic message so that storage backends, SQL errors, and filesystem
    /// paths are never leaked to callers (see docs/review-2026-07.md §3.7).
    pub fn internal(message: impl Into<String>) -> Self {
        let detail = message.into();
        tracing::error!(error = %detail, "internal api error");
        Self::new(
            StatusCode::INTERNAL_SERVER_ERROR,
            "internal",
            "internal server error",
        )
    }

    /// Attaches a request id.
    #[must_use]
    pub fn with_request_id(mut self, id: impl Into<String>) -> Self {
        self.request_id = Some(id.into());
        self
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let body = Json(json!({
            "code": self.code,
            "message": self.message,
            "request_id": self.request_id,
        }));
        (self.status, body).into_response()
    }
}

impl From<picroom_service::ServiceError> for ApiError {
    fn from(e: picroom_service::ServiceError) -> Self {
        match e {
            picroom_service::ServiceError::QuotaExceeded(_, _) => {
                Self::quota_exceeded(e.to_string())
            }
            picroom_service::ServiceError::PermissionDenied => Self::forbidden("permission denied"),
            picroom_service::ServiceError::Domain(d) => match d {
                picroom_domain::DomainError::NotFound => Self::not_found("not found"),
                picroom_domain::DomainError::PermissionDenied => Self::forbidden("forbidden"),
                picroom_domain::DomainError::Validation(msg) => Self::bad_request(msg),
                other => Self::internal(other.to_string()),
            },
            other => Self::internal(other.to_string()),
        }
    }
}
