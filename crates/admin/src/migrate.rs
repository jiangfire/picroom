//! Migration runner.

use picroom_infra::Database;
use thiserror::Error;

/// Migration errors.
#[derive(Debug, Error)]
pub enum MigrateError {
    /// DB error.
    #[error("db: {0}")]
    Db(String),
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
