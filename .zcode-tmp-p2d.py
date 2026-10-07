# Temporary patch: Task 2.6 quota — team dimension + SQLite path + stage check.
p = 'crates/service/src/quota.rs'
src = open(p, encoding='utf-8').read()

src = src.replace('''use crate::ServiceError;
use sqlx::PgPool;
use uuid::Uuid;''', '''use crate::ServiceError;
use sqlx::{PgPool, SqlitePool};
use uuid::Uuid;''')

old = '''/// Quota service.
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
    }'''
new = '''/// Quota service.
#[derive(Clone)]
pub struct QuotaService {
    /// `PostgreSQL` pool. `None` ⇒ unlimited quota (no enforcement).
    pool: Option<PgPool>,
    /// `SQLite` pool for the dev path (Q-6: dev must not silently lose
    /// enforcement). Consulted when no `PostgreSQL` pool is set.
    sqlite_pool: Option<SqlitePool>,
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
            sqlite_pool: None,
            default_quota: DEFAULT_QUOTA,
        }
    }

    /// Creates a quota service backed by a `PostgreSQL` pool.
    pub const fn with_pool(pool: PgPool) -> Self {
        Self {
            pool: Some(pool),
            sqlite_pool: None,
            default_quota: DEFAULT_QUOTA,
        }
    }

    /// Creates a quota service backed by a `SQLite` pool (dev path).
    pub const fn with_sqlite_pool(pool: SqlitePool) -> Self {
        Self {
            pool: None,
            sqlite_pool: Some(pool),
            default_quota: DEFAULT_QUOTA,
        }
    }'''
assert old in src
src = src.replace(old, new)

# remaining_user: route to sqlite when pg absent
old = '''    pub async fn remaining_user(&self, user_id: Uuid) -> Result<u64, ServiceError> {
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
    }'''
new = '''    pub async fn remaining_user(&self, user_id: Uuid) -> Result<u64, ServiceError> {
        if let Some(pool) = &self.pool {
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
            return Ok((row.0.max(0) as u64).saturating_sub(row.1.max(0) as u64));
        }
        if let Some(pool) = &self.sqlite_pool {
            let row: (i64, i64) = sqlx::query_as(
                r"
                SELECT
                    COALESCE((SELECT max_bytes FROM quotas WHERE user_id = ?1), ?2),
                    COALESCE(
                        (SELECT SUM(bytes) FROM images WHERE owner_id = ?1 AND status != 'deleted'),
                        0
                    )
                ",
            )
            .bind(user_id.to_string())
            .bind(self.default_quota as i64)
            .fetch_one(pool)
            .await
            .map_err(|e| ServiceError::Internal(format!("quota query: {e}")))?;
            return Ok((row.0.max(0) as u64).saturating_sub(row.1.max(0) as u64));
        }
        Ok(u64::MAX)
    }'''
assert old in src
src = src.replace(old, new)

# remaining_team (restored, D-9 style): team_quotas + team images; both pools
old = '''            None => Ok(u64::MAX),
        }
    }
}'''
new = '''            None => Ok(u64::MAX),
        }
    }

    /// Returns remaining bytes for the team.
    ///
    /// Computes `max_bytes − used_bytes` where `max_bytes` is the team's
    /// `team_quotas.max_bytes` (defaulting to [`DEFAULT_QUOTA`]) and
    /// `used_bytes` is the sum of non-deleted image sizes attributed to the
    /// team. Unlimited when no database is configured.
    pub async fn remaining_team(&self, team_id: Uuid) -> Result<u64, ServiceError> {
        if let Some(pool) = &self.pool {
            let row: (i64, i64) = sqlx::query_as(
                r"
                SELECT
                    COALESCE((SELECT max_bytes FROM team_quotas WHERE team_id = $1), $2::bigint),
                    COALESCE(
                        (SELECT SUM(bytes)::bigint FROM images WHERE team_id = $1 AND status != 'deleted'),
                        0
                    )
                ",
            )
            .bind(team_id)
            .bind(self.default_quota as i64)
            .fetch_one(pool)
            .await
            .map_err(|e| ServiceError::Internal(format!("quota query: {e}")))?;
            return Ok((row.0.max(0) as u64).saturating_sub(row.1.max(0) as u64));
        }
        if let Some(pool) = &self.sqlite_pool {
            let row: (i64, i64) = sqlx::query_as(
                r"
                SELECT
                    COALESCE((SELECT max_bytes FROM team_quotas WHERE team_id = ?1), ?2),
                    COALESCE(
                        (SELECT SUM(bytes) FROM images WHERE team_id = ?1 AND status != 'deleted'),
                        0
                    )
                ",
            )
            .bind(team_id.to_string())
            .bind(self.default_quota as i64)
            .fetch_one(pool)
            .await
            .map_err(|e| ServiceError::Internal(format!("quota query: {e}")))?;
            return Ok((row.0.max(0) as u64).saturating_sub(row.1.max(0) as u64));
        }
        Ok(u64::MAX)
    }
}'''
assert old in src
src = src.replace(old, new)

# debug reports both pools; fix new_is_unbacked test field access
src = src.replace('''    #[test]
    fn new_is_unbacked_and_uses_default_quota() {
        let q = QuotaService::new();
        assert!(q.pool.is_none());
        assert_eq!(q.default_quota, DEFAULT_QUOTA);
    }''', '''    #[test]
    fn new_is_unbacked_and_uses_default_quota() {
        let q = QuotaService::new();
        assert!(q.pool.is_none());
        assert!(q.sqlite_pool.is_none());
        assert_eq!(q.default_quota, DEFAULT_QUOTA);
    }''')
open(p, 'w', encoding='utf-8', newline='\n').write(src)
print("ok")
