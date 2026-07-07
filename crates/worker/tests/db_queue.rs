//! Integration tests for the SQLite-backed `JobQueue`.
//!
//! These exercise the full enqueue → dequeue → complete → verify path
//! against an in-memory `SQLite` database with the production schema.

use async_trait::async_trait;
use bytes::Bytes;
use picroom_domain::{Image, StorageKey};
use picroom_domain::{ImageId, UserId};
use picroom_storage::driver::LocalDriver;
use picroom_storage::Storage;
use picroom_worker::{
    ImageLookup, ImageProcessor, Job, JobKind, JobQueue, JobResult, ProcessorDeps, SqliteJobQueue,
};
use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};
use sqlx::SqlitePool;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;
use time::OffsetDateTime;
use uuid::Uuid;

/// Look up an image by reading it back from storage (test-only).
struct TestLookup {
    image: Image,
}

#[async_trait]
impl ImageLookup for TestLookup {
    async fn lookup(&self, _id: ImageId) -> Result<Image, String> {
        Ok(self.image.clone())
    }
}

/// Build a minimal `SQLite` DB with the jobs schema applied.
async fn make_pool() -> SqlitePool {
    let opts: SqliteConnectOptions = SqliteConnectOptions::new()
        .filename(":memory:")
        .create_if_missing(true);
    let pool = SqlitePoolOptions::new()
        .max_connections(1)
        .connect_with(opts)
        .await
        .expect("connect to in-memory sqlite");
    sqlx::query(
        r"
        CREATE TABLE IF NOT EXISTS jobs (
            id              TEXT PRIMARY KEY,
            image_id        TEXT NOT NULL,
            kind            TEXT NOT NULL,
            payload         TEXT,
            status          TEXT NOT NULL DEFAULT 'pending',
            attempts        INTEGER NOT NULL DEFAULT 0,
            last_error      TEXT,
            enqueued_at     TEXT NOT NULL,
            started_at      TEXT,
            finished_at     TEXT
        )
        ",
    )
    .execute(&pool)
    .await
    .expect("create schema");
    pool
}

/// Build a 100x80 PNG, return (bytes, `image_metadata`).
fn make_png() -> (Bytes, u32, u32) {
    use std::io::Cursor;
    let img = image::RgbImage::from_fn(100, 80, |x, y| image::Rgb([x as u8, y as u8, 64]));
    let mut buf = Vec::new();
    image::DynamicImage::ImageRgb8(img.clone())
        .write_to(&mut Cursor::new(&mut buf), image::ImageFormat::Png)
        .unwrap();
    (Bytes::from(buf), img.width(), img.height())
}

fn tempdir() -> PathBuf {
    let base = std::env::temp_dir().join(format!("picroom-worker-{}", Uuid::now_v7()));
    std::fs::create_dir_all(&base).unwrap();
    base
}

#[tokio::test]
async fn enqueue_dequeue_complete_roundtrip() {
    let pool = make_pool().await;
    let q = SqliteJobQueue::new(pool);

    let job = Job {
        id: Uuid::now_v7(),
        image_id: ImageId(Uuid::now_v7()),
        kind: JobKind::EncodeAvif,
        attempts: 0,
        enqueued_at: OffsetDateTime::now_utc(),
    };

    q.enqueue(job.clone()).await.unwrap();

    // Dequeue returns our job.
    let dequeued = q.dequeue().await.unwrap().expect("a job");
    assert_eq!(dequeued.id, job.id);
    assert_eq!(dequeued.attempts, 1, "attempts should increment");
    assert!(matches!(dequeued.kind, JobKind::EncodeAvif));

    // No more jobs.
    assert!(q.dequeue().await.unwrap().is_none());

    // Complete.
    q.complete(
        dequeued.id,
        &JobResult::Variant {
            kind: "avif".into(),
            key: format!("img/{}/avif", dequeued.image_id.as_uuid()),
            bytes: Some(vec![1, 2, 3, 4]),
        },
    )
    .await
    .unwrap();
}

#[tokio::test]
async fn fail_eventually_marks_dead() {
    let pool = make_pool().await;
    let q = SqliteJobQueue::new(pool.clone());

    // Insert a job directly with attempts=5 so the next fail → dead.
    let job_id = Uuid::now_v7();
    sqlx::query(
        r"INSERT INTO jobs (id, image_id, kind, status, attempts, enqueued_at)
           VALUES (?1, ?2, ?3, 'pending', 5, ?4)",
    )
    .bind(job_id.to_string())
    .bind(Uuid::now_v7().to_string())
    .bind(r#"{"kind":"encode_avif"}"#)
    .bind(OffsetDateTime::now_utc().to_string())
    .execute(&pool)
    .await
    .expect("insert should succeed");

    q.fail(job_id, "transient").await.unwrap();

    let status: String = sqlx::query_scalar("SELECT status FROM jobs WHERE id = ?1")
        .bind(job_id.to_string())
        .fetch_one(&pool)
        .await
        .expect("status row should exist");
    assert_eq!(status, "dead");
}

#[tokio::test]
async fn full_pipeline_avif_roundtrip() {
    // Real end-to-end: enqueue → dequeue → ImageProcessor → verify variant
    // exists in storage.
    let pool = make_pool().await;
    let q = SqliteJobQueue::new(pool.clone());

    let tmp = tempdir();
    let storage: Arc<dyn Storage> = Arc::new(LocalDriver::new(tmp.clone(), "/i"));
    let (bytes, w, h) = make_png();

    // Persist the "original" so the processor can fetch it.
    let image_id = ImageId(Uuid::now_v7());
    let key = StorageKey::parse(&format!("img/{}.bin", image_id.as_uuid())).unwrap();
    storage.put(&key, bytes.clone()).await.unwrap();

    let image = Image {
        id: image_id,
        owner_id: UserId(Uuid::nil()),
        key: key.clone(),
        content_type: "image/png".into(),
        bytes: bytes.len() as u64,
        width: w,
        height: h,
        sha256: None,
        variants: vec![],
        created_at: OffsetDateTime::now_utc(),
    };
    let lookup: Arc<dyn ImageLookup> = Arc::new(TestLookup {
        image: image.clone(),
    });

    let deps = ProcessorDeps {
        image_lookup: lookup,
        storage: storage.clone(),
        dlq: None,
        variant_repo: None,
    };

    // Enqueue AVIF job.
    let job = Job {
        id: Uuid::now_v7(),
        image_id,
        kind: JobKind::EncodeAvif,
        attempts: 0,
        enqueued_at: OffsetDateTime::now_utc(),
    };
    q.enqueue(job.clone()).await.unwrap();

    // Worker loop (one iteration).
    let dequeued = q.dequeue().await.unwrap().unwrap();
    let result = ImageProcessor::process(&deps, dequeued.clone())
        .await
        .unwrap();

    match &result {
        JobResult::Variant {
            kind,
            key,
            bytes: Some(b),
        } => {
            assert_eq!(kind, "avif");
            assert!(!b.is_empty());
            // Key should follow `<id>/avif`.
            assert!(key.contains("avif"), "key={key}");
            // Verify the variant is actually stored.
            let stored = storage.get(&StorageKey::parse(key).unwrap()).await.unwrap();
            assert_eq!(stored.len(), b.len());
        }
        other => panic!("unexpected result: {other:?}"),
    }

    q.complete(dequeued.id, &result).await.unwrap();

    // Verify the row was marked succeeded.
    let status: String = sqlx::query_scalar("SELECT status FROM jobs WHERE id = ?1")
        .bind(dequeued.id.to_string())
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(status, "succeeded");
}

#[tokio::test]
async fn full_pipeline_webp_and_thumbnail() {
    let pool = make_pool().await;
    let q = SqliteJobQueue::new(pool);

    let tmp = tempdir();
    let storage: Arc<dyn Storage> = Arc::new(LocalDriver::new(tmp.clone(), "/i"));
    let (bytes, w, h) = make_png();
    let image_id = ImageId(Uuid::now_v7());
    let key = StorageKey::parse(&format!("img/{}.bin", image_id.as_uuid())).unwrap();
    storage.put(&key, bytes.clone()).await.unwrap();

    let image = Image {
        id: image_id,
        owner_id: UserId(Uuid::nil()),
        key: key.clone(),
        content_type: "image/png".into(),
        bytes: bytes.len() as u64,
        width: w,
        height: h,
        sha256: None,
        variants: vec![],
        created_at: OffsetDateTime::now_utc(),
    };
    let lookup: Arc<dyn ImageLookup> = Arc::new(TestLookup { image });
    let deps = ProcessorDeps {
        image_lookup: lookup,
        storage: storage.clone(),
        dlq: None,
        variant_repo: None,
    };

    // WebP.
    let webp_job = Job {
        id: Uuid::now_v7(),
        image_id,
        kind: JobKind::EncodeWebp,
        attempts: 0,
        enqueued_at: OffsetDateTime::now_utc(),
    };
    q.enqueue(webp_job.clone()).await.unwrap();
    let dequeued = q.dequeue().await.unwrap().unwrap();
    let result = ImageProcessor::process(&deps, dequeued.clone())
        .await
        .unwrap();
    q.complete(dequeued.id, &result).await.unwrap();

    // Thumbnail 200.
    let thumb_job = Job {
        id: Uuid::now_v7(),
        image_id,
        kind: JobKind::GenerateThumbnail { size: 200 },
        attempts: 0,
        enqueued_at: OffsetDateTime::now_utc(),
    };
    q.enqueue(thumb_job.clone()).await.unwrap();
    let dequeued = q.dequeue().await.unwrap().unwrap();
    let result = ImageProcessor::process(&deps, dequeued.clone())
        .await
        .unwrap();
    q.complete(dequeued.id, &result).await.unwrap();

    // Verify both variants exist on disk.
    let webp_key = StorageKey::parse(&format!("img/{}/webp", image_id.as_uuid())).unwrap();
    let thumb_key =
        StorageKey::parse(&format!("img/{}/thumbnail_200", image_id.as_uuid())).unwrap();
    let webp_bytes = storage.get(&webp_key).await.unwrap();
    let thumb_bytes = storage.get(&thumb_key).await.unwrap();
    assert!(!webp_bytes.is_empty());
    assert!(!thumb_bytes.is_empty());
}

#[tokio::test]
async fn enqueue_is_idempotent() {
    let pool = make_pool().await;
    let q = SqliteJobQueue::new(pool);

    let job = Job {
        id: Uuid::now_v7(),
        image_id: ImageId(Uuid::now_v7()),
        kind: JobKind::EncodeAvif,
        attempts: 0,
        enqueued_at: OffsetDateTime::now_utc(),
    };
    q.enqueue(job.clone()).await.unwrap();
    q.enqueue(job.clone()).await.unwrap(); // ignored by ON CONFLICT

    // Only one job remains.
    q.dequeue().await.unwrap().unwrap();
    assert!(q.dequeue().await.unwrap().is_none());
}

#[tokio::test]
async fn dequeue_is_concurrent_safe() {
    // Two consumers contend on the same row; only one should win.
    let pool = make_pool().await;
    let q = SqliteJobQueue::new(pool.clone());

    let job = Job {
        id: Uuid::now_v7(),
        image_id: ImageId(Uuid::now_v7()),
        kind: JobKind::EncodeAvif,
        attempts: 0,
        enqueued_at: OffsetDateTime::now_utc(),
    };
    q.enqueue(job.clone()).await.unwrap();

    let q1 = q.clone();
    let q2 = q.clone();
    let (r1, r2) = tokio::join!(q1.dequeue(), q2.dequeue());

    let r1 = r1.unwrap();
    let r2 = r2.unwrap();
    // Exactly one of them is Some.
    assert!(r1.is_some() ^ r2.is_some(), "both or none grabbed the job");
}

#[tokio::test]
async fn fail_returns_to_pending_until_max_attempts() {
    let pool = make_pool().await;
    let q = SqliteJobQueue::new(pool.clone());

    let job = Job {
        id: Uuid::now_v7(),
        image_id: ImageId(Uuid::now_v7()),
        kind: JobKind::EncodeAvif,
        attempts: 0,
        enqueued_at: OffsetDateTime::now_utc(),
    };
    q.enqueue(job.clone()).await.unwrap();

    // attempts starts at 0; after dequeue, increments to 1.
    let j = q.dequeue().await.unwrap().unwrap();
    assert_eq!(j.attempts, 1);
    q.fail(j.id, "boom").await.unwrap();

    // Row should be pending again (attempts < 5).
    let status: String = sqlx::query_scalar("SELECT status FROM jobs WHERE id = ?1")
        .bind(job.id.to_string())
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(status, "pending");
    // Re-dequeue → attempts now 2.
    let _ = q.dequeue().await.unwrap();
    let attempts: i32 = sqlx::query_scalar("SELECT attempts FROM jobs WHERE id = ?1")
        .bind(job.id.to_string())
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(attempts, 2);

    // Touch unused symbols to silence warnings.
    let _ = Duration::from_secs(0);
}
