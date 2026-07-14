// SPDX-License-Identifier: MIT
// Copyright (c) 2026 Picroom Contributors

//! Migration runner and status reporting.

use picroom_infra::Database;
use std::collections::HashSet;
use thiserror::Error;

/// Migration errors.
#[derive(Debug, Error)]
pub enum MigrateError {
    /// DB error.
    #[error("db: {0}")]
    Db(String),
}

/// A migration discovered on disk (or embedded by `sqlx::migrate!`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KnownMigration {
    /// Migration version (matches the numeric filename prefix and the
    /// `version` column of `_sqlx_migrations`).
    pub version: i64,
    /// Human-readable description (the migration filename without extension).
    pub description: String,
}

/// A migration row recorded in the `_sqlx_migrations` tracking table.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AppliedMigration {
    /// Migration version.
    pub version: i64,
    /// Description as stored when the migration ran.
    pub description: String,
    /// Whether the migration reported success.
    pub success: bool,
    /// When the migration was installed (ISO-8601 text).
    pub installed_on: String,
}

/// Classification of known vs applied migrations.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct MigrationStatus {
    /// Known migrations that have a successful applied row.
    pub applied: Vec<KnownMigration>,
    /// Known migrations with no applied row yet.
    pub pending: Vec<KnownMigration>,
    /// Applied rows that reported `success = false`.
    pub failed: Vec<AppliedMigration>,
}

/// Pure classifier: splits `known` migrations into `applied`/`pending` and
/// surfaces any rows recorded as failed.
///
/// This contains the entire decision logic so it can be unit-tested without a
/// database.
pub fn classify_migrations(known: &[KnownMigration], applied: &[AppliedMigration]) -> MigrationStatus {
    let applied_success: HashSet<i64> = applied
        .iter()
        .filter(|a| a.success)
        .map(|a| a.version)
        .collect();
    let mut applied_known = Vec::new();
    let mut pending = Vec::new();
    for k in known {
        if applied_success.contains(&k.version) {
            applied_known.push(k.clone());
        } else {
            pending.push(k.clone());
        }
    }
    let failed = applied.iter().filter(|a| !a.success).cloned().collect();
    MigrationStatus {
        applied: applied_known,
        pending,
        failed,
    }
}

/// Parses a migration filename such as `0007_quotas.sql` into its version and
/// description. Returns `None` when the prefix is not a decimal integer.
pub fn parse_migration_filename(name: &str) -> Option<KnownMigration> {
    let stem = name.strip_suffix(".sql").unwrap_or(name);
    let (ver, _desc) = stem.split_once('_')?;
    let version = ver.parse::<i64>().ok()?;
    Some(KnownMigration {
        version,
        description: stem.to_string(),
    })
}

/// Runs all pending migrations.
pub async fn migrate_run(db: &Database) -> Result<(), MigrateError> {
    match db {
        Database::Postgres(pool) => sqlx::migrate!("../../migrations")
            .run(pool)
            .await
            .map_err(|e| MigrateError::Db(e.to_string())),
        Database::Sqlite(pool) => sqlx::migrate!("../../migrations")
            .run(pool)
            .await
            .map_err(|e| MigrateError::Db(e.to_string())),
    }
}

/// Reports the migration status: which known migrations are applied, pending,
/// or recorded as failed.
pub async fn migrate_status(db: &Database) -> Result<MigrationStatus, MigrateError> {
    // Known migrations are embedded at compile time by `sqlx::migrate!`, so we
    // do not depend on the migrations directory being present at runtime.
    let migrator = sqlx::migrate!("../../migrations");
    let known: Vec<KnownMigration> = migrator
        .migrations
        .iter()
        .map(|m| KnownMigration {
            version: m.version,
            description: m.description.to_string(),
        })
        .collect();
    let applied = applied_migrations(db).await?;
    Ok(classify_migrations(&known, &applied))
}

/// Reads the `_sqlx_migrations` tracking table. Errors (e.g. the table does
/// not exist on a fresh database) are propagated so the caller can advise
/// running `migrate run` first.
async fn applied_migrations(db: &Database) -> Result<Vec<AppliedMigration>, MigrateError> {
    const PG: &str = "SELECT version, description, success, installed_on::text \
                      FROM _sqlx_migrations ORDER BY version";
    const SQLITE: &str = "SELECT version, description, success, installed_on \
                          FROM _sqlx_migrations ORDER BY version";

    fn map(rows: Vec<(i64, String, bool, String)>) -> Vec<AppliedMigration> {
        rows.into_iter()
            .map(|(version, description, success, installed_on)| AppliedMigration {
                version,
                description,
                success,
                installed_on,
            })
            .collect()
    }

    match db {
        Database::Postgres(pool) => sqlx::query_as::<_, (i64, String, bool, String)>(PG)
            .fetch_all(pool)
            .await
            .map(map)
            .map_err(|e| MigrateError::Db(e.to_string())),
        Database::Sqlite(pool) => sqlx::query_as::<_, (i64, String, bool, String)>(SQLITE)
            .fetch_all(pool)
            .await
            .map(map)
            .map_err(|e| MigrateError::Db(e.to_string())),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn known(versions: &[i64]) -> Vec<KnownMigration> {
        versions
            .iter()
            .map(|v| KnownMigration {
                version: *v,
                description: format!("m{v}"),
            })
            .collect()
    }

    fn applied(versions: &[i64], success: bool) -> Vec<AppliedMigration> {
        versions
            .iter()
            .map(|v| AppliedMigration {
                version: *v,
                description: format!("m{v}"),
                success,
                installed_on: "2026-01-01T00:00:00Z".into(),
            })
            .collect()
    }

    #[test]
    fn classifies_applied_pending_failed() {
        let known = known(&[1, 2, 3, 4]);
        // 1 ok, 2 failed, 3 ok; 4 never applied.
        let applied_rows = {
            let mut v = applied(&[1, 3], true);
            v.push(AppliedMigration {
                version: 2,
                description: "m2".into(),
                success: false,
                installed_on: "2026-01-01T00:00:00Z".into(),
            });
            v
        };
        let status = classify_migrations(&known, &applied_rows);
        assert_eq!(
            status.applied.iter().map(|m| m.version).collect::<Vec<_>>(),
            vec![1, 3]
        );
        // version 2 is recorded but failed, so it is pending (not applied) and failed.
        assert_eq!(
            status.pending.iter().map(|m| m.version).collect::<Vec<_>>(),
            vec![2, 4]
        );
        assert_eq!(
            status.failed.iter().map(|m| m.version).collect::<Vec<_>>(),
            vec![2]
        );
    }

    #[test]
    fn all_pending_when_nothing_applied() {
        let known = known(&[1, 2]);
        let status = classify_migrations(&known, &[]);
        assert!(status.applied.is_empty());
        assert_eq!(status.pending.len(), 2);
        assert!(status.failed.is_empty());
    }

    #[test]
    fn all_applied_when_fully_migrated() {
        let known = known(&[1, 2]);
        let status = classify_migrations(&known, &applied(&[1, 2], true));
        assert_eq!(status.applied.len(), 2);
        assert!(status.pending.is_empty());
        assert!(status.failed.is_empty());
    }

    #[test]
    fn parse_filename_roundtrip() {
        let m = parse_migration_filename("0007_quotas.sql").unwrap();
        assert_eq!(m.version, 7);
        assert_eq!(m.description, "0007_quotas");
        assert!(parse_migration_filename("not-a-migration.sql").is_none());
        assert!(parse_migration_filename("abc_quotas.sql").is_none());
    }
}
