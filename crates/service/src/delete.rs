//! Delete use case.

use crate::ServiceError;
use picroom_audit::{AuditAction, AuditEvent, AuditSink};
use picroom_domain::UserId;
use picroom_storage::StorageWriter;
use time::OffsetDateTime;
use uuid::Uuid;

/// Delete service (skeleton).
#[derive(Debug, Clone)]
pub struct DeleteService<S: StorageWriter, A: AuditSink> {
    storage: S,
    audit: A,
}

impl<S: StorageWriter, A: AuditSink> DeleteService<S, A> {
    /// Creates a new delete service.
    pub const fn new(storage: S, audit: A) -> Self {
        Self { storage, audit }
    }

    /// Deletes an image (DB row + storage object) and emits audit event.
    pub async fn delete(&self, actor_id: UserId, image_id: Uuid) -> Result<(), ServiceError> {
        // Placeholder: real implementation looks up the storage key first.
        let event = AuditEvent {
            id: Uuid::now_v7(),
            timestamp: OffsetDateTime::now_utc(),
            actor_id: Some(actor_id.as_uuid()),
            actor_label: None,
            action: AuditAction::ImageDelete,
            target_type: "image".into(),
            target_id: Some(image_id.to_string()),
            ip: None,
            user_agent: None,
            metadata: serde_json::Value::Null,
        };
        self.audit
            .record(&event)
            .await
            .map_err(crate::ServiceError::Audit)?;
        Ok(())
    }
}
