// SPDX-License-Identifier: MIT
// Copyright (c) 2026 Picroom Contributors

//! Image repository — DB-backed persistence for `Image` entities.
//!
//! Trait + Postgres implementation. `SQLite` fallback lives in
//! [`SqliteImageRepository`] (post-MVP).

use crate::ServiceError;
use async_trait::async_trait;
use picroom_domain::{
    Image, ImageId, NewOidcUser, NewUser, Page, PageReq, Team, TeamId, TeamMember, User, UserId,
};
use sqlx::{PgPool, SqlitePool};
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
        // Decode the composite cursor (`created_at|id`) if supplied.
        let (cursor_ts, cursor_id): (Option<OffsetDateTime>, Option<Uuid>) = match &page.cursor {
            Some(c) => {
                let parts: Vec<&str> = c.splitn(2, '|').collect();
                if parts.len() == 2 {
                    let ts = OffsetDateTime::parse(
                        parts[0],
                        &time::format_description::well_known::Rfc3339,
                    )
                    .map_err(|e| ServiceError::Internal(format!("invalid cursor: {e}")))?;
                    let id = Uuid::parse_str(parts[1])
                        .map_err(|e| ServiceError::Internal(format!("invalid cursor id: {e}")))?;
                    (Some(ts), Some(id))
                } else {
                    (None, None)
                }
            }
            None => (None, None),
        };
        // Fetch one extra row so we can tell whether another page follows.
        let rows: Vec<ImageRow> = sqlx::query_as::<_, ImageRow>(
            r"
            SELECT id, owner_id, team_id, storage_policy, storage_key, content_type,
                   bytes, width, height, sha256, status, created_at
            FROM images
            WHERE owner_id = $1 AND status != 'deleted'
              AND ($2::timestamptz IS NULL OR (created_at, id) < ($2, $3::uuid))
            ORDER BY created_at DESC, id DESC
            LIMIT $4
            ",
        )
        .bind(owner_id)
        .bind(cursor_ts)
        .bind(cursor_id)
        .bind(limit + 1)
        .fetch_all(&self.pool)
        .await
        .map_err(|e| ServiceError::Internal(format!("list images: {e}")))?;

        let has_more = rows.len() as i64 > limit;
        let page_rows: &[ImageRow] = if has_more {
            &rows[..limit as usize]
        } else {
            &rows[..]
        };
        let images: Vec<Image> = page_rows
            .iter()
            .cloned()
            .map(std::convert::TryInto::try_into)
            .collect::<Result<_, _>>()?;
        // Cursor for the next page encodes the last row's `(created_at, id)`
        // so pagination is stable even when many rows share a timestamp.
        let next_cursor = if has_more {
            page_rows
                .last()
                .map(|r| {
                    Ok::<String, ServiceError>(format!(
                        "{}|{}",
                        r.created_at
                            .format(&time::format_description::well_known::Rfc3339)
                            .map_err(|e| ServiceError::Internal(format!("format cursor: {e}")))?,
                        r.id
                    ))
                })
                .transpose()?
        } else {
            None
        };
        Ok(Page::new(images, next_cursor, page))
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
    /// Creates a user from the given request.
    async fn create_user(&self, new: &NewUser) -> Result<User, ServiceError>;
    /// Updates a user's role.
    async fn set_role(&self, user_id: UserId, role: &str) -> Result<(), ServiceError>;
    /// Lists users (newest first), cursor-paginated on `(created_at, id)`.
    async fn list(&self, page: PageReq) -> Result<Page<User>, ServiceError>;
    /// Looks up a user by id. `Ok(None)` means "no such user".
    async fn find_by_id(&self, id: UserId) -> Result<Option<User>, ServiceError>;
    /// Sets the user's disabled flag.
    async fn set_disabled(&self, id: UserId, disabled: bool) -> Result<(), ServiceError>;
    /// Looks up a user by their external OIDC identity (`provider` + `subject`).
    /// `Ok(None)` means no local account is linked to that identity.
    async fn find_by_external(
        &self,
        provider: &str,
        subject: &str,
    ) -> Result<Option<User>, ServiceError>;
    /// Creates a user authenticated via OIDC (no local password) and records
    /// the `oidc_links` row binding them to `provider` + `subject`.
    async fn create_oidc_user(&self, new: &NewOidcUser) -> Result<User, ServiceError>;
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

    async fn create_user(&self, new: &NewUser) -> Result<User, ServiceError> {
        let id = Uuid::now_v7();
        let row: (Uuid, String, String, bool, OffsetDateTime) = sqlx::query_as(
            r"INSERT INTO users (id, email, name, password_hash, role, created_at, updated_at)
              VALUES ($1, $2, $3, $4, $5, NOW(), NOW())
              RETURNING id, email, name, disabled, created_at",
        )
        .bind(id)
        .bind(&new.email)
        .bind(&new.name)
        .bind(&new.password_hash)
        .bind(&new.role)
        .fetch_one(&self.pool)
        .await
        .map_err(|e| ServiceError::Internal(format!("create user: {e}")))?;
        Ok(User {
            id: UserId(row.0),
            email: row.1,
            name: row.2,
            avatar_url: None,
            role: new.role.clone(),
            created_at: row.4,
            disabled: row.3,
        })
    }

    async fn set_role(&self, user_id: UserId, role: &str) -> Result<(), ServiceError> {
        sqlx::query(r"UPDATE users SET role = $2, updated_at = NOW() WHERE id = $1")
            .bind(user_id.as_uuid())
            .bind(role)
            .execute(&self.pool)
            .await
            .map_err(|e| ServiceError::Internal(format!("set role: {e}")))?;
        Ok(())
    }

    async fn list(&self, page: PageReq) -> Result<Page<User>, ServiceError> {
        let limit = i64::from(page.limit.clamp(1, 200));
        let (cursor_ts, cursor_id) = parse_composite_cursor(page.cursor.as_deref())?;
        // Fetch one extra row to detect whether another page follows.
        let rows: Vec<UserRow> = sqlx::query_as::<_, UserRow>(
            r"SELECT id, email, name, role, disabled, created_at FROM users
              WHERE ($1::timestamptz IS NULL OR (created_at, id) < ($1, $2::uuid))
              ORDER BY created_at DESC, id DESC
              LIMIT $3",
        )
        .bind(cursor_ts)
        .bind(cursor_id)
        .bind(limit + 1)
        .fetch_all(&self.pool)
        .await
        .map_err(|e| ServiceError::Internal(format!("list users: {e}")))?;

        let has_more = rows.len() as i64 > limit;
        let page_rows: &[UserRow] = if has_more {
            &rows[..limit as usize]
        } else {
            &rows[..]
        };
        let users: Vec<User> = page_rows.iter().map(user_from_row).collect();
        let next_cursor = if has_more {
            page_rows
                .last()
                .map(|r| format_cursor(r.created_at, r.id))
                .transpose()?
        } else {
            None
        };
        Ok(Page::new(users, next_cursor, page))
    }

    async fn find_by_id(&self, id: UserId) -> Result<Option<User>, ServiceError> {
        let row: Option<UserRow> = sqlx::query_as::<_, UserRow>(
            r"SELECT id, email, name, role, disabled, created_at FROM users WHERE id = $1",
        )
        .bind(id.as_uuid())
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| ServiceError::Internal(format!("find user by id: {e}")))?;
        Ok(row.map(|r| user_from_row(&r)))
    }

    async fn set_disabled(&self, id: UserId, disabled: bool) -> Result<(), ServiceError> {
        sqlx::query(r"UPDATE users SET disabled = $2, updated_at = NOW() WHERE id = $1")
            .bind(id.as_uuid())
            .bind(disabled)
            .execute(&self.pool)
            .await
            .map_err(|e| ServiceError::Internal(format!("set disabled: {e}")))?;
        Ok(())
    }

    async fn find_by_external(
        &self,
        provider: &str,
        subject: &str,
    ) -> Result<Option<User>, ServiceError> {
        let user_id: Option<Uuid> = sqlx::query_scalar(
            r"SELECT user_id FROM oidc_links WHERE provider = $1 AND subject = $2",
        )
        .bind(provider)
        .bind(subject)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| ServiceError::Internal(format!("find oidc link: {e}")))?;
        match user_id {
            Some(id) => self.find_by_id(UserId(id)).await,
            None => Ok(None),
        }
    }

    async fn create_oidc_user(&self, new: &NewOidcUser) -> Result<User, ServiceError> {
        let mut tx = self
            .pool
            .begin()
            .await
            .map_err(|e| ServiceError::Internal(format!("begin tx: {e}")))?;
        let id = Uuid::now_v7();
        let row: (Uuid, String, String, bool, OffsetDateTime) = sqlx::query_as(
            r"INSERT INTO users (id, email, name, password_hash, role, created_at, updated_at)
              VALUES ($1, $2, $3, NULL, $4, NOW(), NOW())
              RETURNING id, email, name, disabled, created_at",
        )
        .bind(id)
        .bind(&new.email)
        .bind(&new.name)
        .bind(&new.role)
        .fetch_one(&mut *tx)
        .await
        .map_err(|e| ServiceError::Internal(format!("create oidc user: {e}")))?;
        sqlx::query(r"INSERT INTO oidc_links (user_id, provider, subject) VALUES ($1, $2, $3)")
            .bind(id)
            .bind(&new.provider)
            .bind(&new.subject)
            .execute(&mut *tx)
            .await
            .map_err(|e| ServiceError::Internal(format!("link oidc: {e}")))?;
        tx.commit()
            .await
            .map_err(|e| ServiceError::Internal(format!("commit tx: {e}")))?;
        Ok(User {
            id: UserId(row.0),
            email: row.1,
            name: row.2,
            avatar_url: None,
            role: new.role.clone(),
            created_at: row.4,
            disabled: row.3,
        })
    }
}

/// Row projection for the `users` table used by `list` / `find_by_id`.
#[derive(Debug, Clone, sqlx::FromRow)]
struct UserRow {
    /// User id.
    id: Uuid,
    /// Email.
    email: String,
    /// Display name.
    name: String,
    /// Global role.
    role: String,
    /// Disabled flag.
    disabled: bool,
    /// Creation timestamp.
    created_at: OffsetDateTime,
}

/// Maps a `UserRow` into a `User` domain entity (`avatar_url` is not persisted).
fn user_from_row(r: &UserRow) -> User {
    User {
        id: UserId(r.id),
        email: r.email.clone(),
        name: r.name.clone(),
        avatar_url: None,
        role: r.role.clone(),
        created_at: r.created_at,
        disabled: r.disabled,
    }
}

/// Decodes a composite `created_at|id` cursor into its parts.
fn parse_composite_cursor(
    cursor: Option<&str>,
) -> Result<(Option<OffsetDateTime>, Option<Uuid>), ServiceError> {
    let Some(c) = cursor else {
        return Ok((None, None));
    };
    let parts: Vec<&str> = c.splitn(2, '|').collect();
    if parts.len() != 2 {
        return Err(ServiceError::Internal(format!("invalid cursor: {c}")));
    }
    let ts = OffsetDateTime::parse(parts[0], &time::format_description::well_known::Rfc3339)
        .map_err(|e| ServiceError::Internal(format!("invalid cursor ts: {e}")))?;
    let id = Uuid::parse_str(parts[1])
        .map_err(|e| ServiceError::Internal(format!("invalid cursor id: {e}")))?;
    Ok((Some(ts), Some(id)))
}

/// Encodes a `(created_at, id)` pair into the opaque cursor string.
fn format_cursor(ts: OffsetDateTime, id: Uuid) -> Result<String, ServiceError> {
    let ts = ts
        .format(&time::format_description::well_known::Rfc3339)
        .map_err(|e| ServiceError::Internal(format!("format cursor: {e}")))?;
    Ok(format!("{ts}|{id}"))
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
            ON CONFLICT (image_id, kind, COALESCE(size, -1)) DO UPDATE
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
    /// Lists teams (newest first), paginated (R-25: the query used to be
    /// unbounded).
    async fn list(&self, page: PageReq) -> Result<Page<Team>, ServiceError>;
    /// Lists the teams the user is a member of (newest first), paginated.
    async fn list_for_user(
        &self,
        user_id: UserId,
        page: PageReq,
    ) -> Result<Page<Team>, ServiceError>;
    /// Adds or updates a team membership.
    async fn add_member(
        &self,
        team_id: TeamId,
        user_id: UserId,
        role: &str,
    ) -> Result<(), ServiceError>;
    /// Lists the members of a team (oldest join first), paginated.
    async fn list_members(
        &self,
        team_id: TeamId,
        page: PageReq,
    ) -> Result<Page<TeamMember>, ServiceError>;
    /// Returns the actor's role within a team, or `None` when not a member.
    async fn member_role(
        &self,
        team_id: TeamId,
        user_id: UserId,
    ) -> Result<Option<String>, ServiceError>;
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

    async fn list(&self, page: PageReq) -> Result<Page<Team>, ServiceError> {
        let limit = usize::try_from(page.limit).unwrap_or(50).clamp(1, 500);
        let cursor = parse_composite_cursor(page.cursor.as_deref())?;
        let rows: Vec<TeamRow> = sqlx::query_as::<_, TeamRow>(
            r"SELECT id, name, slug, description, storage_policy, created_at
              FROM teams
              WHERE ($1::timestamptz IS NULL OR (created_at, id) < ($1, $2))
              ORDER BY created_at DESC, id DESC
              LIMIT $3",
        )
        .bind(cursor.0)
        .bind(cursor.1)
        .bind(limit as i64 + 1)
        .fetch_all(&self.pool)
        .await
        .map_err(|e| ServiceError::Internal(format!("list teams: {e}")))?;
        let has_more = rows.len() > limit;
        let rows: Vec<TeamRow> = rows.into_iter().take(limit).collect();
        let next_cursor = match (has_more, rows.last()) {
            (true, Some(last)) => Some(format_cursor(last.5, last.0)?),
            _ => None,
        };
        let items: Vec<Team> = rows
            .iter()
            .map(
                |(id, name, slug, description, storage_policy, created_at)| Team {
                    id: TeamId(*id),
                    name: name.clone(),
                    slug: slug.clone(),
                    description: description.clone(),
                    storage_policy: storage_policy.clone(),
                    created_at: *created_at,
                },
            )
            .collect();
        Ok(Page::new(items, next_cursor, page))
    }

    async fn list_for_user(
        &self,
        user_id: UserId,
        page: PageReq,
    ) -> Result<Page<Team>, ServiceError> {
        let limit = usize::try_from(page.limit).unwrap_or(50).clamp(1, 500);
        let cursor = parse_composite_cursor(page.cursor.as_deref())?;
        let rows: Vec<TeamRow> = sqlx::query_as::<_, TeamRow>(
            r"SELECT t.id, t.name, t.slug, t.description, t.storage_policy, t.created_at
              FROM teams t
              JOIN team_members m ON m.team_id = t.id
              WHERE m.user_id = $1
                AND ($2::timestamptz IS NULL OR (t.created_at, t.id) < ($2, $3))
              ORDER BY t.created_at DESC, t.id DESC
              LIMIT $4",
        )
        .bind(user_id.as_uuid())
        .bind(cursor.0)
        .bind(cursor.1)
        .bind(limit as i64 + 1)
        .fetch_all(&self.pool)
        .await
        .map_err(|e| ServiceError::Internal(format!("list teams for user: {e}")))?;
        let has_more = rows.len() > limit;
        let rows: Vec<TeamRow> = rows.into_iter().take(limit).collect();
        let next_cursor = match (has_more, rows.last()) {
            (true, Some(last)) => Some(format_cursor(last.5, last.0)?),
            _ => None,
        };
        let items: Vec<Team> = rows
            .iter()
            .map(
                |(id, name, slug, description, storage_policy, created_at)| Team {
                    id: TeamId(*id),
                    name: name.clone(),
                    slug: slug.clone(),
                    description: description.clone(),
                    storage_policy: storage_policy.clone(),
                    created_at: *created_at,
                },
            )
            .collect();
        Ok(Page::new(items, next_cursor, page))
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

    async fn list_members(
        &self,
        team_id: TeamId,
        page: PageReq,
    ) -> Result<Page<TeamMember>, ServiceError> {
        // joined_at is not unique; page by offset within a clamped limit —
        // rosters are small, and the point is the hard ceiling (R-25).
        let limit = usize::try_from(page.limit).unwrap_or(50).clamp(1, 500);
        let offset: i64 = page
            .cursor
            .as_deref()
            .and_then(|c| c.parse().ok())
            .unwrap_or(0);
        let rows: Vec<(Uuid, Uuid, String, OffsetDateTime)> = sqlx::query_as(
            r"SELECT team_id, user_id, role, joined_at FROM team_members
              WHERE team_id = $1 ORDER BY joined_at ASC
              LIMIT $2 OFFSET $3",
        )
        .bind(team_id.as_uuid())
        .bind(limit as i64 + 1)
        .bind(offset)
        .fetch_all(&self.pool)
        .await
        .map_err(|e| ServiceError::Internal(format!("list members: {e}")))?;
        let has_more = rows.len() > limit;
        let items: Vec<TeamMember> = rows
            .into_iter()
            .take(limit)
            .map(|(tid, uid, role, joined_at)| TeamMember {
                team_id: TeamId(tid),
                user_id: UserId(uid),
                role,
                joined_at,
            })
            .collect();
        let next_cursor = if has_more {
            Some((offset + limit as i64).to_string())
        } else {
            None
        };
        Ok(Page::new(items, next_cursor, page))
    }

    async fn member_role(
        &self,
        team_id: TeamId,
        user_id: UserId,
    ) -> Result<Option<String>, ServiceError> {
        let row: Option<(String,)> =
            sqlx::query_as(r"SELECT role FROM team_members WHERE team_id = $1 AND user_id = $2")
                .bind(team_id.as_uuid())
                .bind(user_id.as_uuid())
                .fetch_optional(&self.pool)
                .await
                .map_err(|e| ServiceError::Internal(format!("member role: {e}")))?;
        Ok(row.map(|(r,)| r))
    }
}

// ---------------------------------------------------------------------------
// Storage policy repository (PG)
// ---------------------------------------------------------------------------

/// A named storage policy row (mirrors the `storage_policies` table).
///
/// Lives in the service layer (not `domain`) because it carries a
/// `serde_json::Value` config blob and is a persistence concern.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct StoragePolicy {
    /// Policy name (primary key, referenced by `images.storage_policy`).
    pub name: String,
    /// Driver kind: `local`, `s3`, `oss`, `cos`, `qiniu`, `minio`.
    pub driver: String,
    /// Driver-specific config (JSON object).
    pub config: serde_json::Value,
    /// Whether this is the default policy.
    pub is_default: bool,
}

/// Repository for the `storage_policies` table.
#[async_trait]
pub trait StoragePolicyRepository: Send + Sync {
    /// Lists all storage policies, ordered by name.
    async fn list(&self) -> Result<Vec<StoragePolicy>, ServiceError>;
    /// Creates a storage policy. Fails on duplicate name.
    async fn create(&self, policy: &StoragePolicy) -> Result<(), ServiceError>;
}

/// PostgreSQL-backed storage-policy repository.
#[derive(Debug, Clone)]
pub struct PgStoragePolicyRepository {
    pool: PgPool,
}

impl PgStoragePolicyRepository {
    /// Creates a new repository bound to the given pool.
    pub const fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl StoragePolicyRepository for PgStoragePolicyRepository {
    async fn list(&self) -> Result<Vec<StoragePolicy>, ServiceError> {
        // Read `config` as text (the `json` sqlx feature is intentionally not
        // enabled workspace-wide), then parse in Rust.
        let rows: Vec<(String, String, String, bool)> = sqlx::query_as(
            r"SELECT name, driver, config::text, is_default FROM storage_policies ORDER BY name",
        )
        .fetch_all(&self.pool)
        .await
        .map_err(|e| ServiceError::Internal(format!("list storage policies: {e}")))?;
        Ok(rows
            .into_iter()
            .map(|(name, driver, config_text, is_default)| StoragePolicy {
                name,
                driver,
                config: serde_json::from_str(&config_text).unwrap_or(serde_json::Value::Null),
                is_default,
            })
            .collect())
    }

    async fn create(&self, policy: &StoragePolicy) -> Result<(), ServiceError> {
        let config_str = serde_json::to_string(&policy.config)
            .map_err(|e| ServiceError::Internal(format!("serialize config: {e}")))?;
        sqlx::query(
            r"INSERT INTO storage_policies (name, driver, config, is_default)
              VALUES ($1, $2, $3::jsonb, $4)",
        )
        .bind(&policy.name)
        .bind(&policy.driver)
        .bind(&config_str)
        .bind(policy.is_default)
        .execute(&self.pool)
        .await
        .map_err(|e| ServiceError::Internal(format!("create storage policy: {e}")))?;
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Resource ACL repository (PG + SQLite)
// ---------------------------------------------------------------------------

/// One grant on a resource — a `resource_acls` row reduced to its semantics.
///
/// `resource_type`/`resource_id` are query parameters (they identify the
/// resource the grant is attached to), so a grant carries only the subject,
/// the action, and the effect.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AclGrant {
    /// Who the grant applies to.
    pub subject: picroom_auth::AclSubject,
    /// Granted action (`admin` acts as a wildcard).
    pub action: picroom_auth::PermissionAction,
    /// `allow` or `deny` (deny is the highest-priority rule).
    pub effect: picroom_auth::AclEffect,
}

impl AclGrant {
    /// Builds an allow grant.
    #[allow(clippy::missing_const_for_fn)] // enum construction; const triggers no benefit
    pub fn allow(
        subject: picroom_auth::AclSubject,
        action: picroom_auth::PermissionAction,
    ) -> Self {
        Self {
            subject,
            action,
            effect: picroom_auth::AclEffect::Allow,
        }
    }

    /// Builds a deny grant.
    #[allow(clippy::missing_const_for_fn)]
    pub fn deny(subject: picroom_auth::AclSubject, action: picroom_auth::PermissionAction) -> Self {
        Self {
            subject,
            action,
            effect: picroom_auth::AclEffect::Deny,
        }
    }
}

/// Repository for `resource_acls`.
#[async_trait]
pub trait ResourceAclRepository: Send + Sync {
    /// Lists every grant attached to a resource.
    async fn list_grants(
        &self,
        resource_type: &str,
        resource_id: Uuid,
    ) -> Result<Vec<AclGrant>, ServiceError>;
    /// Replaces the full grant set of a resource (idempotent).
    async fn replace_grants(
        &self,
        resource_type: &str,
        resource_id: Uuid,
        grants: &[AclGrant],
    ) -> Result<(), ServiceError>;
    /// Removes every grant for one subject on one resource. Returns the number
    /// of removed rows.
    async fn revoke(
        &self,
        resource_type: &str,
        resource_id: Uuid,
        subject: picroom_auth::AclSubject,
    ) -> Result<u64, ServiceError>;
}

/// Parses a DB CHECK value back to a [`picroom_auth::PermissionAction`].
fn action_from_str(s: &str) -> Option<picroom_auth::PermissionAction> {
    match s {
        "read" => Some(picroom_auth::PermissionAction::Read),
        "create" => Some(picroom_auth::PermissionAction::Create),
        "update" => Some(picroom_auth::PermissionAction::Update),
        "delete" => Some(picroom_auth::PermissionAction::Delete),
        "admin" => Some(picroom_auth::PermissionAction::Admin),
        _ => None,
    }
}

/// Splits a subject into its `(subject_type, subject_id)` columns.
const fn subject_to_cols(s: picroom_auth::AclSubject) -> (&'static str, Uuid) {
    match s {
        picroom_auth::AclSubject::User(id) => ("user", id),
        picroom_auth::AclSubject::Team(id) => ("team", id),
    }
}

/// Rebuilds a subject from its `(subject_type, subject_id)` columns.
fn subject_from_cols(t: &str, id: Uuid) -> Option<picroom_auth::AclSubject> {
    match t {
        "user" => Some(picroom_auth::AclSubject::User(id)),
        "team" => Some(picroom_auth::AclSubject::Team(id)),
        _ => None,
    }
}

/// Splits a grant into bindable columns (shared by both backends).
type GrantCols = (String, Uuid, String, Uuid, String, String);

fn grant_to_cols(resource_type: &str, resource_id: Uuid, g: &AclGrant) -> GrantCols {
    let (st, sid) = subject_to_cols(g.subject);
    (
        resource_type.to_string(),
        resource_id,
        st.to_string(),
        sid,
        g.action.as_str().to_string(),
        match g.effect {
            picroom_auth::AclEffect::Allow => "allow".to_string(),
            picroom_auth::AclEffect::Deny => "deny".to_string(),
        },
    )
}

fn grant_from_cols(st: &str, sid: Uuid, permission: &str, effect: &str) -> Option<AclGrant> {
    Some(AclGrant {
        subject: subject_from_cols(st, sid)?,
        action: action_from_str(permission)?,
        effect: match effect {
            "allow" => picroom_auth::AclEffect::Allow,
            "deny" => picroom_auth::AclEffect::Deny,
            _ => return None,
        },
    })
}

/// PostgreSQL-backed ACL repository.
#[derive(Debug, Clone)]
pub struct PgResourceAclRepository {
    pool: PgPool,
}

impl PgResourceAclRepository {
    /// Creates a new repository bound to the given pool.
    pub const fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl ResourceAclRepository for PgResourceAclRepository {
    async fn list_grants(
        &self,
        resource_type: &str,
        resource_id: Uuid,
    ) -> Result<Vec<AclGrant>, ServiceError> {
        let rows: Vec<(String, Uuid, String, String)> = sqlx::query_as(
            r"SELECT subject_type, subject_id, permission, effect
              FROM resource_acls WHERE resource_type = $1 AND resource_id = $2",
        )
        .bind(resource_type)
        .bind(resource_id)
        .fetch_all(&self.pool)
        .await
        .map_err(|e| ServiceError::Internal(format!("list grants: {e}")))?;
        let mut out = Vec::with_capacity(rows.len());
        for (st, sid, permission, effect) in rows {
            out.push(
                grant_from_cols(&st, sid, &permission, &effect)
                    .ok_or_else(|| ServiceError::Internal("unknown acl row value".into()))?,
            );
        }
        Ok(out)
    }

    async fn replace_grants(
        &self,
        resource_type: &str,
        resource_id: Uuid,
        grants: &[AclGrant],
    ) -> Result<(), ServiceError> {
        let mut tx = self
            .pool
            .begin()
            .await
            .map_err(|e| ServiceError::Internal(format!("acl tx: {e}")))?;
        sqlx::query("DELETE FROM resource_acls WHERE resource_type = $1 AND resource_id = $2")
            .bind(resource_type)
            .bind(resource_id)
            .execute(&mut *tx)
            .await
            .map_err(|e| ServiceError::Internal(format!("acl clear: {e}")))?;
        for g in grants {
            let (rt, rid, st, sid, permission, effect) =
                grant_to_cols(resource_type, resource_id, g);
            sqlx::query(
                r"INSERT INTO resource_acls
                    (id, resource_type, resource_id, subject_type, subject_id, permission, effect)
                  VALUES ($1, $2, $3, $4, $5, $6, $7)",
            )
            .bind(Uuid::now_v7())
            .bind(rt)
            .bind(rid)
            .bind(st)
            .bind(sid)
            .bind(permission)
            .bind(effect)
            .execute(&mut *tx)
            .await
            .map_err(|e| ServiceError::Internal(format!("acl insert: {e}")))?;
        }
        tx.commit()
            .await
            .map_err(|e| ServiceError::Internal(format!("acl commit: {e}")))?;
        Ok(())
    }

    async fn revoke(
        &self,
        resource_type: &str,
        resource_id: Uuid,
        subject: picroom_auth::AclSubject,
    ) -> Result<u64, ServiceError> {
        let (st, sid) = subject_to_cols(subject);
        let res = sqlx::query(
            r"DELETE FROM resource_acls
              WHERE resource_type = $1 AND resource_id = $2 AND subject_type = $3 AND subject_id = $4",
        )
        .bind(resource_type)
        .bind(resource_id)
        .bind(st)
        .bind(sid)
        .execute(&self.pool)
        .await
        .map_err(|e| ServiceError::Internal(format!("acl revoke: {e}")))?;
        Ok(res.rows_affected())
    }
}

/// SQLite-backed ACL repository (dev path — mirrors the PostgreSQL semantics).
#[derive(Debug, Clone)]
pub struct SqliteResourceAclRepository {
    pool: SqlitePool,
}

impl SqliteResourceAclRepository {
    /// Creates a new repository bound to the given pool.
    pub const fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl ResourceAclRepository for SqliteResourceAclRepository {
    async fn list_grants(
        &self,
        resource_type: &str,
        resource_id: Uuid,
    ) -> Result<Vec<AclGrant>, ServiceError> {
        let rows: Vec<(String, String, String, String)> = sqlx::query_as(
            r"SELECT subject_type, subject_id, permission, effect
              FROM resource_acls WHERE resource_type = ?1 AND resource_id = ?2",
        )
        .bind(resource_type)
        .bind(resource_id.to_string())
        .fetch_all(&self.pool)
        .await
        .map_err(|e| ServiceError::Internal(format!("list grants: {e}")))?;
        let mut out = Vec::with_capacity(rows.len());
        for (st, sid_text, permission, effect) in rows {
            let sid = Uuid::parse_str(&sid_text)
                .map_err(|e| ServiceError::Internal(format!("acl subject id: {e}")))?;
            out.push(
                grant_from_cols(&st, sid, &permission, &effect)
                    .ok_or_else(|| ServiceError::Internal("unknown acl row value".into()))?,
            );
        }
        Ok(out)
    }

    async fn replace_grants(
        &self,
        resource_type: &str,
        resource_id: Uuid,
        grants: &[AclGrant],
    ) -> Result<(), ServiceError> {
        let mut tx = self
            .pool
            .begin()
            .await
            .map_err(|e| ServiceError::Internal(format!("acl tx: {e}")))?;
        sqlx::query("DELETE FROM resource_acls WHERE resource_type = ?1 AND resource_id = ?2")
            .bind(resource_type)
            .bind(resource_id.to_string())
            .execute(&mut *tx)
            .await
            .map_err(|e| ServiceError::Internal(format!("acl clear: {e}")))?;
        for g in grants {
            let (rt, rid, st, sid, permission, effect) =
                grant_to_cols(resource_type, resource_id, g);
            sqlx::query(
                r"INSERT INTO resource_acls
                    (id, resource_type, resource_id, subject_type, subject_id, permission, effect, granted_at)
                  VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            )
            .bind(Uuid::now_v7().to_string())
            .bind(rt)
            .bind(rid.to_string())
            .bind(st)
            .bind(sid.to_string())
            .bind(permission)
            .bind(effect)
            .bind(OffsetDateTime::now_utc().to_string())
            .execute(&mut *tx)
            .await
            .map_err(|e| ServiceError::Internal(format!("acl insert: {e}")))?;
        }
        tx.commit()
            .await
            .map_err(|e| ServiceError::Internal(format!("acl commit: {e}")))?;
        Ok(())
    }

    async fn revoke(
        &self,
        resource_type: &str,
        resource_id: Uuid,
        subject: picroom_auth::AclSubject,
    ) -> Result<u64, ServiceError> {
        let (st, sid) = subject_to_cols(subject);
        let res = sqlx::query(
            r"DELETE FROM resource_acls
              WHERE resource_type = ?1 AND resource_id = ?2 AND subject_type = ?3 AND subject_id = ?4",
        )
        .bind(resource_type)
        .bind(resource_id.to_string())
        .bind(st)
        .bind(sid.to_string())
        .execute(&self.pool)
        .await
        .map_err(|e| ServiceError::Internal(format!("acl revoke: {e}")))?;
        Ok(res.rows_affected())
    }
}

// ---------------------------------------------------------------------------
// Session repository (PG)
// ---------------------------------------------------------------------------

/// A live login session — a `sessions` row the JWT's `sid` claim points at.
#[derive(Debug, Clone)]
pub struct SessionRow {
    /// Session id (the JWT `sid`).
    pub id: Uuid,
    /// Owning user.
    pub user_id: Uuid,
    /// Expiry (server-side; the JWT has its own shorter `exp`).
    pub expires_at: OffsetDateTime,
}

/// Repository for login sessions. Revocation is the point: a logout or a
/// disabled user must invalidate outstanding tokens before they expire (D-6).
#[async_trait]
pub trait SessionRepository: Send + Sync {
    /// Inserts a new session.
    async fn create(&self, session: &SessionRow) -> Result<(), ServiceError>;
    /// Returns the session when it exists and is neither revoked nor expired.
    async fn get_active(&self, id: Uuid) -> Result<Option<SessionRow>, ServiceError>;
    /// Revokes one session (logout). Returns the number of affected rows.
    async fn revoke(&self, id: Uuid) -> Result<u64, ServiceError>;
    /// Revokes every live session of a user (disable-user cascade).
    async fn revoke_all_for_user(&self, user_id: Uuid) -> Result<u64, ServiceError>;
}

/// PostgreSQL-backed session repository.
#[derive(Debug, Clone)]
pub struct PgSessionRepository {
    pool: PgPool,
}

impl PgSessionRepository {
    /// Creates a new repository bound to the given pool.
    pub const fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl SessionRepository for PgSessionRepository {
    async fn create(&self, session: &SessionRow) -> Result<(), ServiceError> {
        sqlx::query(
            r"INSERT INTO sessions (id, user_id, csrf_token, expires_at)
              VALUES ($1, $2, $3, $4)",
        )
        .bind(session.id)
        .bind(session.user_id)
        .bind(Uuid::now_v7().simple().to_string()) // csrf placeholder; cookie flows unused today
        .bind(session.expires_at)
        .execute(&self.pool)
        .await
        .map_err(|e| ServiceError::Internal(format!("session create: {e}")))?;
        Ok(())
    }

    async fn get_active(&self, id: Uuid) -> Result<Option<SessionRow>, ServiceError> {
        let row: Option<(Uuid, Uuid, OffsetDateTime)> = sqlx::query_as(
            r"SELECT id, user_id, expires_at FROM sessions
              WHERE id = $1 AND revoked_at IS NULL AND expires_at > NOW()",
        )
        .bind(id)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| ServiceError::Internal(format!("session get: {e}")))?;
        Ok(row.map(|(id, user_id, expires_at)| SessionRow {
            id,
            user_id,
            expires_at,
        }))
    }

    async fn revoke(&self, id: Uuid) -> Result<u64, ServiceError> {
        let res = sqlx::query(
            "UPDATE sessions SET revoked_at = NOW() WHERE id = $1 AND revoked_at IS NULL",
        )
        .bind(id)
        .execute(&self.pool)
        .await
        .map_err(|e| ServiceError::Internal(format!("session revoke: {e}")))?;
        Ok(res.rows_affected())
    }

    async fn revoke_all_for_user(&self, user_id: Uuid) -> Result<u64, ServiceError> {
        let res = sqlx::query(
            "UPDATE sessions SET revoked_at = NOW() WHERE user_id = $1 AND revoked_at IS NULL",
        )
        .bind(user_id)
        .execute(&self.pool)
        .await
        .map_err(|e| ServiceError::Internal(format!("session revoke all: {e}")))?;
        Ok(res.rows_affected())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use time::OffsetDateTime;
    use uuid::Uuid;

    #[test]
    fn none_cursor_decodes_to_empty_parts() {
        assert_eq!(parse_composite_cursor(None).unwrap(), (None, None));
    }

    #[test]
    fn cursor_round_trips_through_format_and_parse() {
        let ts = OffsetDateTime::now_utc();
        let id = Uuid::now_v7();
        let encoded = format_cursor(ts, id).unwrap();
        let (got_ts, got_id) = parse_composite_cursor(Some(&encoded)).unwrap();
        assert_eq!(got_ts, Some(ts));
        assert_eq!(got_id, Some(id));
    }

    #[test]
    fn cursor_without_separator_is_invalid() {
        let err = parse_composite_cursor(Some("no-separator-here")).unwrap_err();
        assert!(format!("{err}").contains("invalid cursor"));
    }

    #[test]
    fn cursor_with_bad_timestamp_is_invalid() {
        let id = Uuid::now_v7();
        let bad = format!("not-a-timestamp|{id}");
        let err = parse_composite_cursor(Some(&bad)).unwrap_err();
        assert!(format!("{err}").contains("invalid cursor ts"));
    }

    #[test]
    fn cursor_with_bad_uuid_is_invalid() {
        let ts = OffsetDateTime::now_utc();
        let ts_str = ts
            .format(&time::format_description::well_known::Rfc3339)
            .unwrap();
        let bad = format!("{ts_str}|not-a-uuid");
        let err = parse_composite_cursor(Some(&bad)).unwrap_err();
        assert!(format!("{err}").contains("invalid cursor id"));
    }

    #[test]
    fn format_cursor_uses_rfc3339_pipe_encoding() {
        let ts = OffsetDateTime::UNIX_EPOCH;
        let id = Uuid::nil();
        let encoded = format_cursor(ts, id).unwrap();
        assert!(encoded.starts_with("1970-01-01T00:00:00"));
        assert!(encoded.ends_with("|00000000-0000-0000-0000-000000000000"));
    }
}
