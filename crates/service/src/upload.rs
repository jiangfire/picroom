// SPDX-License-Identifier: MIT
// Copyright (c) 2026 Picroom Contributors

//! Upload use case.
//!
//! Validates the upload, probes the image, persists the original bytes
//! to the configured storage, records an audit event, optionally enqueues
//! variant-generation jobs, and returns the constructed `Image`. Database
//! persistence of the metadata row is the caller's responsibility
//! (see `repo::ImageRepository`).

use crate::QuotaService;
use crate::ServiceError;
use bytes::Bytes;
use picroom_audit::{AuditAction, AuditEvent, AuditSink};
use picroom_domain::{DomainError, Image, ImageId, StorageKey, UserId};
use picroom_imaging::processor::probe::probe_into;
use picroom_imaging::PipelineContext;
use picroom_storage::{StorageError, StorageWriter};
use picroom_worker::{Job, JobKind, JobQueue};
use std::sync::Arc;
use time::OffsetDateTime;
use uuid::Uuid;

/// Default MIME prefix required for upload.
const ALLOWED_MIME_PREFIXES: &[&str] = &["image/jpeg", "image/png", "image/webp", "image/gif"];

/// Default thumbnail sizes.
const DEFAULT_THUMBNAIL_SIZES: &[u32] = &[200, 400, 800];

/// Upload service.
#[derive(Clone)]
pub struct UploadService {
    /// Storage backend (trait object so callers can swap drivers).
    pub storage: Arc<dyn StorageWriter + Send + Sync>,
    /// Audit sink.
    pub audit: Arc<dyn AuditSink + Send + Sync>,
    /// Optional job queue for enqueuing variant-generation jobs.
    pub job_queue: Option<Arc<dyn JobQueue + Send + Sync>>,
    /// Storage policy name attached to uploaded images.
    pub default_storage_policy: String,
    /// Max upload size in bytes.
    pub max_bytes: u64,
    /// Thumbnail sizes to enqueue (empty = disabled).
    pub thumbnail_sizes: Vec<u32>,
    /// Whether to enqueue an AVIF job.
    pub enable_avif: bool,
    /// Whether to enqueue a WebP job.
    pub enable_webp: bool,
    /// Quota service used to enforce per-user byte caps.
    pub quota: QuotaService,
}

impl UploadService {
    /// Creates a new upload service.
    pub fn new(
        storage: Arc<dyn StorageWriter + Send + Sync>,
        audit: Arc<dyn AuditSink + Send + Sync>,
    ) -> Self {
        Self {
            storage,
            audit,
            job_queue: None,
            default_storage_policy: "default".to_string(),
            max_bytes: 100 * 1024 * 1024,
            thumbnail_sizes: DEFAULT_THUMBNAIL_SIZES.to_vec(),
            enable_avif: true,
            enable_webp: true,
            quota: QuotaService::new(),
        }
    }

    /// Sets the default storage policy name (default: `"default"`).
    pub fn with_storage_policy(mut self, name: impl Into<String>) -> Self {
        self.default_storage_policy = name.into();
        self
    }

    /// Sets the maximum allowed upload size (default: 100 MiB).
    pub const fn with_max_bytes(mut self, max: u64) -> Self {
        self.max_bytes = max;
        self
    }

    /// Sets the optional job queue for async variant generation.
    pub fn with_job_queue(mut self, q: Arc<dyn JobQueue + Send + Sync>) -> Self {
        self.job_queue = Some(q);
        self
    }

    /// Sets which thumbnail sizes to enqueue. Empty disables thumbnails.
    pub fn with_thumbnails(mut self, sizes: Vec<u32>) -> Self {
        self.thumbnail_sizes = sizes;
        self
    }

    /// Disables AVIF encoding.
    pub const fn without_avif(mut self) -> Self {
        self.enable_avif = false;
        self
    }

    /// Disables WebP encoding.
    pub const fn without_webp(mut self) -> Self {
        self.enable_webp = false;
        self
    }

    /// Sets the quota service used to enforce per-user byte caps.
    pub fn with_quota(mut self, q: QuotaService) -> Self {
        self.quota = q;
        self
    }

    /// Returns the underlying storage handle.
    pub fn storage(&self) -> &Arc<dyn StorageWriter + Send + Sync> {
        &self.storage
    }

    /// Validates, probes, persists, records audit, and enqueues variant jobs.
    #[allow(clippy::too_many_lines)]
    pub async fn upload(
        &self,
        owner_id: UserId,
        content_type: &str,
        bytes: Bytes,
    ) -> Result<Image, ServiceError> {
        // 1. Size check
        if bytes.is_empty() {
            return Err(DomainError::Validation("empty payload".into()).into());
        }
        if (bytes.len() as u64) > self.max_bytes {
            return Err(DomainError::Validation(format!(
                "payload exceeds {} bytes",
                self.max_bytes
            ))
            .into());
        }

        // 1.5 Quota check — reject before we touch storage so we never persist
        // bytes we would have to roll back. `remaining_user` returns the
        // user's cap minus already-stored bytes (or `u64::MAX` when unbacked).
        let remaining = self.quota.remaining_user(owner_id.as_uuid()).await?;
        if (bytes.len() as u64) > remaining {
            return Err(ServiceError::QuotaExceeded(bytes.len() as u64, remaining));
        }

        // 2. MIME check
        if !ALLOWED_MIME_PREFIXES
            .iter()
            .any(|p| content_type.starts_with(p))
        {
            return Err(DomainError::Validation(format!(
                "unsupported content type: {content_type}"
            ))
            .into());
        }

        // 3. Probe (populate width/height/mime)
        let mut ctx = PipelineContext::default();
        probe_into(&mut ctx, bytes.clone())
            .await
            .map_err(|e| ServiceError::Internal(format!("probe failed: {e}")))?;

        // 4. Persist original
        let id = Uuid::now_v7();
        let key = StorageKey::parse(&format!("img/{id}.bin"))
            .map_err(|e| StorageError::Config(e.to_string()))?;

        self.storage
            .put(&key, bytes.clone())
            .await
            .map_err(ServiceError::Storage)?;

        // 5. Build the entity
        let image = Image {
            id: ImageId(id),
            owner_id,
            team_id: None,
            key,
            content_type: content_type.to_string(),
            bytes: bytes.len() as u64,
            width: ctx.width.unwrap_or(0),
            height: ctx.height.unwrap_or(0),
            sha256: None,
            variants: vec![],
            created_at: OffsetDateTime::now_utc(),
        };

        // 6. Audit
        let event = AuditEvent {
            id: Uuid::now_v7(),
            timestamp: OffsetDateTime::now_utc(),
            actor_id: Some(owner_id.as_uuid()),
            actor_label: None,
            action: AuditAction::ImageUpload,
            target_type: "image".into(),
            target_id: Some(id.to_string()),
            ip: None,
            user_agent: None,
            metadata: serde_json::json!({
                "content_type": content_type,
                "bytes": bytes.len(),
                "width": image.width,
                "height": image.height,
                "storage_policy": &self.default_storage_policy,
            }),
        };
        self.audit
            .record(&event)
            .await
            .map_err(ServiceError::Audit)?;

        // 7. Enqueue variant-generation jobs (best-effort).
        if let Some(queue) = &self.job_queue {
            let enqueued_at = OffsetDateTime::now_utc();
            if self.enable_avif {
                if let Err(e) = queue
                    .enqueue(Job {
                        id: Uuid::now_v7(),
                        image_id: image.id,
                        kind: JobKind::EncodeAvif,
                        attempts: 0,
                        enqueued_at,
                    })
                    .await
                {
                    tracing::warn!(error = %e, "failed to enqueue avif job");
                }
            }
            if self.enable_webp {
                if let Err(e) = queue
                    .enqueue(Job {
                        id: Uuid::now_v7(),
                        image_id: image.id,
                        kind: JobKind::EncodeWebp,
                        attempts: 0,
                        enqueued_at,
                    })
                    .await
                {
                    tracing::warn!(error = %e, "failed to enqueue webp job");
                }
            }
            for size in &self.thumbnail_sizes {
                if let Err(e) = queue
                    .enqueue(Job {
                        id: Uuid::now_v7(),
                        image_id: image.id,
                        kind: JobKind::GenerateThumbnail { size: *size },
                        attempts: 0,
                        enqueued_at,
                    })
                    .await
                {
                    tracing::warn!(error = %e, "failed to enqueue thumbnail job");
                }
            }
        }

        Ok(image)
    }
}

/// Convenience extension: convert `ImageVariant` to its display string.
pub fn variant_label(v: &picroom_domain::ImageVariant) -> String {
    match v {
        picroom_domain::ImageVariant::Avif => "avif".into(),
        picroom_domain::ImageVariant::Webp => "webp".into(),
        picroom_domain::ImageVariant::Thumbnail(n) => format!("thumbnail_{n}"),
        picroom_domain::ImageVariant::Watermark => "watermark".into(),
    }
}
