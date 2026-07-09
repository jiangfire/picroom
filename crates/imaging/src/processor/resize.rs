// SPDX-License-Identifier: MIT
// Copyright (c) 2026 Picroom Contributors

//! Resize processor.

use super::{Processor, ProcessorError, ProcessorOutput};
use crate::PipelineContext;
use async_trait::async_trait;
use bytes::Bytes;
use image::ImageReader;
use std::io::Cursor;

/// Resizes images to a maximum dimension, preserving aspect ratio.
///
/// If the image is already smaller than `max_dimension`, it is returned
/// unchanged.
#[derive(Debug, Clone)]
pub struct ResizeProcessor {
    max_dimension: u32,
}

impl ResizeProcessor {
    /// Creates a resize processor with the given maximum dimension.
    pub const fn new(max_dimension: u32) -> Self {
        Self { max_dimension }
    }

    /// Returns the configured maximum dimension.
    pub const fn max_dimension(&self) -> u32 {
        self.max_dimension
    }
}

#[async_trait]
impl Processor for ResizeProcessor {
    fn name(&self) -> &'static str {
        "resize"
    }

    async fn process(
        &self,
        _ctx: &PipelineContext,
        input: Bytes,
    ) -> Result<ProcessorOutput, ProcessorError> {
        let max = self.max_dimension;
        let bytes = input.clone();
        let out = tokio::task::spawn_blocking(move || -> Result<Bytes, String> {
            let reader = ImageReader::new(Cursor::new(bytes.as_ref()))
                .with_guessed_format()
                .map_err(|e| e.to_string())?;
            let format = reader.format();
            let dims = reader.into_dimensions().map_err(|e| e.to_string())?;
            let (w, h) = (dims.0, dims.1);

            // Skip if already small enough.
            if w <= max && h <= max {
                return Ok(bytes);
            }

            let img = image::load_from_memory(&bytes).map_err(|e| e.to_string())?;
            let (new_w, new_h) = if w >= h {
                let scale = max as f32 / w as f32;
                (max, ((h as f32) * scale).round() as u32)
            } else {
                let scale = max as f32 / h as f32;
                (((w as f32) * scale).round() as u32, max)
            };

            let resized = img.resize_exact(new_w, new_h, image::imageops::FilterType::Lanczos3);

            // Re-encode in the original format if known; otherwise PNG.
            let mut buf = Vec::with_capacity(bytes.len());
            match format {
                Some(image::ImageFormat::Jpeg) => {
                    resized
                        .to_rgb8()
                        .write_to(&mut Cursor::new(&mut buf), image::ImageFormat::Jpeg)
                        .map_err(|e| e.to_string())?;
                }
                Some(image::ImageFormat::WebP) => {
                    resized
                        .to_rgb8()
                        .write_to(&mut Cursor::new(&mut buf), image::ImageFormat::WebP)
                        .map_err(|e| e.to_string())?;
                }
                _ => {
                    resized
                        .write_to(&mut Cursor::new(&mut buf), image::ImageFormat::Png)
                        .map_err(|e| e.to_string())?;
                }
            }
            Ok(Bytes::from(buf))
        })
        .await
        .map_err(|e| ProcessorError::Internal(format!("join: {e}")))?
        .map_err(ProcessorError::Encode)?;

        Ok(ProcessorOutput::Bytes(out))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::GenericImageView;

    #[test]
    fn new_stores_max_dimension() {
        let p = ResizeProcessor::new(8192);
        assert_eq!(p.max_dimension(), 8192);
    }

    fn make_png(w: u32, h: u32) -> Bytes {
        let img = image::RgbImage::from_fn(w, h, |x, y| image::Rgb([x as u8, y as u8, 64]));
        let mut buf = Vec::new();
        let dyn_img = image::DynamicImage::ImageRgb8(img);
        dyn_img
            .write_to(&mut Cursor::new(&mut buf), image::ImageFormat::Png)
            .unwrap();
        Bytes::from(buf)
    }

    #[tokio::test]
    async fn resize_downscales_to_max_dimension() {
        let p = ResizeProcessor::new(50);
        let original = make_png(200, 100);
        let out = p
            .process(&PipelineContext::default(), original.clone())
            .await
            .unwrap();
        let bytes = match out {
            ProcessorOutput::Bytes(b) => b,
            ProcessorOutput::Variant { .. } => panic!("expected bytes"),
        };
        let dims = image::load_from_memory(&bytes).unwrap().dimensions();
        assert_eq!(dims, (50, 25));
    }

    #[tokio::test]
    async fn resize_keeps_smaller_unchanged() {
        let p = ResizeProcessor::new(8192);
        let original = make_png(50, 50);
        let out = p
            .process(&PipelineContext::default(), original.clone())
            .await
            .unwrap();
        let bytes = match out {
            ProcessorOutput::Bytes(b) => b,
            ProcessorOutput::Variant { .. } => panic!("expected bytes"),
        };
        // Should be byte-identical since already small enough.
        assert_eq!(bytes, original);
    }
}
