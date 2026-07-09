// SPDX-License-Identifier: MIT
// Copyright (c) 2026 Picroom Contributors

//! User admin subcommands — DB-backed.
//!
//! Realises `picroom admin user create | list | set-role | disable`.

use clap::Subcommand;
use picroom_auth::{PasswordHasher, Role};
use picroom_domain::UserId;
use serde::{Deserialize, Serialize};
use sqlx::postgres::PgPoolOptions;
use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};
use sqlx::{PgPool, SqlitePool};
use thiserror::Error;
use time::OffsetDateTime;
use uuid::Uuid;

/// User admin errors.
#[derive(Debug, Error)]
pub enum UserError {
    /// DB error.
    #[error("db: {0}")]
    Db(String),
    /// Password hashing error.
    #[error("password: {0}")]
    Password(String),
}

/// User admin subcommand.
#[derive(Debug, Subcommand, Serialize, Deserialize)]
pub enum UserCmd {
    /// Create a new user.
    Create {
        /// Email address.
        #[arg(long)]
        email: String,
        /// Display name.
        #[arg(long)]
        name: String,
        /// Password (will be hashed).
        #[arg(long)]
        password: String,
        /// Role.
        #[arg(long, default_value = "viewer")]
        role: String,
    },
    /// List users.
    List,
    /// Set a user's role.
    SetRole {
        /// User id (UUID).
        user_id: UserId,
        /// New role.
        #[arg(long)]
        role: String,
    },
    /// Disable (soft-delete) a user.
    Disable {
        /// User id.
        user_id: UserId,
    },
}

/// Opens a connection pool based on the URL scheme.
pub async fn open_pool(url: &str) -> Result<AnyPool, sqlx::Error> {
    if url.starts_with("postgres://") || url.starts_with("postgresql://") {
        let pool = PgPoolOptions::new().max_connections(4).connect(url).await?;
        Ok(AnyPool::Pg(pool))
    } else if url.starts_with("sqlite://") {
        let opts: SqliteConnectOptions = url.parse()?;
        let pool = SqlitePoolOptions::new()
            .max_connections(2)
            .connect_with(opts)
            .await?;
        Ok(AnyPool::Sqlite(pool))
    } else {
        Err(sqlx::Error::Configuration(
            format!("unknown scheme: {url}").into(),
        ))
    }
}

/// Database-agnostic pool enum.
#[derive(Clone)]
pub enum AnyPool {
    Pg(PgPool),
    Sqlite(SqlitePool),
}

fn parse_role(s: &str) -> Role {
    match s {
        "admin" => Role::Admin,
        "manager" => Role::Manager,
        "uploader" => Role::Uploader,
        _ => Role::Viewer,
    }
}

/// Creates a user (Postgres-flavored).
pub async fn user_create_pg(
    pool: &PgPool,
    email: String,
    name: String,
    password: String,
    role: Role,
) -> Result<UserId, UserError> {
    let hash = PasswordHasher::new()
        .hash(&password)
        .map_err(|e| UserError::Password(e.to_string()))?;
    let id = Uuid::now_v7();
    sqlx::query(
        r"INSERT INTO users (id, email, name, password_hash, role, created_at, updated_at)
           VALUES ($1, $2, $3, $4, $5, NOW(), NOW())",
    )
    .bind(id)
    .bind(&email)
    .bind(&name)
    .bind(&hash)
    .bind(role.as_str())
    .execute(pool)
    .await
    .map_err(|e| UserError::Db(e.to_string()))?;
    Ok(UserId(id))
}

/// Creates a user (SQLite-flavored).
pub async fn user_create_sqlite(
    pool: &SqlitePool,
    email: String,
    name: String,
    password: String,
    role: Role,
) -> Result<UserId, UserError> {
    let hash = PasswordHasher::new()
        .hash(&password)
        .map_err(|e| UserError::Password(e.to_string()))?;
    let id = Uuid::now_v7();
    let now = OffsetDateTime::now_utc().to_string();
    sqlx::query(
        r"INSERT INTO users (id, email, name, password_hash, role, created_at, updated_at)
           VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?6)",
    )
    .bind(id.to_string())
    .bind(&email)
    .bind(&name)
    .bind(&hash)
    .bind(role.as_str())
    .bind(&now)
    .execute(pool)
    .await
    .map_err(|e| UserError::Db(e.to_string()))?;
    Ok(UserId(id))
}

/// Lists users (Pg).
pub async fn user_list_pg(pool: &PgPool) -> Result<Vec<(UserId, String, Role)>, UserError> {
    let rows: Vec<(Uuid, String, String)> = sqlx::query_as(
        "SELECT id, email, role FROM users WHERE disabled = false ORDER BY created_at DESC",
    )
    .fetch_all(pool)
    .await
    .map_err(|e| UserError::Db(e.to_string()))?;
    Ok(rows
        .into_iter()
        .map(|(id, email, role)| (UserId(id), email, parse_role(&role)))
        .collect())
}

/// Lists users (`SQLite`).
pub async fn user_list_sqlite(pool: &SqlitePool) -> Result<Vec<(UserId, String, Role)>, UserError> {
    let rows: Vec<(String, String, String)> = sqlx::query_as(
        "SELECT id, email, role FROM users WHERE disabled = 0 ORDER BY created_at DESC",
    )
    .fetch_all(pool)
    .await
    .map_err(|e| UserError::Db(e.to_string()))?;
    Ok(rows
        .into_iter()
        .filter_map(|(id, email, role)| {
            Uuid::parse_str(&id)
                .ok()
                .map(|u| (UserId(u), email, parse_role(&role)))
        })
        .collect())
}

/// Changes a user's role (Pg).
pub async fn user_set_role_pg(pool: &PgPool, user_id: UserId, role: Role) -> Result<(), UserError> {
    sqlx::query("UPDATE users SET role = $1, updated_at = NOW() WHERE id = $2")
        .bind(role.as_str())
        .bind(user_id.as_uuid())
        .execute(pool)
        .await
        .map_err(|e| UserError::Db(e.to_string()))?;
    Ok(())
}

/// Changes a user's role (`SQLite`).
pub async fn user_set_role_sqlite(
    pool: &SqlitePool,
    user_id: UserId,
    role: Role,
) -> Result<(), UserError> {
    sqlx::query("UPDATE users SET role = ?1, updated_at = ?2 WHERE id = ?3")
        .bind(role.as_str())
        .bind(OffsetDateTime::now_utc().to_string())
        .bind(user_id.as_uuid().to_string())
        .execute(pool)
        .await
        .map_err(|e| UserError::Db(e.to_string()))?;
    Ok(())
}

/// Disables a user (Pg).
pub async fn user_disable_pg(pool: &PgPool, user_id: UserId) -> Result<(), UserError> {
    sqlx::query("UPDATE users SET disabled = true, updated_at = NOW() WHERE id = $1")
        .bind(user_id.as_uuid())
        .execute(pool)
        .await
        .map_err(|e| UserError::Db(e.to_string()))?;
    Ok(())
}

/// Disables a user (`SQLite`).
pub async fn user_disable_sqlite(pool: &SqlitePool, user_id: UserId) -> Result<(), UserError> {
    sqlx::query("UPDATE users SET disabled = 1, updated_at = ?1 WHERE id = ?2")
        .bind(OffsetDateTime::now_utc().to_string())
        .bind(user_id.as_uuid().to_string())
        .execute(pool)
        .await
        .map_err(|e| UserError::Db(e.to_string()))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_role_maps() {
        assert_eq!(parse_role("admin"), Role::Admin);
        assert_eq!(parse_role("manager"), Role::Manager);
        assert_eq!(parse_role("uploader"), Role::Uploader);
        assert_eq!(parse_role("viewer"), Role::Viewer);
        assert_eq!(parse_role("garbage"), Role::Viewer);
    }
}
