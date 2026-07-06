//! S3 `ListObjectsV2` handler.

use crate::S3State;
use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use picroom_domain::StorageKey;
use std::sync::Arc;

/// Query parameters for `ListObjectsV2`.
#[derive(serde::Deserialize, Default)]
pub struct ListParams {
    #[serde(rename = "list-type")]
    pub list_type: Option<u32>,
    pub prefix: Option<String>,
    pub delimiter: Option<String>,
    #[serde(rename = "max-keys")]
    pub max_keys: Option<u32>,
    pub continuation_token: Option<String>,
}

/// `GET /s3/:bucket` — `ListObjectsV2`.
pub async fn list_objects_v2<S: S3State>(
    State(state): State<Arc<S>>,
    Path(bucket): Path<String>,
    Query(_params): Query<ListParams>,
) -> Response {
    // List all objects with a prefix matching the full bucket prefix.
    // In path-style, objects are stored under /bucket/key; the storage
    // key is `key`, without the bucket prefix.
    let prefix = StorageKey::parse("");
    let prefix = match prefix {
        Ok(p) => p,
        Err(_) => {
            return s3_xml_error(StatusCode::BAD_REQUEST, "InvalidPrefix", "");
        }
    };

    match state.storage().list(&prefix).await {
        Ok(page) => {
            let items: Vec<String> = page
                .items
                .iter()
                .map(|m| format!(
                    r"<Contents><Key>{}</Key><Size>{}</Size></Contents>",
                    m.key.as_str(),
                    m.bytes
                ))
                .collect();

            let xml = format!(
                r#"<?xml version="1.0" encoding="UTF-8"?>
<ListBucketResult xmlns="http://s3.amazonaws.com/doc/2006-03-01/">
<Name>{bucket}</Name>
<IsTruncated>false</IsTruncated>
<KeyCount>{count}</KeyCount>
<MaxKeys>1000</MaxKeys>
{contents}
</ListBucketResult>"#,
                bucket = bucket,
                count = items.len(),
                contents = items.join("\n"),
            );
            (StatusCode::OK, [("content-type", "application/xml")], xml).into_response()
        }
        Err(e) => s3_xml_error(
            StatusCode::INTERNAL_SERVER_ERROR,
            "InternalError",
            &e.to_string(),
        ),
    }
}

fn s3_xml_error(status: StatusCode, code: &str, message: &str) -> Response {
    let xml = format!(
        r#"<?xml version="1.0" encoding="UTF-8"?><Error><Code>{code}</Code><Message>{message}</Message></Error>"#
    );
    (status, [("content-type", "application/xml")], xml).into_response()
}
