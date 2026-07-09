// SPDX-License-Identifier: MIT
// Copyright (c) 2026 Picroom Contributors

//! Team admin subcommands — DB-backed.

use clap::Subcommand;
use picroom_auth::Role;
use picroom_domain::{TeamId, UserId};
use serde::{Deserialize, Serialize};
use sqlx::postgres::PgPool;
use sqlx::sqlite::SqlitePool;
use thiserror::Error;
use uuid::Uuid;

/// Team admin errors.
#[derive(Debug, Error)]
pub enum TeamError {
    /// DB error.
    #[error("db: {0}")]
    Db(String),
}

/// Team admin subcommand.
#[derive(Debug, Subcommand, Serialize, Deserialize)]
pub enum TeamCmd {
    /// Create a new team.
    Create {
        /// Team name.
        #[arg(long)]
        name: String,
        /// Team slug (URL-safe).
        #[arg(long)]
        slug: String,
    },
    /// List teams.
    List,
    /// Add a member.
    AddMember {
        /// Team id.
        team_id: TeamId,
        /// User id.
        user_id: UserId,
        /// Role within the team.
        #[arg(long, default_value = "uploader")]
        role: String,
    },
}

/// Creates a team (Pg).
pub async fn team_create_pg(
    pool: &PgPool,
    name: String,
    slug: String,
) -> Result<TeamId, TeamError> {
    let id = Uuid::now_v7();
    sqlx::query(
        r"INSERT INTO teams (id, name, slug, created_at, updated_at)
           VALUES ($1, $2, $3, NOW(), NOW())",
    )
    .bind(id)
    .bind(&name)
    .bind(&slug)
    .execute(pool)
    .await
    .map_err(|e| TeamError::Db(e.to_string()))?;
    Ok(TeamId(id))
}

/// Creates a team (`SQLite`).
pub async fn team_create_sqlite(
    pool: &SqlitePool,
    name: String,
    slug: String,
) -> Result<TeamId, TeamError> {
    let id = Uuid::now_v7();
    sqlx::query(
        r"INSERT INTO teams (id, name, slug, created_at, updated_at)
           VALUES (?1, ?2, ?3, ?4, ?4)",
    )
    .bind(id.to_string())
    .bind(&name)
    .bind(&slug)
    .bind(time::OffsetDateTime::now_utc().to_string())
    .execute(pool)
    .await
    .map_err(|e| TeamError::Db(e.to_string()))?;
    Ok(TeamId(id))
}

/// Adds a member (Pg).
pub async fn team_add_member_pg(
    pool: &PgPool,
    team_id: TeamId,
    user_id: UserId,
    role: Role,
) -> Result<(), TeamError> {
    sqlx::query(
        r"INSERT INTO team_members (team_id, user_id, role, joined_at)
           VALUES ($1, $2, $3, NOW())
           ON CONFLICT (team_id, user_id) DO UPDATE SET role = EXCLUDED.role",
    )
    .bind(team_id.as_uuid())
    .bind(user_id.as_uuid())
    .bind(role.as_str())
    .execute(pool)
    .await
    .map_err(|e| TeamError::Db(e.to_string()))?;
    Ok(())
}

/// Adds a member (`SQLite`).
pub async fn team_add_member_sqlite(
    pool: &SqlitePool,
    team_id: TeamId,
    user_id: UserId,
    role: Role,
) -> Result<(), TeamError> {
    sqlx::query(
        r"INSERT OR REPLACE INTO team_members (team_id, user_id, role, joined_at)
           VALUES (?1, ?2, ?3, ?4)",
    )
    .bind(team_id.as_uuid().to_string())
    .bind(user_id.as_uuid().to_string())
    .bind(role.as_str())
    .bind(time::OffsetDateTime::now_utc().to_string())
    .execute(pool)
    .await
    .map_err(|e| TeamError::Db(e.to_string()))?;
    Ok(())
}

/// Lists teams (Pg).
pub async fn team_list_pg(pool: &PgPool) -> Result<Vec<(TeamId, String, String)>, TeamError> {
    let rows: Vec<(Uuid, String, String)> =
        sqlx::query_as("SELECT id, name, slug FROM teams ORDER BY created_at DESC")
            .fetch_all(pool)
            .await
            .map_err(|e| TeamError::Db(e.to_string()))?;
    Ok(rows
        .into_iter()
        .map(|(id, name, slug)| (TeamId(id), name, slug))
        .collect())
}

/// Lists teams (`SQLite`).
pub async fn team_list_sqlite(
    pool: &SqlitePool,
) -> Result<Vec<(TeamId, String, String)>, TeamError> {
    let rows: Vec<(String, String, String)> =
        sqlx::query_as("SELECT id, name, slug FROM teams ORDER BY created_at DESC")
            .fetch_all(pool)
            .await
            .map_err(|e| TeamError::Db(e.to_string()))?;
    Ok(rows
        .into_iter()
        .filter_map(|(id, name, slug)| Uuid::parse_str(&id).ok().map(|u| (TeamId(u), name, slug)))
        .collect())
}
