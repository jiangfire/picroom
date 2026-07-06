//! Application state shared across handlers.

use async_trait::async_trait;
use bytes::Bytes;
use picroom_audit::AuditSink;
use picroom_auth::JwtService;
use picroom_domain::Page as _Page;
use picroom_domain::UserId;
use picroom_service::repo::ImageRepository;
use picroom_service::UploadService;
use picroom_storage::Storage;
use picroom_storage::{ObjectMeta, StorageLister, StorageReader, StorageSigner, StorageWriter};
use std::sync::Arc;
use std::time::Duration;
use url::Url;
use uuid::Uuid;

use crate::extractors::auth::JwtProvider;

/// Concrete `UploadService`.
pub type DynUploadService = UploadService;

/// Shared application state.
#[derive(Clone)]
pub struct AppState {
    /// Image upload service.
    pub upload: Arc<DynUploadService>,
    /// Image repository (DB-backed).
    pub image_repo: Option<Arc<dyn ImageRepository>>,
    /// Storage (full set of capabilities).
    pub storage: Arc<dyn Storage>,
    /// Audit sink.
    pub audit: Arc<dyn AuditSink>,
    /// JWT service for auth.
    pub jwt: Arc<JwtService>,
    /// Dev-mode user (fallback when auth disabled).
    pub dev_user: UserId,
}

impl JwtProvider for AppState {
    fn jwt_service(&self) -> &JwtService {
        &self.jwt
    }
}

impl JwtProvider for Arc<AppState> {
    fn jwt_service(&self) -> &JwtService {
        &self.jwt
    }
}

impl AppState {
    /// Convenience: create a dev-mode `AppState`.
    pub fn for_dev<S, A>(storage: Arc<S>, audit: Arc<A>) -> Self
    where
        S: Storage + 'static,
        A: AuditSink + 'static,
    {
        let storage_arc: Arc<dyn StorageWriter + Send + Sync> =
            Arc::new(StorageWriterFromArc(storage.clone()));
        let audit_arc: Arc<dyn AuditSink + Send + Sync> = Arc::new(AuditSinkFromArc(audit.clone()));
        let upload = Arc::new(UploadService::new(storage_arc, audit_arc));
        Self {
            upload,
            image_repo: None,
            storage: storage as Arc<dyn Storage>,
            audit: audit as Arc<dyn AuditSink>,
            jwt: Arc::new(JwtService::new(
                "dev-secret",
                "picroom",
                "picroom-api",
                3600,
            )),
            dev_user: UserId(Uuid::nil()),
        }
    }

    /// Attaches an optional job queue so uploads enqueue variant jobs.
    #[must_use]
    pub fn with_optional_job_queue(
        mut self,
        q: Option<Arc<dyn picroom_worker::JobQueue + Send + Sync>>,
    ) -> Self {
        if let Some(q) = q {
            self.upload = Arc::new(UploadService {
                storage: self.upload.storage.clone(),
                audit: self.upload.audit.clone(),
                job_queue: Some(q),
                default_storage_policy: self.upload.default_storage_policy.clone(),
                max_bytes: self.upload.max_bytes,
                thumbnail_sizes: self.upload.thumbnail_sizes.clone(),
                enable_avif: self.upload.enable_avif,
                enable_webp: self.upload.enable_webp,
            });
        }
        self
    }
}

/// Implement `S3State` for `AppState` so the S3-compatible handlers can
/// access the storage backend.
#[async_trait]
impl picroom_s3compat::S3State for AppState {
    fn storage(&self) -> &Arc<dyn Storage> {
        &self.storage
    }
}

/// Adapter: `Arc<S>` → `StorageWriter + 'static`.
pub struct StorageWriterFromArc<S: Storage + ?Sized>(pub Arc<S>);

#[async_trait]
impl<S: Storage + ?Sized + Send + Sync> StorageWriter for StorageWriterFromArc<S> {
    async fn put(
        &self,
        key: &picroom_domain::StorageKey,
        bytes: Bytes,
    ) -> Result<(), picroom_storage::StorageError> {
        self.0.put(key, bytes).await
    }
    async fn delete(
        &self,
        key: &picroom_domain::StorageKey,
    ) -> Result<(), picroom_storage::StorageError> {
        self.0.delete(key).await
    }
}

#[async_trait]
impl<S: Storage + ?Sized + Send + Sync> StorageReader for StorageWriterFromArc<S> {
    async fn get(
        &self,
        key: &picroom_domain::StorageKey,
    ) -> Result<Bytes, picroom_storage::StorageError> {
        self.0.get(key).await
    }
    async fn head(
        &self,
        key: &picroom_domain::StorageKey,
    ) -> Result<ObjectMeta, picroom_storage::StorageError> {
        self.0.head(key).await
    }
    async fn exists(
        &self,
        key: &picroom_domain::StorageKey,
    ) -> Result<bool, picroom_storage::StorageError> {
        self.0.exists(key).await
    }
}

#[async_trait]
impl<S: Storage + ?Sized + Send + Sync> StorageLister for StorageWriterFromArc<S> {
    async fn list(
        &self,
        prefix: &picroom_domain::StorageKey,
    ) -> Result<_Page<ObjectMeta>, picroom_storage::StorageError> {
        self.0.list(prefix).await
    }
}

#[async_trait]
impl<S: Storage + ?Sized + Send + Sync> StorageSigner for StorageWriterFromArc<S> {
    async fn sign_get_url(
        &self,
        key: &picroom_domain::StorageKey,
        ttl: Duration,
    ) -> Result<Url, picroom_storage::StorageError> {
        self.0.sign_get_url(key, ttl).await
    }
    async fn sign_put_url(
        &self,
        key: &picroom_domain::StorageKey,
        ttl: Duration,
    ) -> Result<Url, picroom_storage::StorageError> {
        self.0.sign_put_url(key, ttl).await
    }
}

/// Adapter: `Arc<A>` → `AuditSink`.
pub struct AuditSinkFromArc<A: AuditSink + ?Sized>(Arc<A>);

#[async_trait]
impl<A: AuditSink + ?Sized + Send + Sync> AuditSink for AuditSinkFromArc<A> {
    async fn record(
        &self,
        event: &picroom_audit::AuditEvent,
    ) -> Result<(), picroom_audit::sink::AuditSinkError> {
        self.0.record(event).await
    }
}
