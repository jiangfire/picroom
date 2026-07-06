//! Image query use case.

use crate::repo::ImageRepository;
use crate::ServiceError;
use bytes::Bytes;
use picroom_audit::{AuditAction, AuditEvent, AuditSink};
use picroom_domain::{DomainError, Image, ImageId, Page, PageReq};
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

    /// Stub helper for callers that don't have a repo yet (skeleton compat).
    pub async fn _stub(&self, _id: ImageId) -> Result<Image, ServiceError> {
        Err(DomainError::NotFound.into())
    }

    /// Placeholder; removed once repository wiring is complete.
    pub const fn _bytes() -> Bytes {
        Bytes::new()
    }
}
