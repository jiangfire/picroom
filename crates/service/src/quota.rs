// SPDX-License-Identifier: MIT
// Copyright (c) 2026 Picroom Contributors

//! Quota service.
//!
//! Per-user byte caps. [`QuotaService::remaining_user`] reads the configured
//! cap (or a built-in default when no `quotas` row exists) and subtracts the
//! bytes already stored, so uploads are rejected once a user's allowance is
//! spent. [`crate::UploadService`] consults this before persisting any bytes.
//!
//! When constructed without a database pool (dev mode / `SQLite` paths that have
//! not wired a quota repository) the service reports unlimited quota so the
//! upload path keeps working.

use crate::ServiceError;
use sqlx::PgPool;
use uuid::Uuid;

/// Default per-user quota when no explicit `quotas` row exists (1 GiB).
pub const DEFAULT_QUOTA: u64 = 1024 * 1024 * 1024;

/// Quota service.
#[derive(Clone)]
pub struct QuotaService {
    /// `PostgreSQL` pool. `None` ⇒ unlimited quota (no enforcement).
    pool: Option<PgPool>,
}

impl QuotaService {
    /// Creates a quota service with no database — reports unlimited quota.
    pub const fn new() -> Self {
        Self { pool: None }
    }

    /// Creates a quota service backed by a `PostgreSQL` pool.
    pub const fn with_pool(pool: PgPool) -> Self {
        Self { pool: Some(pool) }
    }

    /// Returns remaining bytes for the user.
    ///
    /// Computes `max_bytes − used_bytes` where `max_bytes` is the user's
    /// `quotas.max_bytes` (defaulting to [`DEFAULT_QUOTA`]) and `used_bytes`
    /// is the sum of non-deleted image sizes owned by the user.
    pub async fn remaining_user(&self, user_id: Uuid) -> Result<u64, ServiceError> {
        match &self.pool {
            Some(pool) => {
                let row: (i64, i64) = sqlx::query_as(
                    r"
                    SELECT
                        COALESCE((SELECT max_bytes FROM quotas WHERE user_id = $1), $2::bigint),
                        COALESCE(
                            (SELECT SUM(bytes)::bigint FROM images WHERE owner_id = $1 AND status != 'deleted'),
                            0
                        )
                    ",
                )
                .bind(user_id)
                .bind(DEFAULT_QUOTA as i64)
                .fetch_one(pool)
                .await
                .map_err(|e| ServiceError::Internal(format!("quota query: {e}")))?;
                let max = row.0.max(0) as u64;
                let used = row.1.max(0) as u64;
                Ok(max.saturating_sub(used))
            }
            None => Ok(u64::MAX),
        }
    }

    /// Returns remaining bytes for the team.
    ///
    /// Team-level quotas are not yet modeled; this always reports unlimited.
    pub async fn remaining_team(&self, _team_id: Uuid) -> Result<u64, ServiceError> {
        Ok(u64::MAX)
    }

    /// Charges `bytes` against the user's quota.
    ///
    /// Enforcement happens pre-upload via [`QuotaService::remaining_user`];
    /// this is a retained no-op hook kept for API compatibility.
    pub async fn charge_user(&self, _user_id: Uuid, _bytes: u64) -> Result<(), ServiceError> {
        Ok(())
    }
}

impl std::fmt::Debug for QuotaService {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("QuotaService")
            .field("db_backed", &self.pool.is_some())
            .finish()
    }
}

impl Default for QuotaService {
    fn default() -> Self {
        Self::new()
    }
}
