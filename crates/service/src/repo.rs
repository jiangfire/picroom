// SPDX-License-Identifier: MIT
// Copyright (c) 2026 Picroom Contributors

//! Image repository — DB-backed persistence for `Image` entities.
//!
//! Trait + Postgres implementation. `SQLite` fallback lives in
//! [`SqliteImageRepository`] (post-MVP).

use crate::ServiceError;
use async_trait::async_trait;
use picroom_domain::{Image, ImageId, Page, PageReq, Team, TeamId, UserId};
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
    /// Liveness probe — runs a cheap `SELECT 1`.
    async fn ping(&self) -> Result<(), ServiceError>;
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
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, 'pending', $11, $11)
            ",
        )
        .bind(image.id.as_uuid())
        .bind(image.owner_id.as_uuid())
        .bind(image.team_id.as_ref().map(TeamId::as_uuid))
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
            SELECT id, owner_id, team_id, storage_policy, storage_key, content_type,
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
            SELECT id, owner_id, team_id, storage_policy, storage_key, content_type,
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

    async fn ping(&self) -> Result<(), ServiceError> {
        sqlx::query("SELECT 1")
            .execute(&self.pool)
            .await
            .map_err(|e| ServiceError::Internal(format!("db ping: {e}")))?;
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
    /// Owning team id (may be null).
    pub team_id: Option<Uuid>,
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
            team_id: r.team_id.map(TeamId),
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
// User repository (PG) — credential lookup for login
// ---------------------------------------------------------------------------

/// Credentials needed to verify a login attempt.
///
/// Returned by [`UserRepository::find_by_email`]. Intentionally minimal: only
/// the fields required to authenticate and issue a token.
#[derive(Debug, Clone)]
pub struct UserCredentials {
    /// Stable user id (becomes the JWT `sub`).
    pub id: UserId,
    /// Global role name (e.g. `"admin"`).
    pub role: String,
    /// Argon2id password hash.
    pub password_hash: String,
    /// Whether the account is soft-disabled.
    pub disabled: bool,
}

/// Repository for user authentication data.
#[async_trait]
pub trait UserRepository: Send + Sync {
    /// Looks up credentials by email. `Ok(None)` means "no such user".
    async fn find_by_email(&self, email: &str) -> Result<Option<UserCredentials>, ServiceError>;
}

/// PostgreSQL-backed user repository.
#[derive(Debug, Clone)]
pub struct PgUserRepository {
    pool: PgPool,
}

impl PgUserRepository {
    /// Creates a new repository bound to the given pool.
    pub const fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl UserRepository for PgUserRepository {
    async fn find_by_email(&self, email: &str) -> Result<Option<UserCredentials>, ServiceError> {
        let row: Option<(Uuid, String, String, bool)> =
            sqlx::query_as(r"SELECT id, role, password_hash, disabled FROM users WHERE email = $1")
                .bind(email)
                .fetch_optional(&self.pool)
                .await
                .map_err(|e| ServiceError::Internal(format!("find user: {e}")))?;
        Ok(
            row.map(|(id, role, password_hash, disabled)| UserCredentials {
                id: UserId(id),
                role,
                password_hash,
                disabled,
            }),
        )
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

// ---------------------------------------------------------------------------
// Team repository (PG)
// ---------------------------------------------------------------------------

/// Repository for team metadata.
#[async_trait]
pub trait TeamRepository: Send + Sync {
    /// Creates a team.
    async fn create(&self, team: &Team) -> Result<(), ServiceError>;
    /// Fetches a team by id.
    async fn get(&self, id: TeamId) -> Result<Team, ServiceError>;
    /// Lists all teams (newest first).
    async fn list(&self) -> Result<Vec<Team>, ServiceError>;
    /// Adds or updates a team membership.
    async fn add_member(
        &self,
        team_id: TeamId,
        user_id: UserId,
        role: &str,
    ) -> Result<(), ServiceError>;
}

/// PostgreSQL-backed team repository.
#[derive(Debug, Clone)]
pub struct PgTeamRepository {
    pool: PgPool,
}

impl PgTeamRepository {
    /// Creates a new repository bound to the given pool.
    pub const fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

/// Row projection shared by `get`/`list`.
type TeamRow = (
    Uuid,
    String,
    String,
    Option<String>,
    Option<String>,
    OffsetDateTime,
);

#[async_trait]
impl TeamRepository for PgTeamRepository {
    async fn create(&self, team: &Team) -> Result<(), ServiceError> {
        sqlx::query(
            r"INSERT INTO teams (id, name, slug, description, storage_policy, created_at, updated_at)
              VALUES ($1, $2, $3, $4, $5, NOW(), NOW())",
        )
        .bind(team.id.as_uuid())
        .bind(&team.name)
        .bind(&team.slug)
        .bind(&team.description)
        .bind(&team.storage_policy)
        .execute(&self.pool)
        .await
        .map_err(|e| ServiceError::Internal(format!("create team: {e}")))?;
        Ok(())
    }

    async fn get(&self, id: TeamId) -> Result<Team, ServiceError> {
        let row: Option<TeamRow> = sqlx::query_as::<_, TeamRow>(
            r"SELECT id, name, slug, description, storage_policy, created_at FROM teams WHERE id = $1",
        )
        .bind(id.as_uuid())
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| ServiceError::Internal(format!("get team: {e}")))?;
        match row {
            Some((id, name, slug, description, storage_policy, created_at)) => Ok(Team {
                id: TeamId(id),
                name,
                slug,
                description,
                storage_policy,
                created_at,
            }),
            None => Err(picroom_domain::DomainError::NotFound.into()),
        }
    }

    async fn list(&self) -> Result<Vec<Team>, ServiceError> {
        let rows: Vec<TeamRow> = sqlx::query_as::<_, TeamRow>(
            r"SELECT id, name, slug, description, storage_policy, created_at FROM teams ORDER BY created_at DESC",
        )
        .fetch_all(&self.pool)
        .await
        .map_err(|e| ServiceError::Internal(format!("list teams: {e}")))?;
        Ok(rows
            .into_iter()
            .map(
                |(id, name, slug, description, storage_policy, created_at)| Team {
                    id: TeamId(id),
                    name,
                    slug,
                    description,
                    storage_policy,
                    created_at,
                },
            )
            .collect())
    }

    async fn add_member(
        &self,
        team_id: TeamId,
        user_id: UserId,
        role: &str,
    ) -> Result<(), ServiceError> {
        sqlx::query(
            r"INSERT INTO team_members (team_id, user_id, role, joined_at)
              VALUES ($1, $2, $3, NOW())
              ON CONFLICT (team_id, user_id) DO UPDATE SET role = EXCLUDED.role",
        )
        .bind(team_id.as_uuid())
        .bind(user_id.as_uuid())
        .bind(role)
        .execute(&self.pool)
        .await
        .map_err(|e| ServiceError::Internal(format!("add member: {e}")))?;
        Ok(())
    }
}
