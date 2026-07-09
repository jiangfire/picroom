// SPDX-License-Identifier: MIT
// Copyright (c) 2026 Picroom Contributors

//! Tracing + request-id middleware.

use axum::http::{HeaderName, HeaderValue, Request};
use tower_http::request_id::{MakeRequestUuid, PropagateRequestIdLayer, SetRequestIdLayer};
use tower_http::trace::TraceLayer;
use uuid::Uuid;

/// Header name carrying the request id.
pub const REQUEST_ID_HEADER: HeaderName = HeaderName::from_static("x-request-id");

/// Layer that assigns a UUID to each request.
pub fn request_id_layer() -> SetRequestIdLayer<MakeRequestUuid> {
    SetRequestIdLayer::new(REQUEST_ID_HEADER, MakeRequestUuid)
}

/// Layer that propagates request id to the response.
pub fn propagate_request_id_layer() -> PropagateRequestIdLayer {
    PropagateRequestIdLayer::new(REQUEST_ID_HEADER)
}

/// Tracing layer.
pub fn trace_layer(
) -> TraceLayer<tower_http::classify::SharedClassifier<tower_http::classify::ServerErrorsAsFailures>>
{
    TraceLayer::new_for_http()
}

/// Convenience: read the request id from a request.
pub fn extract_request_id<B>(req: &Request<B>) -> Option<String> {
    req.headers()
        .get(&REQUEST_ID_HEADER)
        .and_then(|v: &HeaderValue| v.to_str().ok().map(String::from))
}

/// Convenience: build a typed UUID from a string id.
pub fn parse_request_id(s: &str) -> Option<Uuid> {
    Uuid::parse_str(s).ok()
}
