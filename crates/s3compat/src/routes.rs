//! S3 router factory.

use axum::routing::{get, post};
use axum::Router;
use std::sync::Arc;

/// Returns an S3-compatible router mounted under `/s3`. Generic over the
/// state type so it can be nested under any host router.
pub fn s3_router<S: super::S3State>() -> Router<Arc<S>> {
    Router::new()
        // ListObjectsV2
        .route("/:bucket", get(super::list::list_objects_v2::<S>))
        // Object operations
        .route(
            "/:bucket/*key",
            get(super::object::get_object::<S>)
                .put(super::object::put_object::<S>)
                .head(super::object::head_object::<S>)
                .delete(super::object::delete_object::<S>),
        )
        // Multipart uploads (minimal — just init for now)
        .route(
            "/:bucket/*key",
            post(super::multipart::create_multipart::<S>),
        )
}
