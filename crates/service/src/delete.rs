// SPDX-License-Identifier: MIT
// Copyright (c) 2026 Picroom Contributors

//! Delete use case.
//!
//! Single entry point for image deletion. Performs a soft delete of the DB
//! row and a best-effort removal of the original object from storage, then
//! emits an audit event. Callers remain responsible for authorization checks.

use crate::repo::ImageRepository;
use crate::ServiceError;
use picroom_audit::{AuditAction, AuditEvent, AuditSink};
use picroom_domain::Image;
use picroom_storage::StorageWriter;
use std::sync::Arc;
use time::OffsetDateTime;
use uuid::Uuid;

/// Delete service — unified image deletion (storage + DB + audit).
#[derive(Clone)]
pub struct DeleteService {
    storage: Arc<dyn StorageWriter + Send + Sync>,
    repo: Arc<dyn ImageRepository>,
    audit: Arc<dyn AuditSink>,
}

impl DeleteService {
    /// Creates a new delete service.
    pub fn new(
        storage: Arc<dyn StorageWriter + Send + Sync>,
        repo: Arc<dyn ImageRepository>,
        audit: Arc<dyn AuditSink>,
    ) -> Self {
        Self {
            storage,
            repo,
            audit,
        }
    }

    /// Deletes an image (DB row + storage object) and emits an audit event.
    ///
    /// Takes the already-resolved [`Image`] so callers can perform authorization
    /// and avoid a redundant lookup. This method only performs the deletion and
    /// records the audit event.
    pub async fn delete(&self, image: Image) -> Result<(), ServiceError> {
        // Remove the original object (best-effort — a missing blob must not
        // block the logical delete).
        if let Err(e) = self.storage.delete(&image.key).await {
            tracing::warn!(image_id = %image.id, error = %e, "storage delete failed");
        }

        // Soft-delete the DB row.
        self.repo.delete(image.id).await?;

        // Audit the deletion.
        let event = AuditEvent {
            id: Uuid::now_v7(),
            timestamp: OffsetDateTime::now_utc(),
            actor_id: None,
            actor_label: None,
            action: AuditAction::ImageDelete,
            target_type: "image".into(),
            target_id: Some(image.id.to_string()),
            ip: None,
            user_agent: None,
            metadata: serde_json::json!({ "owner_id": image.owner_id.to_string() }),
        };
        self.audit
            .record(&event)
            .await
            .map_err(crate::ServiceError::Audit)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use async_trait::async_trait;
    use bytes::Bytes;
    use picroom_audit::InMemoryAuditSink;
    use picroom_domain::{Image, ImageId, Page, PageReq, StorageKey, UserId};
    use picroom_storage::StorageError;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Arc;
    use uuid::Uuid;

    fn fake_image() -> Image {
        Image {
            id: ImageId(Uuid::now_v7()),
            owner_id: UserId(Uuid::now_v7()),
            team_id: None,
            key: StorageKey::parse("img/x.bin").unwrap(),
            content_type: "image/png".into(),
            bytes: 10,
            width: 1,
            height: 1,
            sha256: None,
            variants: vec![],
            created_at: OffsetDateTime::now_utc(),
        }
    }

    struct FakeRepo {
        deletes: AtomicUsize,
    }

    #[async_trait]
    impl ImageRepository for FakeRepo {
        async fn insert(&self, _image: &Image) -> Result<(), ServiceError> {
            Ok(())
        }
        async fn get(&self, _id: ImageId) -> Result<Image, ServiceError> {
            Err(ServiceError::Internal("unused in delete test".into()))
        }
        async fn list_for_owner(
            &self,
            _owner_id: Uuid,
            _page: PageReq,
        ) -> Result<Page<Image>, ServiceError> {
            Ok(Page::new(vec![], None, PageReq::default()))
        }
        async fn delete(&self, _id: ImageId) -> Result<(), ServiceError> {
            self.deletes.fetch_add(1, Ordering::SeqCst);
            Ok(())
        }
        async fn ping(&self) -> Result<(), ServiceError> {
            Ok(())
        }
    }

    struct FakeStorage {
        deletes: AtomicUsize,
    }

    #[async_trait]
    impl StorageWriter for FakeStorage {
        async fn put(&self, _key: &StorageKey, _bytes: Bytes) -> Result<(), StorageError> {
            Ok(())
        }
        async fn delete(&self, _key: &StorageKey) -> Result<(), StorageError> {
            self.deletes.fetch_add(1, Ordering::SeqCst);
            Ok(())
        }
    }

    #[tokio::test]
    async fn delete_removes_storage_object_and_db_row_and_audits() {
        let storage = Arc::new(FakeStorage {
            deletes: AtomicUsize::new(0),
        });
        let repo = Arc::new(FakeRepo {
            deletes: AtomicUsize::new(0),
        });
        let audit = Arc::new(InMemoryAuditSink::new());
        let svc = DeleteService::new(storage.clone(), repo.clone(), audit.clone());

        svc.delete(fake_image()).await.unwrap();

        assert_eq!(repo.deletes.load(Ordering::SeqCst), 1);
        assert_eq!(storage.deletes.load(Ordering::SeqCst), 1);
        let events = audit.events();
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].target_type, "image");
        assert!(events[0].target_id.is_some());
    }
}
