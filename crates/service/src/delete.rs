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
