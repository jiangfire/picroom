// SPDX-License-Identifier: MIT
// Copyright (c) 2026 Picroom Contributors

//! S3 `ListObjectsV2` handler.

use crate::S3State;
use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
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
///
/// Picroom stores objects without a bucket prefix (path-style keys), so a
/// bucket maps to the entire backing store; `list` is invoked with no prefix
/// to enumerate every object.
pub async fn list_objects_v2<S: S3State>(
    State(state): State<Arc<S>>,
    Path(bucket): Path<String>,
    Query(_params): Query<ListParams>,
) -> Response {
    match state.storage().list(None).await {
        Ok(page) => {
            let items: Vec<String> = page
                .items
                .iter()
                .map(|m| {
                    format!(
                        r"<Contents><Key>{}</Key><Size>{}</Size></Contents>",
                        m.key.as_str(),
                        m.bytes
                    )
                })
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
