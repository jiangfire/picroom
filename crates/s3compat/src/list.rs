// SPDX-License-Identifier: MIT
// Copyright (c) 2026 Picroom Contributors

//! S3 `ListObjectsV2` handler.

use crate::error::{xml_error, xml_escape};
use crate::S3State;
use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use std::str::FromStr;
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
    #[serde(rename = "continuation-token")]
    pub continuation_token: Option<String>,
}

/// `GET /s3/:bucket` — `ListObjectsV2`.
///
/// Picroom stores objects without a bucket prefix (path-style keys), so a
/// bucket maps to the entire backing store. `prefix`, `max-keys` and
/// `continuation-token` are honored (R-15/R-16): the token is the last key of
/// the previous page and listing resumes strictly after it, matching S3's
/// lexicographic semantics closely enough for `aws s3 ls`/`rclone` paging.
pub async fn list_objects_v2<S: S3State>(
    State(state): State<Arc<S>>,
    Path(bucket): Path<String>,
    Query(params): Query<ListParams>,
) -> Response {
    // R-15: malformed name -> InvalidBucketName; a configured deployment's
    // bucket mismatch -> NoSuchBucket.
    if crate::bucket::BucketName::from_str(&bucket).is_err() {
        return xml_error(
            StatusCode::BAD_REQUEST,
            "InvalidBucketName",
            "The specified bucket is not valid.",
        );
    }
    if let Some(expected) = state.expected_bucket() {
        if expected != bucket {
            return xml_error(
                StatusCode::NOT_FOUND,
                "NoSuchBucket",
                &format!("The specified bucket does not exist: {bucket}"),
            );
        }
    }

    let max_keys = params.max_keys.unwrap_or(1000).clamp(1, 1000) as usize;
    let prefix = params.prefix.unwrap_or_default();
    let token = params.continuation_token.unwrap_or_default();

    match state.storage().list(None).await {
        Ok(page) => {
            // Filter by prefix, resume after the continuation token, then cap
            // at max_keys — S3 returns keys in lexicographic order.
            let mut keyed: Vec<(String, u64)> = page
                .items
                .iter()
                .map(|m| (m.key.as_str().to_string(), m.bytes))
                .filter(|(k, _)| k.starts_with(&prefix))
                .filter(|(k, _)| token.is_empty() || k.as_str() > token.as_str())
                .collect();
            keyed.sort_by(|a, b| a.0.cmp(&b.0));
            let truncated = keyed.len() > max_keys;
            let keyed: Vec<(String, u64)> = keyed.into_iter().take(max_keys).collect();

            // Client-supplied text (prefix, keys, the continuation token) is
            // XML-escaped — an unescaped `&`/`<` in a prefix used to produce
            // malformed XML and could inject fake <Contents> entries.
            let contents: Vec<String> = keyed
                .iter()
                .map(|(k, bytes)| {
                    format!(
                        "<Contents><Key>{}</Key><Size>{bytes}</Size></Contents>",
                        xml_escape(k)
                    )
                })
                .collect();

            let next_token = if truncated {
                keyed.last().map(|(k, _)| k.clone())
            } else {
                None
            };

            let bucket_escaped = xml_escape(&bucket);
            let prefix_escaped = xml_escape(&prefix);
            let xml = format!(
                r#"<?xml version="1.0" encoding="UTF-8"?>
<ListBucketResult xmlns="http://s3.amazonaws.com/doc/2006-03-01/">
<Name>{bucket_escaped}</Name>
<IsTruncated>{truncated}</IsTruncated>
<KeyCount>{count}</KeyCount>
<MaxKeys>{max_keys}</MaxKeys>
<Prefix>{prefix_escaped}</Prefix>
{next}
{contents}
</ListBucketResult>"#,
                bucket_escaped = bucket_escaped,
                truncated = truncated,
                count = contents.len(),
                max_keys = max_keys,
                prefix_escaped = prefix_escaped,
                next = next_token
                    .map(|t| {
                        format!(
                            "<NextContinuationToken>{}</NextContinuationToken>",
                            xml_escape(&t)
                        )
                    })
                    .unwrap_or_default(),
                contents = contents.join("\n"),
            );
            (StatusCode::OK, [("content-type", "application/xml")], xml).into_response()
        }
        Err(e) => {
            tracing::error!("s3 list failed: {e}");
            xml_error(
                StatusCode::INTERNAL_SERVER_ERROR,
                "InternalError",
                "An internal error occurred",
            )
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_util::TestState;
    use axum::body::to_bytes;
    use axum::extract::{Path, Query, State};
    use bytes::Bytes;
    use picroom_domain::StorageKey;
    use std::sync::Arc;

    fn state() -> Arc<TestState> {
        Arc::new(TestState::new())
    }

    async fn body_string(response: Response) -> String {
        let bytes = to_bytes(response.into_body(), usize::MAX).await.unwrap();
        String::from_utf8_lossy(&bytes).into_owned()
    }

    #[tokio::test]
    async fn lists_all_objects_when_present() {
        let st = state();
        st.storage()
            .put(
                &StorageKey::parse("img/a.png").unwrap(),
                Bytes::from_static(b"1"),
            )
            .await
            .unwrap();
        st.storage()
            .put(
                &StorageKey::parse("img/b.jpg").unwrap(),
                Bytes::from_static(b"2"),
            )
            .await
            .unwrap();

        let resp = list_objects_v2(
            State(st),
            Path("mybucket".into()),
            Query(ListParams::default()),
        )
        .await;
        assert_eq!(resp.status(), StatusCode::OK);
        let body = body_string(resp).await;
        assert!(body.contains("<KeyCount>2</KeyCount>"));
        assert!(body.contains("<Key>img/a.png</Key>"));
        assert!(body.contains("<Key>img/b.jpg</Key>"));
    }

    #[tokio::test]
    async fn lists_empty_when_no_objects() {
        let st = state();
        let resp = list_objects_v2(
            State(st),
            Path("mybucket".into()),
            Query(ListParams::default()),
        )
        .await;
        assert_eq!(resp.status(), StatusCode::OK);
        assert!(body_string(resp).await.contains("<KeyCount>0</KeyCount>"));
    }

    #[tokio::test]
    async fn internal_error_on_storage_failure() {
        let st = state();
        st.set_fail(true);
        let resp = list_objects_v2(
            State(st),
            Path("mybucket".into()),
            Query(ListParams::default()),
        )
        .await;
        assert_eq!(resp.status(), StatusCode::INTERNAL_SERVER_ERROR);
    }
}
