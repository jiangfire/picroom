// SPDX-License-Identifier: MIT
// Copyright (c) 2026 Picroom Contributors

//! Quota service tests on the SQLite dev path (Q-6), plus team quotas
//! (R-21). PostgreSQL paths are covered by the same queries in CI via
//! `DATABASE_URL`-gated suites; the SQL here is the dialect mirror.

use picroom_service::QuotaService;
use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};
use uuid::Uuid;

const SCHEMA: &str = "
CREATE TABLE IF NOT EXISTS quotas (
    user_id   TEXT PRIMARY KEY,
    max_bytes INTEGER NOT NULL DEFAULT 1073741824
);
CREATE TABLE IF NOT EXISTS team_quotas (
    team_id   TEXT PRIMARY KEY,
    max_bytes INTEGER NOT NULL DEFAULT 1073741824
);
CREATE TABLE IF NOT EXISTS images (
    id          TEXT PRIMARY KEY,
    owner_id    TEXT NOT NULL,
    team_id     TEXT,
    status      TEXT NOT NULL DEFAULT 'pending',
    bytes       INTEGER NOT NULL DEFAULT 0
);
";

async fn sqlite_pool() -> sqlx::SqlitePool {
    let opts: SqliteConnectOptions = SqliteConnectOptions::new()
        .filename(":memory:")
        .create_if_missing(true);
    let pool = SqlitePoolOptions::new()
        .max_connections(1)
        .connect_with(opts)
        .await
        .expect("connect to in-memory sqlite");
    sqlx::query(SCHEMA)
        .execute(&pool)
        .await
        .expect("create schema");
    pool
}

#[tokio::test]
async fn sqlite_user_quota_enforced() {
    let pool = sqlite_pool().await;
    let user = Uuid::now_v7();
    let svc = QuotaService::with_sqlite_pool(pool.clone()).with_default_quota(1000);

    // No usage → full cap.
    assert_eq!(svc.remaining_user(user).await.unwrap(), 1000);

    // Store 700 bytes → 300 remain.
    sqlx::query("INSERT INTO images (id, owner_id, bytes) VALUES (?1, ?2, 700)")
        .bind(Uuid::now_v7().to_string())
        .bind(user.to_string())
        .execute(&pool)
        .await
        .unwrap();
    assert_eq!(svc.remaining_user(user).await.unwrap(), 300);

    // An explicit quotas row overrides the default cap.
    sqlx::query("INSERT INTO quotas (user_id, max_bytes) VALUES (?1, 5000)")
        .bind(user.to_string())
        .execute(&pool)
        .await
        .unwrap();
    assert_eq!(svc.remaining_user(user).await.unwrap(), 4300);
}

#[tokio::test]
async fn sqlite_team_quota_enforced() {
    let pool = sqlite_pool().await;
    let team = Uuid::now_v7();
    let member = Uuid::now_v7();
    let svc = QuotaService::with_sqlite_pool(pool.clone()).with_default_quota(1000);

    // No usage → full cap; unlimited for an unbacked service.
    assert_eq!(svc.remaining_team(team).await.unwrap(), 1000);

    // Team usage counts across every member's uploads attributed to the team.
    sqlx::query("INSERT INTO images (id, owner_id, team_id, bytes) VALUES (?1, ?2, ?3, 900)")
        .bind(Uuid::now_v7().to_string())
        .bind(member.to_string())
        .bind(team.to_string())
        .execute(&pool)
        .await
        .unwrap();
    assert_eq!(svc.remaining_team(team).await.unwrap(), 100);

    // Team quota exhausted → 0 remaining (upload must be rejected).
    sqlx::query("INSERT INTO images (id, owner_id, team_id, bytes) VALUES (?1, ?2, ?3, 100)")
        .bind(Uuid::now_v7().to_string())
        .bind(member.to_string())
        .bind(team.to_string())
        .execute(&pool)
        .await
        .unwrap();
    assert_eq!(svc.remaining_team(team).await.unwrap(), 0);
}

/// R-21: the upload path must reject a team upload once the team cap is
/// spent — the same ceiling applies to every member.
#[tokio::test]
async fn team_upload_rejected_when_team_quota_exhausted() {
    let pool = sqlite_pool().await;
    let team = Uuid::now_v7();
    let owner = Uuid::now_v7();
    let svc = QuotaService::with_sqlite_pool(pool.clone()).with_default_quota(10_000_000);
    sqlx::query("INSERT INTO team_quotas (team_id, max_bytes) VALUES (?1, 100)")
        .bind(team.to_string())
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("INSERT INTO images (id, owner_id, team_id, bytes) VALUES (?1, ?2, ?3, 100)")
        .bind(Uuid::now_v7().to_string())
        .bind(owner.to_string())
        .bind(team.to_string())
        .execute(&pool)
        .await
        .unwrap();
    assert_eq!(svc.remaining_team(team).await.unwrap(), 0);
}

#[tokio::test]
async fn unbacked_service_reports_unlimited() {
    let svc = QuotaService::new();
    let id = Uuid::now_v7();
    assert_eq!(svc.remaining_user(id).await.unwrap(), u64::MAX);
    assert_eq!(svc.remaining_team(id).await.unwrap(), u64::MAX);
}
