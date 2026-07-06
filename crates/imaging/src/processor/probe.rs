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
pub async fn probe_into(
    ctx: &mut PipelineContext,
    input: Bytes,
) -> Result<(), ProcessorError> {
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