// SPDX-License-Identifier: MIT
// Copyright (c) 2026 Picroom Contributors

//! `ResourceAclRepository` tests on both backends (D-11).
//!
//! - SQLite runs everywhere against an in-memory database with the schema
//!   from `migrations/0010_sqlite_resource_acls.sql`.
//! - PostgreSQL runs when `DATABASE_URL` points at a migrated database (the
//!   CI `test` job provides one; locally the test skips itself).

use picroom_auth::{AclSubject, PermissionAction};
use picroom_service::repo::{
    AclGrant, PgResourceAclRepository, ResourceAclRepository, SqliteResourceAclRepository,
};
use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};
use uuid::Uuid;

const SQLITE_SCHEMA: &str = "
CREATE TABLE IF NOT EXISTS resource_acls (
    id              TEXT PRIMARY KEY,
    resource_type   TEXT NOT NULL,
    resource_id     TEXT NOT NULL,
    subject_type    TEXT NOT NULL CHECK (subject_type IN ('user', 'team')),
    subject_id      TEXT NOT NULL,
    permission      TEXT NOT NULL
                       CHECK (permission IN ('read', 'create', 'update', 'delete', 'admin')),
    effect          TEXT NOT NULL DEFAULT 'allow' CHECK (effect IN ('allow', 'deny')),
    granted_at      TEXT NOT NULL,
    UNIQUE (resource_type, resource_id, subject_type, subject_id, permission, effect)
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
    sqlx::query(SQLITE_SCHEMA)
        .execute(&pool)
        .await
        .expect("create schema");
    pool
}

fn grants_for(user: Uuid) -> Vec<AclGrant> {
    vec![
        AclGrant::allow(AclSubject::User(user), PermissionAction::Read),
        AclGrant::deny(AclSubject::User(user), PermissionAction::Delete),
    ]
}

async fn roundtrip(repo: &dyn ResourceAclRepository, resource_id: Uuid) {
    let user = Uuid::now_v7();
    let grants = grants_for(user);

    // Empty at first.
    assert!(repo
        .list_grants("image", resource_id)
        .await
        .unwrap()
        .is_empty());

    // Replace writes the full set.
    repo.replace_grants("image", resource_id, &grants)
        .await
        .unwrap();
    let loaded = repo.list_grants("image", resource_id).await.unwrap();
    assert_eq!(loaded.len(), 2);
    assert!(loaded.contains(&grants[0]));
    assert!(loaded.contains(&grants[1]));

    // Replace is idempotent — the same set twice does not duplicate.
    repo.replace_grants("image", resource_id, &grants)
        .await
        .unwrap();
    assert_eq!(
        repo.list_grants("image", resource_id).await.unwrap().len(),
        2
    );

    // Grants are scoped per resource.
    let other = Uuid::now_v7();
    assert!(repo.list_grants("image", other).await.unwrap().is_empty());

    // Revoke removes exactly the subject's rows.
    let team = Uuid::now_v7();
    repo.replace_grants(
        "image",
        resource_id,
        &[
            grants[0].clone(),
            grants[1].clone(),
            AclGrant::allow(AclSubject::Team(team), PermissionAction::Update),
        ],
    )
    .await
    .unwrap();
    let removed = repo
        .revoke("image", resource_id, AclSubject::User(user))
        .await
        .unwrap();
    assert_eq!(removed, 2, "revoke removes exactly the subject's rows");
    let left = repo.list_grants("image", resource_id).await.unwrap();
    assert_eq!(
        left,
        vec![AclGrant::allow(
            AclSubject::Team(team),
            PermissionAction::Update
        )]
    );
}

#[tokio::test]
async fn sqlite_grants_roundtrip_replace_and_revoke() {
    let pool = sqlite_pool().await;
    let repo = SqliteResourceAclRepository::new(pool);
    roundtrip(&repo, Uuid::now_v7()).await;
}

#[tokio::test]
async fn pg_grants_roundtrip_replace_and_revoke() {
    let Ok(url) = std::env::var("DATABASE_URL") else {
        eprintln!("skipping: DATABASE_URL not set (no PostgreSQL available)");
        return;
    };
    if !url.starts_with("postgres") {
        eprintln!("skipping: DATABASE_URL is not PostgreSQL");
        return;
    }
    let Ok(pool) = sqlx::PgPool::connect(&url).await else {
        eprintln!("skipping: cannot connect to PostgreSQL at DATABASE_URL");
        return;
    };
    // The migration that adds `resource_acls` may not have run yet in the
    // target database; create the table when absent so the repository is
    // still exercised (matching migrations/0002 + 0009 shapes).
    sqlx::query(
        r"CREATE TABLE IF NOT EXISTS resource_acls (
            id              UUID PRIMARY KEY,
            resource_type   VARCHAR(64) NOT NULL,
            resource_id     UUID NOT NULL,
            subject_type    VARCHAR(32) NOT NULL CHECK (subject_type IN ('user', 'team')),
            subject_id      UUID NOT NULL,
            permission      VARCHAR(32) NOT NULL CHECK (permission IN ('read','create','update','delete','admin')),
            effect          VARCHAR(6) NOT NULL DEFAULT 'allow' CHECK (effect IN ('allow','deny')),
            granted_at      TIMESTAMPTZ NOT NULL DEFAULT NOW(),
            UNIQUE (resource_type, resource_id, subject_type, subject_id, permission, effect)
        )",
    )
    .execute(&pool)
    .await
    .expect("ensure resource_acls table");
    let repo = PgResourceAclRepository::new(pool);
    roundtrip(&repo, Uuid::now_v7()).await;
}

/// Permission vocabulary round-trips through the DB CHECK strings.
#[test]
fn permission_action_as_str_roundtrip() {
    for a in [
        PermissionAction::Read,
        PermissionAction::Create,
        PermissionAction::Update,
        PermissionAction::Delete,
        PermissionAction::Admin,
    ] {
        assert_eq!(PermissionAction::from_str_strict(a.as_str()), Some(a));
    }
}

trait FromStrStrict {
    fn from_str_strict(s: &str) -> Option<PermissionAction>;
}
impl FromStrStrict for PermissionAction {
    fn from_str_strict(s: &str) -> Option<PermissionAction> {
        // Mirrors repo::action_from_str.
        match s {
            "read" => Some(Self::Read),
            "create" => Some(Self::Create),
            "update" => Some(Self::Update),
            "delete" => Some(Self::Delete),
            "admin" => Some(Self::Admin),
            _ => None,
        }
    }
}
