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
    /// Cap applied when a user has no explicit `quotas` row. Defaults to
    /// [`DEFAULT_QUOTA`] but is overridden from `QuotaConfig` in the binary
    /// wiring so the operator-tunable default is honored.
    default_quota: u64,
}

impl QuotaService {
    /// Creates a quota service with no database — reports unlimited quota.
    pub const fn new() -> Self {
        Self {
            pool: None,
            default_quota: DEFAULT_QUOTA,
        }
    }

    /// Creates a quota service backed by a `PostgreSQL` pool.
    pub const fn with_pool(pool: PgPool) -> Self {
        Self {
            pool: Some(pool),
            default_quota: DEFAULT_QUOTA,
        }
    }

    /// Overrides the default per-user cap used when no `quotas` row exists for
    /// the user. Mirrors `QuotaConfig::default_user_bytes`.
    pub const fn with_default_quota(mut self, bytes: u64) -> Self {
        self.default_quota = bytes;
        self
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
                .bind(self.default_quota as i64)
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

#[cfg(test)]
mod tests {
    use super::*;
    use uuid::Uuid;

    #[test]
    fn default_quota_constant_is_one_gib() {
        assert_eq!(DEFAULT_QUOTA, 1024 * 1024 * 1024);
    }

    #[test]
    fn new_is_unbacked_and_uses_default_quota() {
        let q = QuotaService::new();
        assert!(q.pool.is_none());
        assert_eq!(q.default_quota, DEFAULT_QUOTA);
    }

    #[test]
    fn with_default_quota_overrides_cap() {
        let q = QuotaService::new().with_default_quota(1234);
        assert_eq!(q.default_quota, 1234);
    }

    #[test]
    fn debug_reports_unbacked() {
        let q = QuotaService::new();
        assert!(format!("{q:?}").contains("db_backed: false"));
    }

    #[tokio::test]
    async fn remaining_user_unbacked_is_unlimited() {
        let q = QuotaService::new();
        assert_eq!(q.remaining_user(Uuid::now_v7()).await.unwrap(), u64::MAX);
    }

    #[tokio::test]
    async fn remaining_team_is_always_unlimited() {
        let q = QuotaService::new();
        assert_eq!(q.remaining_team(Uuid::now_v7()).await.unwrap(), u64::MAX);
    }

    #[tokio::test]
    async fn charge_user_is_a_noop() {
        let q = QuotaService::new();
        assert!(q.charge_user(Uuid::now_v7(), 10).await.is_ok());
    }
}
