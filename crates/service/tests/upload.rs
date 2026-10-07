// SPDX-License-Identifier: MIT
// Copyright (c) 2026 Picroom Contributors

//! Tests for the upload use case.

use bytes::Bytes;
use picroom_audit::{AuditAction, InMemoryAuditSink};
use picroom_auth::Actor;
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
        .upload(
            &Actor::with_roles(owner.as_uuid(), vec![picroom_auth::Role::Uploader]),
            "image/png",
            make_png(100, 80),
        )
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
        .upload(
            &Actor::with_roles(owner.as_uuid(), vec![picroom_auth::Role::Uploader]),
            "image/png",
            Bytes::new(),
        )
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
        .upload(
            &Actor::with_roles(owner.as_uuid(), vec![picroom_auth::Role::Uploader]),
            "application/pdf",
            make_png(10, 10),
        )
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
        .upload(
            &Actor::with_roles(owner.as_uuid(), vec![picroom_auth::Role::Uploader]),
            "image/png",
            make_png(50, 50),
        )
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
        .upload(
            &Actor::with_roles(owner.as_uuid(), vec![picroom_auth::Role::Uploader]),
            "image/png",
            Bytes::from_static(b"not an image"),
        )
        .await
        .unwrap_err();
    assert!(format!("{err}").contains("corrupt"));
}

/// Records every enqueued job with a global sequence number so tests can
/// prove ordering between staging, the (simulated) row insert, and enqueue.
#[derive(Default, Clone)]
struct RecordingQueue {
    jobs: Arc<std::sync::Mutex<Vec<picroom_worker::JobKind>>>,
}

#[async_trait::async_trait]
impl picroom_worker::JobQueue for RecordingQueue {
    async fn enqueue(&self, job: picroom_worker::Job) -> Result<(), picroom_worker::JobError> {
        self.jobs.lock().expect("mutex poisoned").push(job.kind);
        Ok(())
    }
    async fn dequeue(&self) -> Result<Option<picroom_worker::Job>, picroom_worker::JobError> {
        Ok(None)
    }
    async fn complete(
        &self,
        _id: uuid::Uuid,
        _result: &picroom_worker::JobResult,
    ) -> Result<(), picroom_worker::JobError> {
        Ok(())
    }
    async fn fail(&self, _id: uuid::Uuid, _error: &str) -> Result<(), picroom_worker::JobError> {
        Ok(())
    }
}

/// R-04: `stage()` must not enqueue anything — variant jobs are only created
/// by `enqueue_variants()`, which the handler calls after the `images` row is
/// committed. Otherwise a fast worker claims a job whose image row does not
/// exist yet and dead-letters a valid upload.
#[tokio::test]
async fn stage_enqueues_nothing_and_enqueue_variants_enqueues_after_insert() {
    let tmp = tempdir();
    let driver = LocalDriver::new(tmp.clone(), "/i");
    let audit = InMemoryAuditSink::new();
    let queue = RecordingQueue::default();
    let svc = UploadService::new(Arc::new(driver), Arc::new(audit))
        .with_job_queue(Arc::new(queue.clone()));

    let owner = UserId(Uuid::now_v7());
    let image = svc
        .stage(
            &Actor::with_roles(owner.as_uuid(), vec![picroom_auth::Role::Uploader]),
            None,
            "image/png",
            make_png(20, 10),
        )
        .await
        .unwrap();

    assert!(
        queue.jobs.lock().expect("mutex poisoned").is_empty(),
        "stage() must not enqueue any job"
    );

    // Simulate the handler: insert the row, then enqueue.
    let inserted = image.id;
    svc.enqueue_variants(&image).await;

    let kinds = queue.jobs.lock().expect("mutex poisoned").clone();
    assert_eq!(kinds.len(), 5, "avif + webp + 3 thumbnails");
    assert!(
        kinds.iter().all(|k| matches!(
            k,
            picroom_worker::JobKind::EncodeAvif
                | picroom_worker::JobKind::EncodeWebp
                | picroom_worker::JobKind::GenerateThumbnail { .. }
        )),
        "every job must reference the inserted image {inserted}"
    );
}

/// D-7 / R-05: enforcement lives in the service layer — a `viewer` Actor is
/// rejected with no HTTP layer involved.
#[tokio::test]
async fn viewer_actor_cannot_stage_upload() {
    let tmp = tempdir();
    let driver = LocalDriver::new(tmp.clone(), "/i");
    let audit = InMemoryAuditSink::new();
    let svc = UploadService::new(Arc::new(driver), Arc::new(audit))
        .with_authz(Arc::new(picroom_service::AuthzService::without_backends()));

    let viewer = Actor::with_roles(Uuid::now_v7(), vec![picroom_auth::Role::Viewer]);
    let err = svc
        .stage(&viewer, None, "image/png", make_png(20, 10))
        .await
        .unwrap_err();
    assert!(matches!(
        err,
        picroom_service::ServiceError::PermissionDenied
    ));

    // An uploader Actor passes.
    let uploader = Actor::with_roles(Uuid::now_v7(), vec![picroom_auth::Role::Uploader]);
    assert!(svc
        .stage(&uploader, None, "image/png", make_png(20, 10))
        .await
        .is_ok());
}

/// D-7: a non-owner Actor cannot delete through `DeleteService` even when no
/// HTTP layer is involved.
#[tokio::test]
async fn delete_service_rejects_non_owner_actor() {
    use picroom_service::DeleteService;

    struct NoopStorage;
    #[async_trait::async_trait]
    impl picroom_storage::StorageWriter for NoopStorage {
        async fn put(
            &self,
            _key: &picroom_domain::StorageKey,
            _bytes: Bytes,
        ) -> Result<(), picroom_storage::StorageError> {
            Ok(())
        }
        async fn delete(
            &self,
            _key: &picroom_domain::StorageKey,
        ) -> Result<(), picroom_storage::StorageError> {
            Ok(())
        }
    }
    struct NoopRepo;
    #[async_trait::async_trait]
    impl picroom_service::ImageRepository for NoopRepo {
        async fn insert(
            &self,
            _image: &picroom_domain::Image,
        ) -> Result<(), picroom_service::ServiceError> {
            Ok(())
        }
        async fn get(
            &self,
            _id: picroom_domain::ImageId,
        ) -> Result<picroom_domain::Image, picroom_service::ServiceError> {
            Err(picroom_service::ServiceError::Internal("unused".into()))
        }
        async fn list_for_owner(
            &self,
            _owner_id: Uuid,
            _page: picroom_domain::PageReq,
        ) -> Result<picroom_domain::Page<picroom_domain::Image>, picroom_service::ServiceError>
        {
            Ok(picroom_domain::Page::new(
                vec![],
                None,
                picroom_domain::PageReq::default(),
            ))
        }
        async fn delete(
            &self,
            _id: picroom_domain::ImageId,
        ) -> Result<(), picroom_service::ServiceError> {
            Ok(())
        }
        async fn ping(&self) -> Result<(), picroom_service::ServiceError> {
            Ok(())
        }
    }

    let owner = Uuid::now_v7();
    let image = picroom_domain::Image {
        id: picroom_domain::ImageId(Uuid::now_v7()),
        owner_id: UserId(owner),
        team_id: None,
        key: picroom_domain::StorageKey::parse("img/x.bin").unwrap(),
        content_type: "image/png".into(),
        bytes: 1,
        width: 1,
        height: 1,
        sha256: None,
        variants: vec![],
        created_at: time::OffsetDateTime::now_utc(),
    };
    let svc = DeleteService::new(
        Arc::new(NoopStorage),
        Arc::new(NoopRepo),
        Arc::new(picroom_audit::InMemoryAuditSink::new()),
    )
    .with_authz(Arc::new(picroom_service::AuthzService::without_backends()));

    // A random non-owner with no role is denied.
    let stranger = Actor::with_roles(Uuid::now_v7(), vec![]);
    assert!(matches!(
        svc.delete(&stranger, image.clone()).await,
        Err(picroom_service::ServiceError::PermissionDenied)
    ));
    // The owner is allowed (rule 2 — ownership).
    let owner_actor = Actor::with_roles(owner, vec![]);
    assert!(svc.delete(&owner_actor, image).await.is_ok());
}
