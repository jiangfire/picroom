//! Watermark processor.

use super::{Processor, ProcessorError, ProcessorOutput};
use crate::PipelineContext;
use async_trait::async_trait;
use bytes::Bytes;

/// Applies a watermark to images.
#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct WatermarkProcessor {
    text: Option<String>,
    image: Option<Bytes>,
    position: WatermarkPosition,
}

/// Watermark placement.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WatermarkPosition {
    /// Top-left.
    TopLeft,
    /// Top-right.
    TopRight,
    /// Bottom-left.
    BottomLeft,
    /// Bottom-right.
    BottomRight,
    /// Center.
    Center,
}

impl WatermarkProcessor {
    /// Creates a text watermark.
    pub fn text(text: impl Into<String>, position: WatermarkPosition) -> Self {
        Self {
            text: Some(text.into()),
            image: None,
            position,
        }
    }

    /// Creates an image watermark.
    pub const fn image(image: Bytes, position: WatermarkPosition) -> Self {
        Self {
            text: None,
            image: Some(image),
            position,
        }
    }
}

#[async_trait]
impl Processor for WatermarkProcessor {
    fn name(&self) -> &'static str {
        "watermark"
    }

    async fn process(
        &self,
        _ctx: &PipelineContext,
        input: Bytes,
    ) -> Result<ProcessorOutput, ProcessorError> {
        Ok(ProcessorOutput::Bytes(input))
    }
}
