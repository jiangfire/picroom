# Temporary patch: service-layer enforcement in upload.rs and delete.rs.
p = 'crates/service/src/upload.rs'
src = open(p, encoding='utf-8').read()

src = src.replace(
    'use crate::QuotaService;\nuse crate::ServiceError;',
    'use crate::authz::AuthzService;\nuse crate::QuotaService;\nuse crate::ServiceError;',
)
src = src.replace(
    'use picroom_domain::{DomainError, Image, ImageId, StorageKey, UserId};',
    'use picroom_auth::Actor;\nuse picroom_domain::{DomainError, Image, ImageId, StorageKey, TeamId};',
)

old = '''    /// Quota service used to enforce per-user byte caps.
    pub quota: QuotaService,
}'''
new = '''    /// Quota service used to enforce per-user byte caps.
    pub quota: QuotaService,
    /// Authorization coordinator. `None` skips enforcement (dev mode); the
    /// binary wiring always sets it.
    pub authz: Option<Arc<AuthzService>>,
}'''
assert old in src
src = src.replace(old, new)

old = '''            enable_webp: true,
            quota: QuotaService::new(),
        }
    }'''
new = '''            enable_webp: true,
            quota: QuotaService::new(),
            authz: None,
        }
    }

    /// Sets the authorization coordinator used to enforce `Image/Create`.
    pub fn with_authz(mut self, authz: Arc<AuthzService>) -> Self {
        self.authz = Some(authz);
        self
    }'''
assert old in src
src = src.replace(old, new)

# stage(): actor + team_id, enforcement, server-side team attribution, typed probe error
old = '''    /// Validates, probes, persists, and records audit — everything up to and
    /// including the storage write, but **no job enqueue**.
    ///
    /// Callers persist the returned `Image` via the repository and only then
    /// call [`Self::enqueue_variants`]. Enqueueing any earlier lets a fast
    /// worker claim a job whose `images` row does not exist yet, dead-lettering
    /// a perfectly valid upload (R-04).
    #[allow(clippy::too_many_lines)]
    pub async fn stage(
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
            team_id: None,'''
new = '''    /// Validates, probes, persists, and records audit — everything up to and
    /// including the storage write, but **no job enqueue**.
    ///
    /// Enforces `Image/Create` for the [`Actor`] before any bytes are stored:
    /// a team-scoped upload requires `Image/Create` within that team (i.e.
    /// team `uploader` and above); a personal upload requires the global
    /// permission. The `team_id` is attributed server-side — callers cannot
    /// bind an arbitrary team onto the row (R-13).
    ///
    /// Callers persist the returned `Image` via the repository and only then
    /// call [`Self::enqueue_variants`]. Enqueueing any earlier lets a fast
    /// worker claim a job whose `images` row does not exist yet, dead-lettering
    /// a perfectly valid upload (R-04).
    #[allow(clippy::too_many_lines)]
    pub async fn stage(
        &self,
        actor: &Actor,
        team_id: Option<TeamId>,
        content_type: &str,
        bytes: Bytes,
    ) -> Result<Image, ServiceError> {
        // 0. Authorization — before quota, probe, or storage.
        if let Some(authz) = &self.authz {
            authz
                .authorize(
                    actor,
                    &picroom_auth::Resource::new(
                        picroom_domain::permission::ResourceType::Image,
                        Uuid::nil(), // not yet created; team scope is what matters
                        None,
                        team_id.map(|t| t.as_uuid()),
                    ),
                    picroom_auth::PermissionAction::Create,
                )
                .await?;
        }

        let owner_id = UserId(actor.user_id);

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

        // 3. Probe (populate width/height/mime). Decoder errors carry parser
        // internals — log the cause, return a generic validation error (R-19).
        let mut ctx = PipelineContext::default();
        if let Err(e) = probe_into(&mut ctx, bytes.clone()).await {
            tracing::warn!(error = %e, "upload probe failed");
            return Err(DomainError::Validation(
                "unsupported or corrupt image".into(),
            )
            .into());
        }

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
            team_id,'''
assert old in src
src = src.replace(old, new)

# legacy upload wrapper keeps old callers working
old = '''    /// Validates, probes, persists, records audit, and enqueues variant jobs.
    ///
    /// Convenience for callers that have no repository step between staging
    /// and enqueueing. Prefer [`Self::stage`] + [`Self::enqueue_variants`]
    /// when a DB row must be inserted in between (the HTTP upload path does).
    pub async fn upload(
        &self,
        owner_id: UserId,
        content_type: &str,
        bytes: Bytes,
    ) -> Result<Image, ServiceError> {
        let image = self.stage(owner_id, content_type, bytes).await?;
        self.enqueue_variants(&image).await;
        Ok(image)
    }'''
new = '''    /// Validates, probes, persists, records audit, and enqueues variant jobs.
    ///
    /// Convenience for callers that have no repository step between staging
    /// and enqueueing. Prefer [`Self::stage`] + [`Self::enqueue_variants`]
    /// when a DB row must be inserted in between (the HTTP upload path does).
    pub async fn upload(
        &self,
        actor: &Actor,
        content_type: &str,
        bytes: Bytes,
    ) -> Result<Image, ServiceError> {
        let image = self.stage(actor, None, content_type, bytes).await?;
        self.enqueue_variants(&image).await;
        Ok(image)
    }'''
assert old in src
src = src.replace(old, new)
open(p, 'w', encoding='utf-8', newline='\n').write(src)

# ---- delete.rs ----
p = 'crates/service/src/delete.rs'
src = open(p, encoding='utf-8').read()
src = src.replace(
    'use crate::repo::ImageRepository;\nuse crate::ServiceError;',
    'use crate::authz::AuthzService;\nuse crate::repo::ImageRepository;\nuse crate::ServiceError;',
)
src = src.replace(
    'use picroom_domain::Image;',
    'use picroom_auth::Actor;\nuse picroom_domain::Image;',
)
old = '''/// Delete service — unified image deletion (storage + DB + audit).
#[derive(Clone)]
pub struct DeleteService {
    storage: Arc<dyn StorageWriter + Send + Sync>,
    repo: Arc<dyn ImageRepository>,
    audit: Arc<dyn AuditSink>,
}'''
new = '''/// Delete service — unified image deletion (storage + DB + audit).
#[derive(Clone)]
pub struct DeleteService {
    storage: Arc<dyn StorageWriter + Send + Sync>,
    repo: Arc<dyn ImageRepository>,
    audit: Arc<dyn AuditSink>,
    authz: Option<Arc<AuthzService>>,
}'''
assert old in src
src = src.replace(old, new)

old = '''        Self {
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
    pub async fn delete(&self, image: Image) -> Result<(), ServiceError> {'''
new = '''        Self {
            storage,
            repo,
            audit,
            authz: None,
        }
    }

    /// Sets the authorization coordinator. When set, deletion is denied
    /// unless the actor owns the image, holds `Image/Delete` (manager/admin),
    /// or is allowed by the image's ACL — evaluated in the spec §10.3 order.
    pub fn with_authz(mut self, authz: Arc<AuthzService>) -> Self {
        self.authz = Some(authz);
        self
    }

    /// Deletes an image (DB row + storage object) and emits an audit event.
    ///
    /// Takes the already-resolved [`Image`] so callers avoid a redundant
    /// lookup. Enforcement happens here, in the service layer, so every path
    /// that reaches deletion is authorized (D-7) — not just the HTTP route.
    pub async fn delete(&self, actor: &Actor, image: Image) -> Result<(), ServiceError> {
        if let Some(authz) = &self.authz {
            authz
                .authorize(
                    actor,
                    &picroom_auth::Resource::new(
                        picroom_domain::permission::ResourceType::Image,
                        image.id.as_uuid(),
                        Some(image.owner_id.as_uuid()),
                        image.team_id.map(|t| t.as_uuid()),
                    ),
                    picroom_auth::PermissionAction::Delete,
                )
                .await?;
        }'''
assert old in src
src = src.replace(old, new)

# fix test call
old = '''        let svc = DeleteService::new(storage.clone(), repo.clone(), audit.clone());

        svc.delete(fake_image()).await.unwrap();'''
new = '''        let svc = DeleteService::new(storage.clone(), repo.clone(), audit.clone());
        let owner = uuid::Uuid::now_v7();
        let mut img = fake_image();
        img.owner_id = picroom_domain::UserId(owner);

        svc.delete(&picroom_auth::Actor::with_roles(owner, vec![]), img)
            .await
            .unwrap();'''
assert old in src
src = src.replace(old, new)
open(p, 'w', encoding='utf-8', newline='\n').write(src)
print("ok")
