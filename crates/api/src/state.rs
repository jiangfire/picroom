// SPDX-License-Identifier: MIT
// Copyright (c) 2026 Picroom Contributors

//! Application state shared across handlers.

use async_trait::async_trait;
use bytes::Bytes;
use picroom_audit::{AuditReader, AuditSink};
use picroom_auth::JwtService;
use picroom_domain::Page as _Page;
use picroom_infra::config::OidcProviderConfig;
use picroom_service::repo::{
    ImageRepository, ResourceAclRepository, SessionRepository, StoragePolicyRepository,
    TeamRepository, UserRepository,
};
use picroom_service::AuthzService;
use picroom_service::DeleteService;
use picroom_service::PermissionService;
use picroom_service::QuotaService;
use picroom_service::UploadService;
use picroom_storage::Storage;
use picroom_storage::{ObjectMeta, StorageLister, StorageReader, StorageSigner, StorageWriter};
use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use std::time::Duration;
use url::Url;

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
    /// User repository (used by the login handler to verify credentials).
    pub user_repo: Option<Arc<dyn UserRepository>>,
    /// Storage (full set of capabilities).
    pub storage: Arc<dyn Storage>,
    /// Audit sink.
    pub audit: Arc<dyn AuditSink>,
    /// JWT service for auth.
    pub jwt: Arc<JwtService>,
    /// RBAC permission service (replaces ad-hoc role checks in handlers).
    pub permissions: Arc<PermissionService>,
    /// Team repository (None when running without a DB).
    pub team_repo: Option<Arc<dyn TeamRepository>>,
    /// Audit log reader (None when running without a DB).
    pub audit_reader: Option<Arc<dyn AuditReader>>,
    /// Storage-policy repository (None when running without a DB).
    pub storage_policy_repo: Option<Arc<dyn StoragePolicyRepository>>,
    /// Unified delete service (None when running without a DB).
    pub delete_service: Option<Arc<DeleteService>>,
    /// Optional S3 client credential; when set, the S3 endpoint enforces `SigV4`.
    pub s3_credentials: Option<picroom_s3compat::S3Credential>,
    /// The S3 bucket this deployment serves; requests naming another bucket
    /// get `NoSuchBucket` instead of sharing one flat namespace (R-15).
    pub s3_bucket: Option<String>,
    /// Public base URL for image links (e.g. `"https://cdn.example.com"`).
    /// When `None`, the link handler emits a path-relative `/i/{key}` URL.
    pub public_url_base: Option<String>,
    /// OIDC provider configurations, keyed by provider name (e.g. `"google"`).
    pub oidc_providers: Arc<HashMap<String, OidcProviderConfig>>,
    /// Emails promoted to `admin` on first OIDC login.
    pub oidc_admin_emails: Arc<HashSet<String>>,
    /// Whether OIDC state cookies are marked `Secure` (false for local HTTP dev).
    pub cookie_secure: bool,
    /// Refuse sid-less tokens when a session repository is configured
    /// (`[auth].require_sessions`; the D-6 compat window).
    pub require_sessions: bool,
    /// Login-session repository (None without a DB). Makes `logout` and the
    /// disable-user cascade revoke outstanding tokens (D-6).
    pub session_repo: Option<Arc<dyn SessionRepository>>,
    /// ACL grant repository backing the `/acl` endpoints (None without a DB).
    pub acl_repo: Option<Arc<dyn ResourceAclRepository>>,
    /// Authorization coordinator shared by the service layer and the ACL
    /// endpoints.
    pub authz: Arc<AuthzService>,
}

impl JwtProvider for AppState {
    fn jwt_service(&self) -> &JwtService {
        &self.jwt
    }

    fn session_repo(&self) -> Option<&Arc<dyn SessionRepository>> {
        self.session_repo.as_ref()
    }

    fn require_sessions(&self) -> bool {
        self.require_sessions
    }
}

impl JwtProvider for Arc<AppState> {
    fn jwt_service(&self) -> &JwtService {
        &self.jwt
    }

    fn session_repo(&self) -> Option<&Arc<dyn SessionRepository>> {
        self.session_repo.as_ref()
    }

    fn require_sessions(&self) -> bool {
        self.require_sessions
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
        let upload =
            Arc::new(UploadService::new(storage_arc, audit_arc).with_quota(QuotaService::new()));
        Self {
            upload,
            image_repo: None,
            user_repo: None,
            storage: storage as Arc<dyn Storage>,
            audit: audit as Arc<dyn AuditSink>,
            jwt: Arc::new(JwtService::new(
                "dev-secret",
                "picroom",
                "picroom-api",
                3600,
            )),
            permissions: Arc::new(PermissionService::new()),
            team_repo: None,
            audit_reader: None,
            delete_service: None,
            s3_credentials: None,
            s3_bucket: None,
            public_url_base: None,
            storage_policy_repo: None,
            oidc_providers: Arc::new(HashMap::new()),
            oidc_admin_emails: Arc::new(HashSet::new()),
            cookie_secure: false,
            require_sessions: false,
            session_repo: None,
            acl_repo: None,
            authz: Arc::new(AuthzService::without_backends()),
        }
    }

    /// Attaches the session repository (PostgreSQL-backed).
    #[must_use]
    pub fn with_session_repo(mut self, repo: Arc<dyn SessionRepository>) -> Self {
        self.session_repo = Some(repo);
        self
    }

    /// Sets the authorization coordinator.
    #[must_use]
    pub fn with_authz(mut self, authz: Arc<AuthzService>) -> Self {
        self.authz = authz;
        self
    }

    /// Sets the authorization coordinator used by the upload service (the
    /// service-layer `Image/Create` / `Image/Delete` checks). Test and
    /// wiring convenience over rebuilding `upload` by hand.
    #[must_use]
    pub fn with_upload_authz(mut self, authz: &AuthzService) -> Self {
        self.upload = Arc::new(UploadService {
            storage: self.upload.storage.clone(),
            audit: self.upload.audit.clone(),
            job_queue: self.upload.job_queue.clone(),
            default_storage_policy: self.upload.default_storage_policy.clone(),
            max_bytes: self.upload.max_bytes,
            thumbnail_sizes: self.upload.thumbnail_sizes.clone(),
            enable_avif: self.upload.enable_avif,
            enable_webp: self.upload.enable_webp,
            quota: self.upload.quota.clone(),
            authz: (*authz).clone(),
        });
        self
    }

    /// Attaches the ACL grant repository backing the `/acl` endpoints.
    #[must_use]
    pub fn with_acl_repo(mut self, repo: Arc<dyn ResourceAclRepository>) -> Self {
        self.acl_repo = Some(repo);
        self
    }

    /// Attaches a user repository so the login handler can verify credentials.
    #[must_use]
    pub fn with_user_repo(mut self, repo: Arc<dyn UserRepository>) -> Self {
        self.user_repo = Some(repo);
        self
    }

    /// Attaches an image repository so image/list/link handlers can read metadata.
    #[must_use]
    pub fn with_image_repo(mut self, repo: Arc<dyn ImageRepository>) -> Self {
        self.image_repo = Some(repo);
        self
    }

    /// Attaches a team repository so team handlers can read metadata.
    #[must_use]
    pub fn with_team_repo(mut self, repo: Arc<dyn TeamRepository>) -> Self {
        self.team_repo = Some(repo);
        self
    }

    /// Sets the public base URL used to build absolute image links.
    #[must_use]
    pub fn with_public_url_base(mut self, base: String) -> Self {
        self.public_url_base = Some(base);
        self
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
                quota: self.upload.quota.clone(),
                authz: self.upload.authz.clone(),
            });
        }
        self
    }

    /// Attaches OIDC provider configuration and the admin-email allowlist.
    #[must_use]
    pub fn with_oidc(
        mut self,
        providers: HashMap<String, OidcProviderConfig>,
        admin_emails: Vec<String>,
        secure_cookies: bool,
    ) -> Self {
        self.oidc_providers = Arc::new(providers);
        self.oidc_admin_emails = Arc::new(admin_emails.into_iter().collect());
        self.cookie_secure = secure_cookies;
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

    fn s3_credentials(&self) -> Option<picroom_s3compat::S3Credential> {
        self.s3_credentials.clone()
    }

    fn expected_bucket(&self) -> Option<String> {
        self.s3_bucket.clone()
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
        prefix: Option<&picroom_domain::StorageKey>,
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

#[cfg(test)]
mod tests {
    use super::*;
    use picroom_audit::NoopAuditSink;
    use picroom_storage::driver::LocalDriver;
    use std::collections::HashMap;

    fn tmpdir() -> std::path::PathBuf {
        let p = std::env::temp_dir().join(format!("picroom-state-{}", uuid::Uuid::now_v7()));
        std::fs::create_dir_all(&p).unwrap();
        p
    }

    #[test]
    fn for_dev_defaults_are_dev_mode() {
        let storage = Arc::new(LocalDriver::new(tmpdir(), "https://cdn.example.com/i"));
        let audit = Arc::new(NoopAuditSink);
        let state = AppState::for_dev(storage, audit);
        // In dev mode every DB-backed capability is absent.
        assert!(state.image_repo.is_none());
        assert!(state.user_repo.is_none());
        assert!(state.team_repo.is_none());
        assert!(state.audit_reader.is_none());
        assert!(state.delete_service.is_none());
        assert!(state.s3_credentials.is_none());
        assert!(state.public_url_base.is_none());
        assert!(state.oidc_providers.as_ref().is_empty());
        #[allow(clippy::assert_is_empty)] // HashMap has no convenient empty literal here
        let _ = state.oidc_providers.is_empty();
        assert!(state.oidc_admin_emails.as_ref().is_empty());
        assert!(!state.cookie_secure);
    }

    #[test]
    fn with_public_url_base_sets_field() {
        let storage = Arc::new(LocalDriver::new(tmpdir(), "https://cdn.example.com/i"));
        let audit = Arc::new(NoopAuditSink);
        let state = AppState::for_dev(storage, audit)
            .with_public_url_base("https://cdn.example.com".to_string());
        assert_eq!(
            state.public_url_base.as_deref(),
            Some("https://cdn.example.com")
        );
    }

    #[test]
    fn with_oidc_sets_providers_and_admin_emails() {
        let storage = Arc::new(LocalDriver::new(tmpdir(), "https://cdn.example.com/i"));
        let audit = Arc::new(NoopAuditSink);
        let mut providers = HashMap::new();
        providers.insert(
            "google".to_string(),
            picroom_infra::config::OidcProviderConfig {
                issuer: "https://accounts.google.com".to_string(),
                client_id: "cid".to_string(),
                client_secret: "sec".to_string(),
                redirect_uri: "https://app/callback".to_string(),
                scopes: vec![],
                insecure_skip_verify: false,
            },
        );
        let state = AppState::for_dev(storage, audit).with_oidc(
            providers,
            vec!["admin@example.com".to_string()],
            true,
        );
        assert!(state.oidc_providers.contains_key("google"));
        assert_eq!(state.oidc_admin_emails.len(), 1);
        assert!(state.cookie_secure);
    }

    #[test]
    fn with_oidc_empty_is_fine() {
        let storage = Arc::new(LocalDriver::new(tmpdir(), "https://cdn.example.com/i"));
        let audit = Arc::new(NoopAuditSink);
        let state = AppState::for_dev(storage, audit).with_oidc(
            HashMap::new(),
            vec!["admin@example.com".to_string()],
            true,
        );
        assert!(state.oidc_providers.as_ref().is_empty());
        #[allow(clippy::assert_is_empty)] // HashMap has no convenient empty literal here
        let _ = state.oidc_providers.is_empty();
        assert_eq!(state.oidc_admin_emails.len(), 1);
        assert!(state.cookie_secure);
    }

    #[test]
    fn with_optional_job_queue_none_is_noop() {
        let storage = Arc::new(LocalDriver::new(tmpdir(), "https://cdn.example.com/i"));
        let audit = Arc::new(NoopAuditSink);
        let state = AppState::for_dev(storage, audit).with_optional_job_queue(None);
        assert!(state.upload.job_queue.is_none());
    }

    #[test]
    fn jwt_provider_returns_jwt_service_for_both_types() {
        let storage = Arc::new(LocalDriver::new(tmpdir(), "https://cdn.example.com/i"));
        let audit = Arc::new(NoopAuditSink);
        let state = AppState::for_dev(storage, audit);
        let _svc = state.jwt_service();
        let arc = Arc::new(state);
        let _svc2 = arc.jwt_service();
    }
}
