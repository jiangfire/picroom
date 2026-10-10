// SPDX-License-Identifier: MIT
// Copyright (c) 2026 Picroom Contributors

//! Integration tests for the S3 driver against a wiremock server.

use bytes::Bytes;
use picroom_domain::StorageKey;
use picroom_storage::driver::s3::{S3Config, S3Driver};
use picroom_storage::driver::{StorageLister, StorageReader, StorageSigner, StorageWriter};
use picroom_storage::StorageError;
use std::time::Duration;
use wiremock::matchers::{method, path_regex, query_param, query_param_is_missing};
use wiremock::{Mock, MockServer, ResponseTemplate};

async fn driver(server: &MockServer) -> S3Driver {
    let cfg = S3Config::new(
        "test-bucket",
        "us-east-1",
        "AKID",
        "shh-secret-very-long-enough-for-hs256",
    )
    .with_endpoint(server.uri())
    .with_path_style(true);
    S3Driver::new(cfg).unwrap()
}

#[tokio::test]
async fn put_then_get_roundtrip() {
    let server = MockServer::start().await;

    Mock::given(method("PUT"))
        .and(path_regex(r"^/test-bucket/.+"))
        .respond_with(ResponseTemplate::new(200))
        .expect(1)
        .mount(&server)
        .await;

    let key = StorageKey::parse("img/sample.bin").unwrap();
    let payload = Bytes::from_static(b"hello, s3!");
    Mock::given(method("GET"))
        .and(path_regex(r"^/test-bucket/img/sample\.bin$"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_bytes(payload.clone())
                .insert_header("content-length", payload.len().to_string())
                .insert_header("etag", "\"deadbeef\""),
        )
        .expect(1)
        .mount(&server)
        .await;

    let d = driver(&server).await;
    d.put(&key, payload.clone()).await.expect("PUT");
    let got = d.get(&key).await.expect("GET");
    assert_eq!(got, payload);
}

#[tokio::test]
async fn get_missing_returns_not_found() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path_regex(r"^/test-bucket/.+"))
        .respond_with(ResponseTemplate::new(404))
        .expect(1)
        .mount(&server)
        .await;

    let d = driver(&server).await;
    let key = StorageKey::parse("missing.bin").unwrap();
    let err = d.get(&key).await.unwrap_err();
    assert!(matches!(err, StorageError::NotFound(_)), "got {err:?}");
}

#[tokio::test]
async fn delete_idempotent() {
    let server = MockServer::start().await;
    Mock::given(method("DELETE"))
        .and(path_regex(r"^/test-bucket/.+"))
        .respond_with(ResponseTemplate::new(204))
        .expect(1)
        .mount(&server)
        .await;

    let d = driver(&server).await;
    let key = StorageKey::parse("img/x.bin").unwrap();
    d.delete(&key).await.expect("DELETE should succeed");
}

/// A `ListObjectsV2` response carries at most `MaxKeys` entries and reports the
/// rest through `IsTruncated` plus `NextContinuationToken`. Reading only the
/// first response silently dropped everything past that cap, so the driver has
/// to follow the token until the backend says it is done.
#[tokio::test]
async fn list_follows_continuation_token_across_pages() {
    const PAGE_ONE: &str = concat!(
        r#"<?xml version="1.0" encoding="UTF-8"?>"#,
        "\n",
        "<ListBucketResult>",
        "<Name>test-bucket</Name>",
        "<IsTruncated>true</IsTruncated>",
        "<NextContinuationToken>tok-1</NextContinuationToken>",
        "<KeyCount>2</KeyCount>",
        "<Contents><Key>img/a.png</Key><Size>10</Size></Contents>",
        "<Contents><Key>img/b.png</Key><Size>20</Size></Contents>",
        "</ListBucketResult>",
    );
    // Deliberately emitted as a single line: a driver that scraped the body
    // line by line would read neither its keys nor its IsTruncated flag.
    const PAGE_TWO: &str = concat!(
        r#"<?xml version="1.0" encoding="UTF-8"?>"#,
        "<ListBucketResult><Name>test-bucket</Name>",
        "<IsTruncated>false</IsTruncated><KeyCount>1</KeyCount>",
        "<Contents><Key>img/c.png</Key><Size>30</Size></Contents>",
        "</ListBucketResult>",
    );

    let server = MockServer::start().await;

    Mock::given(method("GET"))
        .and(path_regex(r"^/test-bucket"))
        .and(query_param_is_missing("continuation-token"))
        .respond_with(ResponseTemplate::new(200).set_body_string(PAGE_ONE))
        .expect(1)
        .mount(&server)
        .await;

    Mock::given(method("GET"))
        .and(path_regex(r"^/test-bucket"))
        .and(query_param("continuation-token", "tok-1"))
        .respond_with(ResponseTemplate::new(200).set_body_string(PAGE_TWO))
        .expect(1)
        .mount(&server)
        .await;

    let d = driver(&server).await;
    let page = d.list(None).await.expect("list");

    let keys: Vec<&str> = page.items.iter().map(|m| m.key.as_str()).collect();
    assert_eq!(keys, ["img/a.png", "img/b.png", "img/c.png"]);
    let sizes: Vec<u64> = page.items.iter().map(|m| m.bytes).collect();
    assert_eq!(sizes, [10, 20, 30]);
    // The listing is complete, so there is nothing left to page.
    assert!(page.next_cursor.is_none());
    assert!(!page.has_more);
}

/// A backend that keeps answering `IsTruncated=true` with the same token must
/// not spin forever — the driver stops once the token stops advancing.
#[tokio::test]
async fn list_stops_when_continuation_token_does_not_advance() {
    const STUCK: &str = concat!(
        "<ListBucketResult><IsTruncated>true</IsTruncated>",
        "<NextContinuationToken>stuck</NextContinuationToken>",
        "<Contents><Key>img/a.png</Key><Size>1</Size></Contents>",
        "</ListBucketResult>",
    );

    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path_regex(r"^/test-bucket"))
        .respond_with(ResponseTemplate::new(200).set_body_string(STUCK))
        .expect(2)
        .mount(&server)
        .await;

    let d = driver(&server).await;
    // Without the non-advancing guard this would keep paging until MAX_PAGES.
    let page = d.list(None).await.expect("list terminates");
    assert_eq!(page.items.len(), 2);
}

#[tokio::test]
async fn delete_missing_treated_as_success() {
    let server = MockServer::start().await;
    Mock::given(method("DELETE"))
        .and(path_regex(r"^/test-bucket/.+"))
        .respond_with(ResponseTemplate::new(404))
        .expect(1)
        .mount(&server)
        .await;

    let d = driver(&server).await;
    let key = StorageKey::parse("does-not-exist.bin").unwrap();
    d.delete(&key).await.expect("DELETE missing should be OK");
}

#[tokio::test]
async fn head_returns_metadata() {
    let server = MockServer::start().await;
    Mock::given(method("HEAD"))
        .and(path_regex(r"^/test-bucket/m\.bin$"))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("content-length", "42")
                .insert_header("etag", "\"abc123\"")
                .insert_header("last-modified", "Sun, 02 Jul 2006 22:04:46 GMT"),
        )
        .expect(1)
        .mount(&server)
        .await;

    let d = driver(&server).await;
    let key = StorageKey::parse("m.bin").unwrap();
    let meta = d.head(&key).await.expect("HEAD");
    assert_eq!(meta.bytes, 42);
    assert_eq!(meta.etag.as_deref(), Some("abc123"));
}

#[tokio::test]
async fn exists_returns_true_when_present() {
    let server = MockServer::start().await;
    Mock::given(method("HEAD"))
        .and(path_regex(r"^/test-bucket/.+"))
        .respond_with(ResponseTemplate::new(200).insert_header("content-length", "1"))
        .expect(1)
        .mount(&server)
        .await;

    let d = driver(&server).await;
    let key = StorageKey::parse("p.bin").unwrap();
    assert!(d.exists(&key).await.unwrap());
}

#[tokio::test]
async fn exists_returns_false_when_404() {
    let server = MockServer::start().await;
    Mock::given(method("HEAD"))
        .and(path_regex(r"^/test-bucket/.+"))
        .respond_with(ResponseTemplate::new(404))
        .expect(1)
        .mount(&server)
        .await;

    let d = driver(&server).await;
    let key = StorageKey::parse("n.bin").unwrap();
    assert!(!d.exists(&key).await.unwrap());
}

#[tokio::test]
async fn sign_get_url_is_well_formed_and_signed() {
    let server = MockServer::start().await;
    let d = driver(&server).await;
    let key = StorageKey::parse("img/x.png").unwrap();
    let url = d
        .sign_get_url(&key, Duration::from_secs(600))
        .await
        .unwrap();
    let s = url.as_str();
    assert!(s.contains("/img/x.png"), "url={s}");
    assert!(s.contains("X-Amz-Signature="));
    assert!(s.contains("X-Amz-Algorithm=AWS4-HMAC-SHA256"));
    assert!(s.contains("X-Amz-Expires=600"));
    assert!(s.contains("X-Amz-Credential="));
}
