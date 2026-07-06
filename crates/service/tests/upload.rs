//! Tests for the upload use case.

use bytes::Bytes;
use picroom_audit::{AuditAction, InMemoryAuditSink};
use picroom_domain::UserId;
use picroom_service::UploadService;
use picroom_storage::driver::LocalDriver;
use picroom_storage::StorageReader;
use std::path::PathBuf;
use std::sync::Arc;
use uuid::Uuid;

fn make_png(w: u32, h: u32) -> Bytes {
    use std::io::Cursor;
    let img = image::RgbImage::from_fn(w, h, |x, y| image::Rgb([x as u8, y as u8, 64]));
    let mut buf = Vec::new();
    image::DynamicImage::ImageRgb8(img)
        .write_to(&mut Cursor::new(&mut buf), image::ImageFormat::Png)
        .unwrap();
    Bytes::from(buf)
}

fn tempdir() -> PathBuf {
    let base = std::env::temp_dir().join(format!("picroom-svc-{}", Uuid::now_v7()));
    std::fs::create_dir_all(&base).unwrap();
    base
}

#[tokio::test]
async fn upload_stores_and_audits_png() {
    let tmp = tempdir();
    let driver = LocalDriver::new(tmp.clone(), "/i");
    let audit = InMemoryAuditSink::new();
    let driver_arc = Arc::new(driver.clone());
    let audit_arc = Arc::new(audit.clone());
    let svc = UploadService::new(driver_arc.clone(), audit_arc);

    let owner = UserId(Uuid::now_v7());
    let result = svc
        .upload(owner, "image/png", make_png(100, 80))
        .await
        .unwrap();

    assert_eq!(result.owner_id, owner);
    assert_eq!(result.width, 100);
    assert_eq!(result.height, 80);
    assert!(result.bytes > 0);

    // Verify storage
    let stored = driver.get(&result.key).await.unwrap();
    assert_eq!(stored.len(), result.bytes as usize);

    // Verify audit
    let events = audit.events();
    assert_eq!(events.len(), 1);
    assert!(matches!(events[0].action, AuditAction::ImageUpload));
    assert_eq!(events[0].target_id, Some(result.id.to_string()));
}

#[tokio::test]
async fn upload_rejects_empty_payload() {
    let tmp = tempdir();
    let driver = LocalDriver::new(tmp.clone(), "/i");
    let audit = InMemoryAuditSink::new();
    let svc = UploadService::new(Arc::new(driver), Arc::new(audit));

    let owner = UserId(Uuid::now_v7());
    let err = svc
        .upload(owner, "image/png", Bytes::new())
        .await
        .unwrap_err();
    assert!(format!("{err}").contains("empty"));
}

#[tokio::test]
async fn upload_rejects_unsupported_mime() {
    let tmp = tempdir();
    let driver = LocalDriver::new(tmp.clone(), "/i");
    let audit = InMemoryAuditSink::new();
    let svc = UploadService::new(Arc::new(driver), Arc::new(audit));

    let owner = UserId(Uuid::now_v7());
    let err = svc
        .upload(owner, "application/pdf", make_png(10, 10))
        .await
        .unwrap_err();
    assert!(format!("{err}").contains("unsupported"));
}

#[tokio::test]
async fn upload_rejects_oversized_payload() {
    let tmp = tempdir();
    let driver = LocalDriver::new(tmp.clone(), "/i");
    let audit = InMemoryAuditSink::new();
    let svc = UploadService::new(Arc::new(driver), Arc::new(audit)).with_max_bytes(10);

    let owner = UserId(Uuid::now_v7());
    let err = svc
        .upload(owner, "image/png", make_png(50, 50))
        .await
        .unwrap_err();
    assert!(format!("{err}").contains("exceeds"));
}

#[tokio::test]
async fn upload_rejects_garbage_bytes() {
    let tmp = tempdir();
    let driver = LocalDriver::new(tmp.clone(), "/i");
    let audit = InMemoryAuditSink::new();
    let svc = UploadService::new(Arc::new(driver), Arc::new(audit));

    let owner = UserId(Uuid::now_v7());
    let err = svc
        .upload(owner, "image/png", Bytes::from_static(b"not an image"))
        .await
        .unwrap_err();
    assert!(format!("{err}").contains("probe"));
}
