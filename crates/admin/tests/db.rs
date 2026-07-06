//! Integration tests for the admin CLI against an in-memory SQLite
//! database with the production schema (subset).

use picroom_admin::team::{team_add_member_sqlite, team_create_sqlite, team_list_sqlite};
use picroom_admin::user::{
    user_create_sqlite, user_disable_sqlite, user_list_sqlite, user_set_role_sqlite,
};
use picroom_auth::Role;
use picroom_domain::UserId;
use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};
use sqlx::SqlitePool;
use uuid::Uuid;

async fn make_pool() -> SqlitePool {
    let opts: SqliteConnectOptions = SqliteConnectOptions::new()
        .filename(":memory:")
        .create_if_missing(true);
    let pool = SqlitePoolOptions::new()
        .max_connections(1)
        .connect_with(opts)
        .await
        .expect("connect");

    // Create the `users` and `teams` and `team_members` tables that admin
    // commands touch. We embed a small subset matching the production
    // migration (CREATE TABLE IF NOT EXISTS — idempotent).
    for stmt in [
        "CREATE TABLE IF NOT EXISTS users (
            id TEXT PRIMARY KEY,
            email TEXT NOT NULL UNIQUE,
            name TEXT NOT NULL,
            password_hash TEXT NOT NULL,
            role TEXT NOT NULL DEFAULT 'viewer',
            avatar_url TEXT,
            disabled INTEGER NOT NULL DEFAULT 0,
            email_verified INTEGER NOT NULL DEFAULT 0,
            created_at TEXT NOT NULL,
            updated_at TEXT NOT NULL
        )",
        "CREATE TABLE IF NOT EXISTS teams (
            id TEXT PRIMARY KEY,
            name TEXT NOT NULL,
            slug TEXT NOT NULL UNIQUE,
            description TEXT,
            storage_policy TEXT,
            created_at TEXT NOT NULL,
            updated_at TEXT NOT NULL
        )",
        "CREATE TABLE IF NOT EXISTS team_members (
            team_id TEXT NOT NULL REFERENCES teams(id) ON DELETE CASCADE,
            user_id TEXT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
            role TEXT NOT NULL DEFAULT 'uploader',
            joined_at TEXT NOT NULL,
            PRIMARY KEY (team_id, user_id)
        )",
    ] {
        sqlx::query(stmt).execute(&pool).await.expect(stmt);
    }
    pool
}

#[tokio::test]
async fn create_list_set_role_disable_user() {
    let pool = make_pool().await;

    // Create user with viewer role.
    let alice_id = user_create_sqlite(
        &pool,
        "alice@example.com".into(),
        "Alice".into(),
        "p@ssw0rd-strong-enough".into(),
        Role::Viewer,
    )
    .await
    .expect("create alice");

    let list = user_list_sqlite(&pool).await.expect("list");
    assert_eq!(list.len(), 1, "one user");
    assert_eq!(list[0].0, alice_id);
    assert_eq!(list[0].1, "alice@example.com");

    // Promote to admin and check.
    user_set_role_sqlite(&pool, alice_id, Role::Admin)
        .await
        .expect("set role");
    let list = user_list_sqlite(&pool).await.expect("list");
    assert_eq!(list[0].2, Role::Admin);

    // Disable and re-list — disabled users are excluded by the list query.
    user_disable_sqlite(&pool, alice_id).await.expect("disable");
    let list = user_list_sqlite(&pool).await.expect("list");
    assert_eq!(list.len(), 0, "disabled user hidden");
}

#[tokio::test]
async fn create_team_and_add_member() {
    let pool = make_pool().await;

    let team_id = team_create_sqlite(&pool, "Engineering".into(), "eng".into())
        .await
        .expect("create team");
    let user_id = user_create_sqlite(
        &pool,
        "bob@example.com".into(),
        "Bob".into(),
        "another-strong-pwd".into(),
        Role::Uploader,
    )
    .await
    .expect("create user");

    team_add_member_sqlite(&pool, team_id, user_id, Role::Manager)
        .await
        .expect("add member");

    let teams = team_list_sqlite(&pool).await.expect("list");
    assert_eq!(teams.len(), 1);
    assert_eq!(teams[0].0, team_id);
    assert_eq!(teams[0].1, "Engineering");
    assert_eq!(teams[0].2, "eng");
}

#[tokio::test]
async fn team_member_idempotent() {
    let pool = make_pool().await;
    let team_id = team_create_sqlite(&pool, "T".into(), "t".into())
        .await
        .unwrap();
    // Need a real user so the FK constraint is satisfied.
    let user_id = user_create_sqlite(
        &pool,
        "idem@example.com".into(),
        "Idem".into(),
        "strong-pwd-12345".into(),
        Role::Viewer,
    )
    .await
    .unwrap();

    // Direct insertion (admin user add path) — multiple times is fine.
    team_add_member_sqlite(&pool, team_id, user_id, Role::Viewer)
        .await
        .unwrap();
    team_add_member_sqlite(&pool, team_id, user_id, Role::Admin)
        .await
        .unwrap();

    // Last write wins (REPLACE in SQLite); we don't require a specific
    // role here, just that the row exists.
    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM team_members WHERE team_id = ?1")
        .bind(team_id.as_uuid().to_string())
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(count, 1);
}
