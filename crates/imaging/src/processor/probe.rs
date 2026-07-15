// SPDX-License-Identifier: MIT
// Copyright (c) 2026 Picroom Contributors

//! Probe processor — reads image metadata.
//!
//! Populates `PipelineContext::width`, `height`, `mime_type` from the
//! image header. CPU-bound work runs on a blocking task so it does not
//! stall the runtime.

use super::{Processor, ProcessorError, ProcessorOutput};
use crate::PipelineContext;
use async_trait::async_trait;
use bytes::Bytes;
use image::ImageReader;
use std::io::Cursor;

/// Reads image dimensions and format.
#[derive(Debug, Default, Clone)]
pub struct ProbeProcessor;

impl ProbeProcessor {
    /// Creates a new probe processor.
    pub const fn new() -> Self {
        Self
    }
}

#[async_trait]
impl Processor for ProbeProcessor {
    fn name(&self) -> &'static str {
        "probe"
    }

    async fn process(
        &self,
        _ctx: &PipelineContext,
        input: Bytes,
    ) -> Result<ProcessorOutput, ProcessorError> {
        // Probe is non-fatal: pass-through on decode failure so the
        // pipeline can still continue. Use `probe_into` for strict
        // validation.
        let bytes = input.clone();
        let _ = tokio::task::spawn_blocking(move || -> Result<(), String> {
            let reader = ImageReader::new(Cursor::new(bytes.as_ref()))
                .with_guessed_format()
                .map_err(|e| e.to_string())?;
            let _ = reader.format();
            let _ = reader.into_dimensions();
            Ok(())
        })
        .await;

        Ok(ProcessorOutput::Bytes(input))
    }
}

/// Probe an image and return its dimensions + MIME hint, mutating `ctx`.
/// Exposed separately for callers that want to update the context
/// without running the full pipeline.
pub async fn probe_into(ctx: &mut PipelineContext, input: Bytes) -> Result<(), ProcessorError> {
    let bytes = input;
    let info = tokio::task::spawn_blocking(move || -> Result<(u32, u32, String), String> {
        let reader = ImageReader::new(Cursor::new(bytes.as_ref()))
            .with_guessed_format()
            .map_err(|e| e.to_string())?;
        let format = reader.format().ok_or("unknown format")?;
        let ext = format!("image/{format:?}").to_lowercase();
        let dims = reader.into_dimensions().map_err(|e| e.to_string())?;
        Ok((dims.0, dims.1, ext))
    })
    .await
    .map_err(|e| ProcessorError::Internal(format!("join: {e}")))?
    .map_err(ProcessorError::Decode)?;

    ctx.width = Some(info.0);
    ctx.height = Some(info.1);
    ctx.mime_type = Some(info.2);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::RgbImage;

    fn make_png(w: u32, h: u32) -> Bytes {
        let img = RgbImage::from_fn(w, h, |x, y| image::Rgb([x as u8, y as u8, 64]));
        let dyn_img = image::DynamicImage::ImageRgb8(img);
        let mut buf = Vec::new();
        dyn_img
            .write_to(&mut Cursor::new(&mut buf), image::ImageFormat::Png)
            .unwrap();
        Bytes::from(buf)
    }

    #[test]
    fn processor_name_and_new() {
        let p = ProbeProcessor::new();
        assert_eq!(p.name(), "probe");
    }

    #[tokio::test]
    async fn process_passthrough_returns_input() {
        let p = ProbeProcessor::new();
        let bytes = make_png(4, 4);
        let out = p
            .process(&PipelineContext::default(), bytes.clone())
            .await
            .unwrap();
        match out {
            ProcessorOutput::Bytes(b) => assert_eq!(b, bytes),
            ProcessorOutput::Variant { .. } => panic!("expected bytes"),
        }
    }

    #[tokio::test]
    async fn probe_into_populates_context() {
        let mut ctx = PipelineContext::default();
        let bytes = make_png(8, 4);
        probe_into(&mut ctx, bytes).await.unwrap();
        assert_eq!(ctx.width, Some(8));
        assert_eq!(ctx.height, Some(4));
        assert_eq!(ctx.mime_type.as_deref(), Some("image/png"));
    }

    #[tokio::test]
    async fn probe_into_rejects_garbage() {
        let mut ctx = PipelineContext::default();
        let res = probe_into(&mut ctx, Bytes::from_static(b"not an image")).await;
        assert!(res.is_err());
        // Context is left untouched on failure.
        assert_eq!(ctx.width, None);
        assert_eq!(ctx.height, None);
        assert_eq!(ctx.mime_type, None);
    }
}
