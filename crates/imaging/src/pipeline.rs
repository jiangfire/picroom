// SPDX-License-Identifier: MIT
// Copyright (c) 2026 Picroom Contributors

//! Image processing pipeline.

use super::processor::{Processor, ProcessorError, ProcessorOutput};
use bytes::Bytes;

/// Sequential pipeline that runs processors in order with shared context.
#[derive(Default)]
pub struct Pipeline {
    processors: Vec<Box<dyn Processor>>,
}

impl Pipeline {
    /// Creates an empty pipeline.
    pub fn new() -> Self {
        Self::default()
    }

    /// Appends a processor to the pipeline.
    #[must_use]
    pub fn then<P: Processor + 'static>(mut self, processor: P) -> Self {
        self.processors.push(Box::new(processor));
        self
    }

    /// Runs the pipeline on `input`, returning the final output.
    pub async fn run(
        &self,
        ctx: &super::processor::PipelineContext,
        input: Bytes,
    ) -> Result<ProcessorOutput, ProcessorError> {
        let mut current = ProcessorOutput::Bytes(input);
        for p in &self.processors {
            let input_bytes = match current {
                ProcessorOutput::Bytes(b) => b,
                ProcessorOutput::Variant { bytes, .. } => bytes,
            };
            current = p.process(ctx, input_bytes).await?;
        }
        Ok(current)
    }

    /// Returns the number of processors in the pipeline.
    pub fn len(&self) -> usize {
        self.processors.len()
    }

    /// Returns whether the pipeline is empty.
    pub fn is_empty(&self) -> bool {
        self.processors.is_empty()
    }
}

/// Re-export so callers don't need a second `use`.
pub type PipelineContext = super::processor::PipelineContext;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::processor::probe::probe_into;
    use crate::processor::ProbeProcessor;

    /// Generates a minimal 100x80 RGB PNG (deterministic, fast).
    fn make_png() -> Bytes {
        let mut img = image::RgbImage::new(100, 80);
        for y in 0..80 {
            for x in 0..100 {
                img.put_pixel(x, y, image::Rgb([x as u8, y as u8, 128]));
            }
        }
        let mut buf: Vec<u8> = Vec::new();
        let dyn_img = image::DynamicImage::ImageRgb8(img);
        dyn_img
            .write_to(&mut std::io::Cursor::new(&mut buf), image::ImageFormat::Png)
            .unwrap();
        Bytes::from(buf)
    }

    #[tokio::test]
    async fn empty_pipeline_passes_through() {
        let p = Pipeline::new();
        let out = p
            .run(&PipelineContext::default(), Bytes::from_static(b"x"))
            .await
            .unwrap();
        match out {
            ProcessorOutput::Bytes(b) => assert_eq!(b, Bytes::from_static(b"x")),
            ProcessorOutput::Variant { .. } => panic!("expected bytes output"),
        }
    }

    #[tokio::test]
    async fn probe_processor_preserves_bytes() {
        let p = Pipeline::new().then(ProbeProcessor::new());
        let out = p
            .run(&PipelineContext::default(), Bytes::from_static(b"abc"))
            .await
            .unwrap();
        match out {
            ProcessorOutput::Bytes(b) => assert_eq!(b, Bytes::from_static(b"abc")),
            ProcessorOutput::Variant { .. } => panic!("expected bytes output"),
        }
    }

    #[tokio::test]
    async fn probe_into_extracts_dimensions_from_png() {
        let mut ctx = PipelineContext::default();
        probe_into(&mut ctx, make_png()).await.unwrap();
        assert_eq!(ctx.width, Some(100));
        assert_eq!(ctx.height, Some(80));
        assert!(ctx.mime_type.is_some());
        let mime = ctx.mime_type.as_deref().unwrap();
        assert!(mime.starts_with("image/"), "got {mime}");
    }

    #[tokio::test]
    async fn probe_into_rejects_garbage() {
        let mut ctx = PipelineContext::default();
        let err = probe_into(&mut ctx, Bytes::from_static(b"not an image"))
            .await
            .unwrap_err();
        assert!(matches!(err, ProcessorError::Decode(_)));
    }
}
