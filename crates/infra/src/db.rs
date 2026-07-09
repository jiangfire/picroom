// SPDX-License-Identifier: MIT
// Copyright (c) 2026 Picroom Contributors

//! Database pool wrappers.

use sqlx::postgres::{PgPool, PgPoolOptions};
use sqlx::sqlite::{SqlitePool, SqlitePoolOptions};
use std::time::Duration;
use thiserror::Error;

/// Database backend errors.
#[derive(Debug, Error)]
pub enum DbError {
    /// Postgres error.
    #[error("postgres: {0}")]
    Postgres(String),
    /// `SQLite` error.
    #[error("sqlite: {0}")]
    Sqlite(String),
    /// Invalid URL.
    #[error("invalid url: {0}")]
    InvalidUrl(String),
}

/// A unified database handle.
#[derive(Debug, Clone)]
pub enum Database {
    /// `PostgreSQL`.
    Postgres(PgPool),
    /// `SQLite`.
    Sqlite(SqlitePool),
}

impl Database {
    /// Builds a connection pool from a URL.
    pub async fn connect(url: &str) -> Result<Self, DbError> {
        if url.starts_with("postgres://") || url.starts_with("postgresql://") {
            let pool = PgPoolOptions::new()
                .max_connections(20)
                .min_connections(2)
                .acquire_timeout(Duration::from_secs(10))
                .connect(url)
                .await
                .map_err(|e| DbError::Postgres(e.to_string()))?;
            Ok(Self::Postgres(pool))
        } else if url.starts_with("sqlite://") {
            let pool = SqlitePoolOptions::new()
                .max_connections(5)
                .connect(url)
                .await
                .map_err(|e| DbError::Sqlite(e.to_string()))?;
            Ok(Self::Sqlite(pool))
        } else {
            Err(DbError::InvalidUrl(format!("unknown scheme: {url}")))
        }
    }

    /// Returns true if the connection is healthy.
    pub async fn ping(&self) -> bool {
        match self {
            Self::Postgres(p) => sqlx::query("SELECT 1").execute(p).await.is_ok(),
            Self::Sqlite(p) => sqlx::query("SELECT 1").execute(p).await.is_ok(),
        }
    }
}
