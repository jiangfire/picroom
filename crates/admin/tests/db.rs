// SPDX-License-Identifier: MIT
// Copyright (c) 2026 Picroom Contributors

//! Integration tests for the admin CLI against an in-memory `SQLite`
//! database with the production schema (subset).

use picroom_admin::audit_cmd::AuditCmdError;
use picroom_admin::audit_tail;
use picroom_admin::migrate_status;
use picroom_admin::team::{team_add_member_sqlite, team_create_sqlite, team_list_sqlite};
use picroom_admin::user::{
    open_pool, user_create_sqlite, user_disable_sqlite, user_list_sqlite, user_set_role_sqlite,
    AnyPool,
};
use picroom_auth::Role;
use picroom_infra::Database;
use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};
use sqlx::SqlitePool;

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

// ---------------------------------------------------------------------------
// audit tail — reads `audit_events` (schema mirrors migrations/0005_sqlite_init.sql)
// ---------------------------------------------------------------------------

async fn make_audit_pool() -> SqlitePool {
    let opts: SqliteConnectOptions = SqliteConnectOptions::new()
        .filename(":memory:")
        .create_if_missing(true);
    let pool = SqlitePoolOptions::new()
        .max_connections(1)
        .connect_with(opts)
        .await
        .expect("connect");

    sqlx::query(
        "CREATE TABLE audit_events (
            id          TEXT PRIMARY KEY,
            timestamp   TEXT NOT NULL,
            actor_id    TEXT,
            actor_label TEXT,
            action      TEXT NOT NULL,
            target_type TEXT NOT NULL,
            target_id   TEXT,
            ip          TEXT,
            user_agent  TEXT,
            metadata    TEXT NOT NULL DEFAULT '{}'
        )",
    )
    .execute(&pool)
    .await
    .expect("create audit_events");
    pool
}

#[tokio::test]
async fn audit_tail_survives_corrupt_rows() {
    let pool = make_audit_pool().await;

    // A well-formed row.
    sqlx::query(
        "INSERT INTO audit_events (id, timestamp, actor_id, actor_label, action, target_type,
                                   target_id, ip, user_agent, metadata)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
    )
    .bind("11111111-1111-1111-1111-111111111111")
    .bind("2026-01-02T03:04:05Z")
    .bind("22222222-2222-2222-2222-222222222222")
    .bind("alice@example.com")
    .bind("image.upload")
    .bind("image")
    .bind("img-1")
    .bind("127.0.0.1")
    .bind("curl/8")
    .bind("{}")
    .execute(&pool)
    .await
    .expect("insert good row");

    // Every column the SQLite reader parses is garbage, and target_id/ip/
    // user_agent are NULL. The reader must fall back rather than abort the
    // whole tail — a single bad row cannot take the CLI down.
    sqlx::query(
        "INSERT INTO audit_events (id, timestamp, actor_id, actor_label, action, target_type,
                                   target_id, ip, user_agent, metadata)
         VALUES (?1, ?2, ?3, ?4, ?5, 'image', NULL, NULL, NULL, ?6)",
    )
    .bind("not-a-uuid")
    .bind("not-a-timestamp")
    .bind("not-a-uuid")
    .bind("bob@example.com")
    .bind("image.delete")
    .bind("not-json")
    .execute(&pool)
    .await
    .expect("insert corrupt row");

    audit_tail(&AnyPool::Sqlite(pool), false, None)
        .await
        .expect("audit tail tolerates malformed rows");
}

#[tokio::test]
async fn audit_tail_actor_filter_matching_nothing_is_clean() {
    let pool = make_audit_pool().await;
    sqlx::query(
        "INSERT INTO audit_events (id, timestamp, actor_label, action, target_type, metadata)
         VALUES (?1, ?2, ?3, ?4, ?5, '{}')",
    )
    .bind("11111111-1111-1111-1111-111111111111")
    .bind("2026-01-02T03:04:05Z")
    .bind("alice@example.com")
    .bind("image.upload")
    .bind("image")
    .execute(&pool)
    .await
    .expect("insert");

    // An actor that matches nothing must simply print nothing and return —
    // not error, not hang.
    audit_tail(
        &AnyPool::Sqlite(pool),
        false,
        Some("nobody@example.com".into()),
    )
    .await
    .expect("empty result is not an error");
}

#[tokio::test]
async fn audit_tail_reports_missing_table() {
    // make_pool() has no `audit_events`; the DB error must reach the caller.
    let pool = make_pool().await;
    let err = audit_tail(&AnyPool::Sqlite(pool), false, None)
        .await
        .expect_err("missing table is an error");
    assert!(matches!(err, AuditCmdError::Db(_)), "got {err:?}");
}

// ---------------------------------------------------------------------------
// open_pool — scheme dispatch
// ---------------------------------------------------------------------------

#[tokio::test]
async fn open_pool_accepts_sqlite_url() {
    // sqlx strips the `sqlite://` prefix and uses the remainder as the
    // filename, so `:memory:` keeps this test off the filesystem (a Windows
    // path would need forward slashes and is not worth the extra moving
    // parts here).
    let pool = open_pool("sqlite://:memory:")
        .await
        .expect("sqlite url opens");
    assert!(matches!(pool, AnyPool::Sqlite(_)));
}

#[tokio::test]
async fn open_pool_rejects_unknown_scheme() {
    // `AnyPool` is not Debug, so unwrap the Err by hand rather than via
    // `expect_err`.
    let err = match open_pool("mysql://localhost/picroom").await {
        Ok(_) => panic!("unknown scheme must not open a pool"),
        Err(e) => e,
    };
    assert!(
        err.to_string().contains("unknown scheme"),
        "unexpected error: {err}"
    );
}

// ---------------------------------------------------------------------------
// migrate — status against a database that was never migrated
// ---------------------------------------------------------------------------

#[tokio::test]
async fn migrate_status_propagates_missing_tracking_table() {
    let pool = make_audit_pool().await;
    let db = Database::Sqlite(pool);
    // Documented behaviour: a database with no `_sqlx_migrations` table makes
    // `migrate status` fail rather than report everything as pending, so the
    // CLI can advise running `migrate run` first.
    let err = migrate_status(&db)
        .await
        .expect_err("unmigrated database reports an error");
    assert!(err.to_string().contains("db:"), "unexpected error: {err}");
}
