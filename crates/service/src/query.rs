// SPDX-License-Identifier: MIT
// Copyright (c) 2026 Picroom Contributors

//! Image query use case.

use crate::repo::ImageRepository;
use crate::ServiceError;
use picroom_audit::{AuditAction, AuditEvent, AuditSink};
use picroom_domain::{Image, ImageId, Page, PageReq};
use picroom_storage::StorageError;
use picroom_storage::StorageWriter;
use std::sync::Arc;
use time::OffsetDateTime;
use uuid::Uuid;

/// Image query service backed by an `ImageRepository`.
#[derive(Clone)]
pub struct ImageQueryService {
    repo: Arc<dyn ImageRepository>,
}

impl ImageQueryService {
    /// Creates a new query service.
    pub fn new(repo: Arc<dyn ImageRepository>) -> Self {
        Self { repo }
    }

    /// Lists images for the given owner.
    pub async fn list_for_owner(
        &self,
        owner_id: Uuid,
        page: PageReq,
    ) -> Result<Page<Image>, ServiceError> {
        self.repo.list_for_owner(owner_id, page).await
    }

    /// Fetches an image by id.
    pub async fn get(&self, id: ImageId) -> Result<Image, ServiceError> {
        self.repo.get(id).await
    }

    /// Deletes an image (storage + DB).
    pub async fn delete<S: StorageWriter, A: AuditSink>(
        &self,
        repo: &dyn ImageRepository,
        storage: &S,
        audit: &A,
        actor_id: Uuid,
        image_id: ImageId,
    ) -> Result<(), ServiceError> {
        let img = repo.get(image_id).await?;
        // Best-effort storage delete; if missing, treat as success.
        if let Err(e) = storage.delete(&img.key).await {
            if !matches!(e, StorageError::NotFound(_)) {
                return Err(ServiceError::Storage(e));
            }
        }
        repo.delete(image_id).await?;

        let event = AuditEvent {
            id: Uuid::now_v7(),
            timestamp: OffsetDateTime::now_utc(),
            actor_id: Some(actor_id),
            actor_label: None,
            action: AuditAction::ImageDelete,
            target_type: "image".into(),
            target_id: Some(image_id.to_string()),
            ip: None,
            user_agent: None,
            metadata: serde_json::Value::Null,
        };
        audit.record(&event).await.map_err(ServiceError::Audit)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use async_trait::async_trait;
    use bytes::Bytes;
    use picroom_audit::{AuditAction, InMemoryAuditSink};
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
        image: Image,
        gets: AtomicUsize,
        deletes: AtomicUsize,
        lists: AtomicUsize,
    }

    #[async_trait]
    impl ImageRepository for FakeRepo {
        async fn insert(&self, _image: &Image) -> Result<(), ServiceError> {
            Ok(())
        }
        async fn get(&self, _id: ImageId) -> Result<Image, ServiceError> {
            self.gets.fetch_add(1, Ordering::SeqCst);
            Ok(self.image.clone())
        }
        async fn list_for_owner(
            &self,
            _owner_id: Uuid,
            _page: PageReq,
        ) -> Result<Page<Image>, ServiceError> {
            self.lists.fetch_add(1, Ordering::SeqCst);
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
    async fn get_delegates_to_repository() {
        let img = fake_image();
        let repo = Arc::new(FakeRepo {
            image: img.clone(),
            gets: AtomicUsize::new(0),
            deletes: AtomicUsize::new(0),
            lists: AtomicUsize::new(0),
        });
        let q = ImageQueryService::new(repo.clone());
        let got = q.get(img.id).await.unwrap();
        assert_eq!(got.id, img.id);
        assert_eq!(repo.gets.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn list_for_owner_delegates_to_repository() {
        let repo = Arc::new(FakeRepo {
            image: fake_image(),
            gets: AtomicUsize::new(0),
            deletes: AtomicUsize::new(0),
            lists: AtomicUsize::new(0),
        });
        let q = ImageQueryService::new(repo.clone());
        let _ = q
            .list_for_owner(Uuid::now_v7(), PageReq::default())
            .await
            .unwrap();
        assert_eq!(repo.lists.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn delete_records_audit_and_persists() {
        let img = fake_image();
        let repo = Arc::new(FakeRepo {
            image: img.clone(),
            gets: AtomicUsize::new(0),
            deletes: AtomicUsize::new(0),
            lists: AtomicUsize::new(0),
        });
        let storage = Arc::new(FakeStorage {
            deletes: AtomicUsize::new(0),
        });
        let audit = Arc::new(InMemoryAuditSink::new());
        let q = ImageQueryService::new(repo.clone());

        q.delete(&*repo, &*storage, &*audit, Uuid::now_v7(), img.id)
            .await
            .unwrap();

        assert_eq!(repo.deletes.load(Ordering::SeqCst), 1);
        assert_eq!(storage.deletes.load(Ordering::SeqCst), 1);
        let events = audit.events();
        assert_eq!(events.len(), 1);
        assert!(matches!(events[0].action, AuditAction::ImageDelete));
    }
}
