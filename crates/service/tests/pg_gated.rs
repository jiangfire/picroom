// SPDX-License-Identifier: MIT
// Copyright (c) 2026 Picroom Contributors

//! PostgreSQL-gated tests: variant upsert idempotency (R-18/D-5) and the
//! append-only guard on `audit_events` (R-17/S14). Skipped (with a note)
//! when `DATABASE_URL` is absent or unreachable — CI's `test` job provides
//! a migrated PostgreSQL instance.

use picroom_worker::processor::VariantRepository;
use uuid::Uuid;

async fn pg_pool() -> Option<sqlx::PgPool> {
    let Ok(url) = std::env::var("DATABASE_URL") else {
        eprintln!("skipping: DATABASE_URL not set");
        return None;
    };
    if !url.starts_with("postgres") {
        eprintln!("skipping: DATABASE_URL is not PostgreSQL");
        return None;
    }
    match sqlx::PgPool::connect(&url).await {
        Ok(p) => Some(p),
        Err(e) => {
            eprintln!("skipping: cannot connect to PostgreSQL ({e})");
            None
        }
    }
}

/// Creates the minimal schema when the target database has not been migrated.
///
/// The suite's tests run concurrently against one pool; identical concurrent
/// DDL can collide inside the system catalog, so the setup is serialized
/// behind a session advisory lock.
async fn ensure_schema(pool: &sqlx::PgPool) {
    const SCHEMA_LOCK: i64 = 0x7069_6365_726f_6f6d; // 'piceroom'
    sqlx::query("SELECT pg_advisory_lock($1)")
        .bind(SCHEMA_LOCK)
        .execute(pool)
        .await
        .expect("advisory lock");
    sqlx::query(
        r"CREATE TABLE IF NOT EXISTS images (
            id             UUID PRIMARY KEY,
            owner_id       UUID NOT NULL,
            team_id        UUID,
            storage_policy VARCHAR(64) NOT NULL DEFAULT 'default',
            storage_key    VARCHAR(1024) NOT NULL,
            content_type   VARCHAR(127) NOT NULL,
            bytes          BIGINT NOT NULL DEFAULT 0,
            width          INTEGER NOT NULL DEFAULT 0,
            height         INTEGER NOT NULL DEFAULT 0,
            sha256         VARCHAR(64),
            status         VARCHAR(32) NOT NULL DEFAULT 'pending',
            created_at     TIMESTAMPTZ NOT NULL DEFAULT NOW(),
            updated_at     TIMESTAMPTZ NOT NULL DEFAULT NOW()
        )",
    )
    .execute(pool)
    .await
    .unwrap();
    sqlx::query(
        r"CREATE TABLE IF NOT EXISTS image_variants (
            id              UUID PRIMARY KEY,
            image_id        UUID NOT NULL REFERENCES images(id) ON DELETE CASCADE,
            kind            VARCHAR(32) NOT NULL
                               CHECK (kind IN ('avif', 'webp', 'thumbnail', 'watermark')),
            size            INTEGER,
            storage_key     VARCHAR(1024) NOT NULL,
            bytes           BIGINT NOT NULL CHECK (bytes >= 0),
            content_type    VARCHAR(127) NOT NULL,
            created_at      TIMESTAMPTZ NOT NULL DEFAULT NOW(),
            UNIQUE (image_id, kind, size)
        )",
    )
    .execute(pool)
    .await
    .unwrap();
    sqlx::query(
        r"CREATE TABLE IF NOT EXISTS audit_events (
            id              UUID PRIMARY KEY,
            timestamp       TIMESTAMPTZ NOT NULL DEFAULT NOW(),
            actor_id        UUID,
            actor_label     VARCHAR(254),
            action          VARCHAR(64) NOT NULL,
            target_type     VARCHAR(64) NOT NULL,
            target_id       VARCHAR(64),
            ip              INET,
            user_agent      VARCHAR(512),
            metadata        JSONB
        )",
    )
    .execute(pool)
    .await
    .unwrap();
    // The natural-key index from migration 0012 (idempotent).
    sqlx::query(
        r"CREATE UNIQUE INDEX IF NOT EXISTS image_variants_natural_key
          ON image_variants (image_id, kind, COALESCE(size, -1))",
    )
    .execute(pool)
    .await
    .unwrap();
    // The append-only triggers from migration 0013 (idempotent).
    sqlx::query(
        r"CREATE OR REPLACE FUNCTION audit_events_append_only() RETURNS trigger AS $$
         BEGIN RAISE EXCEPTION 'audit_events is append-only'; END;
         $$ LANGUAGE plpgsql",
    )
    .execute(pool)
    .await
    .unwrap();
    sqlx::query(
        r"DO $$ BEGIN
             IF NOT EXISTS (SELECT 1 FROM pg_trigger WHERE tgname = 'audit_events_no_update') THEN
               CREATE TRIGGER audit_events_no_update BEFORE UPDATE ON audit_events
                 FOR EACH STATEMENT EXECUTE FUNCTION audit_events_append_only();
             END IF;
             IF NOT EXISTS (SELECT 1 FROM pg_trigger WHERE tgname = 'audit_events_no_delete') THEN
               CREATE TRIGGER audit_events_no_delete BEFORE DELETE ON audit_events
                 FOR EACH STATEMENT EXECUTE FUNCTION audit_events_append_only();
             END IF;
           END $$",
    )
    .execute(pool)
    .await
    .unwrap();
    sqlx::query("SELECT pg_advisory_unlock($1)")
        .bind(SCHEMA_LOCK)
        .execute(pool)
        .await
        .expect("advisory unlock");
}

/// R-18: enqueueing the same avif variant twice yields ONE row (size IS NULL
/// must not dodge the upsert).
#[tokio::test]
async fn avif_variant_upsert_is_idempotent() {
    let Some(pool) = pg_pool().await else { return };
    ensure_schema(&pool).await;
    let repo = picroom_service::PgVariantRepository::new(pool.clone());

    let image_id = picroom_domain::ImageId(Uuid::now_v7());
    sqlx::query("INSERT INTO images (id, owner_id, storage_key, content_type) VALUES ($1, $2, 'img/x.bin', 'image/png')")
        .bind(image_id.as_uuid())
        .bind(Uuid::now_v7())
        .execute(&pool)
        .await
        .unwrap();

    for _ in 0..2 {
        repo.insert_variant(image_id, "avif", None, "img/x/avif", 10, "image/avif")
            .await
            .unwrap();
    }
    let (count,): (i64,) =
        sqlx::query_as("SELECT COUNT(*) FROM image_variants WHERE image_id = $1 AND kind = 'avif'")
            .bind(image_id.as_uuid())
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(count, 1, "re-running the avif job must not duplicate rows");
}

/// R-17 / S14: `audit_events` rejects UPDATE and DELETE.
#[tokio::test]
async fn audit_events_reject_update_and_delete() {
    let Some(pool) = pg_pool().await else { return };
    ensure_schema(&pool).await;
    let id = Uuid::now_v7();
    sqlx::query(
        "INSERT INTO audit_events (id, action, target_type) VALUES ($1, 'auth.login', 'auth')",
    )
    .bind(id)
    .execute(&pool)
    .await
    .unwrap();

    let upd = sqlx::query("UPDATE audit_events SET action = 'tampered' WHERE id = $1")
        .bind(id)
        .execute(&pool)
        .await;
    assert!(upd.is_err(), "UPDATE on audit_events must be rejected");

    let del = sqlx::query("DELETE FROM audit_events WHERE id = $1")
        .bind(id)
        .execute(&pool)
        .await;
    assert!(del.is_err(), "DELETE on audit_events must be rejected");
}
