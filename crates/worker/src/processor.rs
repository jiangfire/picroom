// SPDX-License-Identifier: MIT
// Copyright (c) 2026 Picroom Contributors

//! Job processor: turns an `ImageJob` into one or more stored variants.

use crate::dlq::{DlqEntry, DlqSink};
use crate::job::{Job, JobKind, JobResult};
use async_trait::async_trait;
use bytes::Bytes;
use picroom_domain::{Image, ImageId, StorageKey};
use picroom_storage::Storage;
use std::sync::Arc;
use time::OffsetDateTime;

/// Variant repository — persists variant metadata to DB.
#[async_trait]
pub trait VariantRepository: Send + Sync {
    /// Inserts a variant record for the given image.
    async fn insert_variant(
        &self,
        image_id: ImageId,
        kind: &str,
        size: Option<u32>,
        storage_key: &str,
        bytes: u64,
        content_type: &str,
    ) -> Result<(), String>;
}

/// Dependencies required by the image job processor.
pub struct ProcessorDeps {
    /// Image repository (`get` only — read metadata + storage key).
    pub image_lookup: Arc<dyn ImageLookup>,
    /// Storage (full capabilities).
    pub storage: Arc<dyn Storage>,
    /// Optional DLQ sink.
    pub dlq: Option<Arc<dyn DlqSink>>,
    /// Optional variant repository (writes `image_variants` table).
    pub variant_repo: Option<Arc<dyn VariantRepository + Send + Sync>>,
}

/// Minimal lookup the processor needs (avoids coupling to `picroom-service`).
#[async_trait]
pub trait ImageLookup: Send + Sync {
    /// Loads an image by id, returning the storage key + content type.
    async fn lookup(&self, id: ImageId) -> Result<Image, String>;
}

/// Image job processor.
pub struct ImageProcessor;

impl ImageProcessor {
    /// Creates a new processor.
    pub const fn new() -> Self {
        Self
    }

    /// Processes a single job, producing a `JobResult`.
    pub async fn process(deps: &ProcessorDeps, job: Job) -> Result<JobResult, String> {
        match &job.kind {
            JobKind::EncodeAvif => {
                encode_variant(deps, &job, "avif", None, "avif", Box::new(avif_encode)).await
            }
            JobKind::EncodeWebp => {
                encode_variant(deps, &job, "webp", None, "webp", Box::new(webp_encode)).await
            }
            JobKind::GenerateThumbnail { size } => {
                let size = *size;
                let enc: Encoder = Box::new(move |img| thumbnail_encode(img, size));
                // DB identity is `kind='thumbnail', size=<n>` (the CHECK
                // constraint on `image_variants.kind` admits `thumbnail` only);
                // the storage key keeps the size suffix so thumbnails of
                // different sizes do not collide.
                let key_name = format!("thumbnail_{size}");
                encode_variant(deps, &job, "thumbnail", Some(size), &key_name, enc).await
            }
            JobKind::ApplyWatermark => Err("watermark not yet implemented".into()),
            JobKind::StripExif => Err("strip-exif not yet implemented".into()),
        }
    }
}

impl Default for ImageProcessor {
    fn default() -> Self {
        Self::new()
    }
}

type Encoder = Box<dyn Fn(&image::DynamicImage) -> Result<Bytes, String> + Send + Sync>;

async fn encode_variant(
    deps: &ProcessorDeps,
    job: &Job,
    kind: &str,
    size: Option<u32>,
    key_name: &str,
    encoder: Encoder,
) -> Result<JobResult, String> {
    let image = deps
        .image_lookup
        .lookup(job.image_id)
        .await
        .map_err(|e| format!("lookup {}: {e}", job.image_id))?;

    // Load original bytes.
    let original = deps
        .storage
        .get(&image.key)
        .await
        .map_err(|e| format!("storage get: {e}"))?;

    // Decode for re-encode.
    let decoded =
        image::load_from_memory(&original).map_err(|e| format!("decode original: {e}"))?;

    let bytes = tokio::task::spawn_blocking(move || encoder(&decoded))
        .await
        .map_err(|e| format!("join: {e}"))?
        .map_err(|e| format!("encode: {e}"))?;

    // Persist variant to storage.
    let key = variant_key(&image, key_name)?;
    deps.storage
        .put(&key, bytes.clone())
        .await
        .map_err(|e| format!("storage put: {e}"))?;

    // Determine content type for this variant.
    let content_type = match kind {
        "avif" => "image/avif",
        "webp" => "image/webp",
        _ => "image/jpeg", // thumbnails are JPEG
    };

    // Persist variant metadata to DB. A failed insert must fail the job so it
    // retries or dead-letters — bytes in storage with no visible row are the
    // "success that isn't" this code path exists to prevent.
    if let Some(repo) = &deps.variant_repo {
        repo.insert_variant(
            job.image_id,
            kind,
            size,
            key.as_str(),
            bytes.len() as u64,
            content_type,
        )
        .await
        .map_err(|e| format!("insert image_variant row: {e}"))?;
    }

    Ok(JobResult::Variant {
        kind: kind.to_string(),
        key: key.to_string(),
        bytes: Some(bytes.to_vec()),
    })
}

fn variant_key(image: &Image, name: &str) -> Result<StorageKey, String> {
    let id = image.id.as_uuid();
    let key = format!("img/{id}/{name}");
    StorageKey::parse(&key).map_err(|e| format!("invalid variant key \"{key}\": {e}"))
}

fn avif_encode(img: &image::DynamicImage) -> Result<Bytes, String> {
    use ravif::{Img, RGB8};
    let w = img.width() as usize;
    let h = img.height() as usize;
    let rgb = img.to_rgb8();
    let pixels: Vec<RGB8> = rgb
        .pixels()
        .map(|p| {
            let ch = p.0;
            RGB8 {
                r: ch[0],
                g: ch[1],
                b: ch[2],
            }
        })
        .collect();
    let enc = ravif::Encoder::new()
        .with_quality(60.0)
        .with_speed(6)
        .encode_rgb(Img::new(pixels.as_slice(), w, h))
        .map_err(|e| format!("ravif encode: {e:?}"))?;
    Ok(Bytes::from(enc.avif_file))
}

fn webp_encode(img: &image::DynamicImage) -> Result<Bytes, String> {
    let mut out = Vec::new();
    let mut cur = std::io::Cursor::new(&mut out);
    img.to_rgb8()
        .write_to(&mut cur, image::ImageFormat::WebP)
        .map_err(|e| e.to_string())?;
    Ok(Bytes::from(out))
}

fn thumbnail_encode(img: &image::DynamicImage, size: u32) -> Result<Bytes, String> {
    let resized = img.resize(size, size, image::imageops::FilterType::Triangle);
    let mut out = Vec::new();
    let mut cur = std::io::Cursor::new(&mut out);
    resized
        .to_rgb8()
        .write_to(&mut cur, image::ImageFormat::Jpeg)
        .map_err(|e| e.to_string())?;
    Ok(Bytes::from(out))
}

/// Helper: build a `DlqEntry` for a failed job.
pub fn make_dlq_entry(job: &Job, error: String) -> DlqEntry {
    DlqEntry {
        job_id: job.id,
        error,
        attempts: job.attempts,
        moved_at: OffsetDateTime::now_utc(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dlq::InMemoryDlq;
    use picroom_storage::driver::LocalDriver;
    use std::path::PathBuf;

    #[test]
    fn variant_key_uses_id_and_kind() {
        let id = uuid::Uuid::now_v7();
        let img = Image {
            id: ImageId(id),
            owner_id: picroom_domain::UserId(uuid::Uuid::nil()),
            team_id: None,
            key: picroom_domain::StorageKey::parse("img/x.bin").unwrap(),
            content_type: "image/png".into(),
            bytes: 1,
            width: 100,
            height: 100,
            sha256: None,
            variants: vec![],
            created_at: time::OffsetDateTime::now_utc(),
        };
        let k = variant_key(&img, "avif").expect("valid key");
        assert_eq!(k.as_str(), &format!("img/{id}/avif"));
    }

    /// Recorded `insert_variant` calls: `(kind, size)` pairs.
    type RecordedCalls = Arc<std::sync::Mutex<Vec<(String, Option<u32>)>>>;

    /// Records `insert_variant` calls; optionally fails.
    #[derive(Default, Clone)]
    struct RecordingRepo {
        calls: RecordedCalls,
        fail: bool,
    }

    #[async_trait]
    impl VariantRepository for RecordingRepo {
        async fn insert_variant(
            &self,
            _image_id: ImageId,
            kind: &str,
            size: Option<u32>,
            _storage_key: &str,
            _bytes: u64,
            _content_type: &str,
        ) -> Result<(), String> {
            if self.fail {
                return Err("forced insert failure".into());
            }
            self.calls
                .lock()
                .expect("mutex poisoned")
                .push((kind.to_string(), size));
            Ok(())
        }
    }

    fn test_image(png: Bytes) -> (Image, Arc<dyn Storage>) {
        let dir: PathBuf =
            std::env::temp_dir().join(format!("picroom-proc-{}", uuid::Uuid::now_v7()));
        std::fs::create_dir_all(&dir).unwrap();
        let storage: Arc<dyn Storage> = Arc::new(LocalDriver::new(dir, "/i"));
        let id = uuid::Uuid::now_v7();
        let key = StorageKey::parse("img/test-original.png").unwrap();
        let img = Image {
            id: ImageId(id),
            owner_id: picroom_domain::UserId(uuid::Uuid::nil()),
            team_id: None,
            key: key.clone(),
            content_type: "image/png".into(),
            bytes: png.len() as u64,
            width: 100,
            height: 80,
            sha256: None,
            variants: vec![],
            created_at: OffsetDateTime::now_utc(),
        };
        // Store the original so the processor can read it back.
        futures::executor::block_on(storage.put(&key, png)).unwrap();
        (img, storage)
    }

    fn png_bytes() -> Bytes {
        use std::io::Cursor;
        let img = image::RgbImage::from_fn(100, 80, |x, y| image::Rgb([x as u8, y as u8, 64]));
        let mut buf = Vec::new();
        image::DynamicImage::ImageRgb8(img)
            .write_to(&mut Cursor::new(&mut buf), image::ImageFormat::Png)
            .unwrap();
        Bytes::from(buf)
    }

    fn deps_for(
        img: &Image,
        storage: Arc<dyn Storage>,
        repo: Arc<dyn VariantRepository + Send + Sync>,
    ) -> ProcessorDeps {
        struct Lookup(Image);
        #[async_trait]
        impl ImageLookup for Lookup {
            async fn lookup(&self, _id: ImageId) -> Result<Image, String> {
                Ok(self.0.clone())
            }
        }
        ProcessorDeps {
            image_lookup: Arc::new(Lookup(img.clone())),
            storage,
            dlq: Some(Arc::new(InMemoryDlq::new())),
            variant_repo: Some(repo),
        }
    }

    /// R-02: the thumbnail row identity must be `kind='thumbnail'` with the
    /// size carried in `size` — `thumbnail_200` violates the DB CHECK
    /// constraint, so the row insert failed and the job reported success.
    #[tokio::test]
    async fn thumbnail_job_persists_row_as_kind_thumbnail_with_size() {
        let (img, storage) = test_image(png_bytes());
        let repo = Arc::new(RecordingRepo::default());
        let deps = deps_for(&img, storage, repo.clone());
        let job = Job {
            id: uuid::Uuid::now_v7(),
            image_id: img.id,
            kind: JobKind::GenerateThumbnail { size: 200 },
            attempts: 0,
            enqueued_at: OffsetDateTime::now_utc(),
        };
        let result = ImageProcessor::process(&deps, job).await.expect("job ok");
        let calls = repo.calls.lock().expect("mutex poisoned");
        assert_eq!(*calls, vec![("thumbnail".to_string(), Some(200))]);
        let JobResult::Variant { kind, key, .. } = result else {
            panic!("expected a Variant result");
        };
        assert_eq!(kind, "thumbnail");
        assert!(
            key.ends_with("/thumbnail_200"),
            "storage key keeps the size: {key}"
        );
    }

    /// R-02: a failed variant-row insert must fail the job (retry/DLQ), not
    /// be swallowed as a warning while the job reports success.
    #[tokio::test]
    async fn insert_failure_fails_the_job() {
        let (img, storage) = test_image(png_bytes());
        let repo = Arc::new(RecordingRepo {
            fail: true,
            calls: <RecordedCalls as Default>::default(),
        });
        let deps = deps_for(&img, storage, repo);
        let job = Job {
            id: uuid::Uuid::now_v7(),
            image_id: img.id,
            kind: JobKind::GenerateThumbnail { size: 400 },
            attempts: 0,
            enqueued_at: OffsetDateTime::now_utc(),
        };
        let result = ImageProcessor::process(&deps, job).await;
        assert!(
            result.is_err(),
            "insert failure must surface as a job error"
        );
    }
}
