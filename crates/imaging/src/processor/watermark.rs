// SPDX-License-Identifier: MIT
// Copyright (c) 2026 Picroom Contributors

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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn text_watermark_records_text_and_position() {
        let w = WatermarkProcessor::text("hello", WatermarkPosition::BottomRight);
        assert_eq!(w.text.as_deref(), Some("hello"));
        assert!(w.image.is_none());
        assert_eq!(w.position, WatermarkPosition::BottomRight);
    }

    #[test]
    fn image_watermark_records_bytes_and_position() {
        let data = Bytes::from_static(b"img");
        let w = WatermarkProcessor::image(data.clone(), WatermarkPosition::Center);
        assert_eq!(w.image.as_deref(), Some(&data[..]));
        assert!(w.text.is_none());
        assert_eq!(w.position, WatermarkPosition::Center);
    }

    #[test]
    fn name_reports_watermark() {
        let w = WatermarkProcessor::text("x", WatermarkPosition::TopLeft);
        assert_eq!(w.name(), "watermark");
    }

    #[tokio::test]
    async fn process_passthrough_returns_input() {
        let w = WatermarkProcessor::text("x", WatermarkPosition::TopRight);
        let bytes = Bytes::from_static(b"payload");
        let out = w
            .process(&PipelineContext::default(), bytes.clone())
            .await
            .unwrap();
        match out {
            ProcessorOutput::Bytes(b) => assert_eq!(b, bytes),
            ProcessorOutput::Variant { .. } => panic!("expected bytes"),
        }
    }
}
