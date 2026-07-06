//! Multipart upload handlers (skeleton — full multipart support is post-MVP).

use crate::S3State;
use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use std::sync::Arc;

/// `POST /s3/:bucket/:key?uploads` — initiate multipart.
/// Returns an `UploadId` (stub — always returns the same ID).
pub async fn create_multipart<S: S3State>(
    State(_state): State<Arc<S>>,
    Path((bucket, key)): Path<(String, String)>,
) -> Response {
    let xml = format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<InitiateMultipartUploadResult xmlns="http://s3.amazonaws.com/doc/2006-03-01/">
<Bucket>{bucket}</Bucket>
<Key>{key}</Key>
<UploadId>picroom-multipart-1</UploadId>
</InitiateMultipartUploadResult>"#
    );
    (StatusCode::OK, [("content-type", "application/xml")], xml).into_response()
}

/// `PUT /s3/:bucket/:key?partNumber=N&uploadId=U` — upload part (stub).
pub async fn upload_part<S: S3State>(
    State(_state): State<Arc<S>>,
) -> Response {
    (StatusCode::OK, [("etag", "\"part-etag\"")]).into_response()
}

/// `POST /s3/:bucket/:key?uploadId=U` — complete multipart (stub).
pub async fn complete_multipart<S: S3State>(
    State(_state): State<Arc<S>>,
) -> Response {
    let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
<CompleteMultipartUploadResult xmlns="http://s3.amazonaws.com/doc/2006-03-01/">
<Location>/s3/bucket/key</Location>
<Bucket>bucket</Bucket>
<Key>key</Key>
<ETag>"complete-etag"</ETag>
</CompleteMultipartUploadResult>"#
        .to_string();
    (StatusCode::OK, [("content-type", "application/xml")], xml).into_response()
}

/// `DELETE /s3/:bucket/:key?uploadId=U` — abort multipart (stub).
pub async fn abort_multipart<S: S3State>(
    State(_state): State<Arc<S>>,
) -> Response {
    StatusCode::NO_CONTENT.into_response()
}
