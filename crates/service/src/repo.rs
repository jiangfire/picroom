//! Image repository — DB-backed persistence for `Image` entities.
//!
//! Trait + Postgres implementation. `SQLite` fallback lives in
//! [`SqliteImageRepository`] (post-MVP).

use crate::ServiceError;
use async_trait::async_trait;
use picroom_domain::{Image, ImageId, Page, PageReq};
use sqlx::PgPool;
use time::OffsetDateTime;
use uuid::Uuid;

/// Repository for image metadata.
#[async_trait]
pub trait ImageRepository: Send + Sync {
    /// Inserts a new image.
    async fn insert(&self, image: &Image) -> Result<(), ServiceError>;
    /// Fetches an image by id.
    async fn get(&self, id: ImageId) -> Result<Image, ServiceError>;
    /// Lists images for a given owner.
    async fn list_for_owner(
        &self,
        owner_id: Uuid,
        page: PageReq,
    ) -> Result<Page<Image>, ServiceError>;
    /// Deletes an image by id.
    async fn delete(&self, id: ImageId) -> Result<(), ServiceError>;
}

/// PostgreSQL-backed image repository.
#[derive(Debug, Clone)]
pub struct PgImageRepository {
    pool: PgPool,
}

impl PgImageRepository {
    /// Creates a new repository bound to the given pool.
    pub const fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl ImageRepository for PgImageRepository {
    async fn insert(&self, image: &Image) -> Result<(), ServiceError> {
        sqlx::query(
            r"
            INSERT INTO images (
                id, owner_id, team_id, storage_policy, storage_key,
                content_type, bytes, width, height, sha256, status,
                created_at, updated_at
            )
            VALUES ($1, $2, NULL, $3, $4, $5, $6, $7, $8, $9, 'pending', $10, $10)
            ",
        )
        .bind(image.id.as_uuid())
        .bind(image.owner_id.as_uuid())
        .bind("default")
        .bind(image.key.as_str())
        .bind(&image.content_type)
        .bind(image.bytes as i64)
        .bind(image.width as i32)
        .bind(image.height as i32)
        .bind(image.sha256.as_deref())
        .bind(image.created_at)
        .execute(&self.pool)
        .await
        .map_err(|e| ServiceError::Internal(format!("insert image: {e}")))?;
        Ok(())
    }

    async fn get(&self, id: ImageId) -> Result<Image, ServiceError> {
        let row: Option<ImageRow> = sqlx::query_as::<_, ImageRow>(
            r"
            SELECT id, owner_id, storage_policy, storage_key, content_type,
                   bytes, width, height, sha256, status, created_at
            FROM images
            WHERE id = $1 AND status != 'deleted'
            ",
        )
        .bind(id.as_uuid())
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| ServiceError::Internal(format!("get image: {e}")))?;

        match row {
            Some(r) => r.try_into(),
            None => Err(picroom_domain::DomainError::NotFound.into()),
        }
    }

    async fn list_for_owner(
        &self,
        owner_id: Uuid,
        page: PageReq,
    ) -> Result<Page<Image>, ServiceError> {
        let limit = i64::from(page.limit.clamp(1, 200));
        let rows: Vec<ImageRow> = sqlx::query_as::<_, ImageRow>(
            r"
            SELECT id, owner_id, storage_policy, storage_key, content_type,
                   bytes, width, height, sha256, status, created_at
            FROM images
            WHERE owner_id = $1 AND status != 'deleted'
            ORDER BY created_at DESC
            LIMIT $2
            ",
        )
        .bind(owner_id)
        .bind(limit)
        .fetch_all(&self.pool)
        .await
        .map_err(|e| ServiceError::Internal(format!("list images: {e}")))?;

        let images: Vec<Image> = rows
            .into_iter()
            .map(std::convert::TryInto::try_into)
            .collect::<Result<_, _>>()?;

        Ok(Page::new(images, None, page))
    }

    async fn delete(&self, id: ImageId) -> Result<(), ServiceError> {
        sqlx::query(r"UPDATE images SET status = 'deleted', updated_at = NOW() WHERE id = $1")
            .bind(id.as_uuid())
            .execute(&self.pool)
            .await
            .map_err(|e| ServiceError::Internal(format!("delete image: {e}")))?;
        Ok(())
    }
}

/// Row representation matching `images` table columns.
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct ImageRow {
    /// Image id.
    pub id: Uuid,
    /// Owner user id.
    pub owner_id: Uuid,
    /// Storage policy name.
    pub storage_policy: String,
    /// Storage key.
    pub storage_key: String,
    /// Content type.
    pub content_type: String,
    /// Size in bytes.
    pub bytes: i64,
    /// Width.
    pub width: i32,
    /// Height.
    pub height: i32,
    /// Hex SHA-256, if available.
    pub sha256: Option<String>,
    /// Row status (pending/ready/failed/deleted).
    pub status: String,
    /// Creation timestamp.
    pub created_at: OffsetDateTime,
}

impl TryFrom<ImageRow> for Image {
    type Error = ServiceError;
    fn try_from(r: ImageRow) -> Result<Self, Self::Error> {
        let key = picroom_domain::StorageKey::parse(&r.storage_key)
            .map_err(|e| ServiceError::Internal(format!("invalid storage_key: {e}")))?;
        Ok(Self {
            id: picroom_domain::ImageId(r.id),
            owner_id: picroom_domain::UserId(r.owner_id),
            key,
            content_type: r.content_type,
            bytes: r.bytes as u64,
            width: r.width as u32,
            height: r.height as u32,
            sha256: r.sha256,
            variants: vec![],
            created_at: r.created_at,
        })
    }
}

// ---------------------------------------------------------------------------
// Variant repository (PG)
// ---------------------------------------------------------------------------

/// PostgreSQL-backed variant repository.
#[derive(Debug, Clone)]
pub struct PgVariantRepository {
    pool: PgPool,
}

impl PgVariantRepository {
    /// Creates a new variant repository.
    pub const fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl picroom_worker::processor::VariantRepository for PgVariantRepository {
    async fn insert_variant(
        &self,
        image_id: picroom_domain::ImageId,
        kind: &str,
        size: Option<u32>,
        storage_key: &str,
        bytes: u64,
        content_type: &str,
    ) -> Result<(), String> {
        sqlx::query(
            r"
            INSERT INTO image_variants (id, image_id, kind, size, storage_key, bytes, content_type, created_at)
            VALUES ($1, $2, $3, $4, $5, $6, $7, NOW())
            ON CONFLICT (image_id, kind, size) DO UPDATE
              SET storage_key = EXCLUDED.storage_key,
                  bytes = EXCLUDED.bytes,
                  content_type = EXCLUDED.content_type
            ",
        )
        .bind(Uuid::now_v7())
        .bind(image_id.as_uuid())
        .bind(kind)
        .bind(size.map(|s| s as i32))
        .bind(storage_key)
        .bind(bytes as i64)
        .bind(content_type)
        .execute(&self.pool)
        .await
        .map_err(|e| format!("insert variant: {e}"))?;
        Ok(())
    }
}
