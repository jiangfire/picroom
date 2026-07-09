// SPDX-License-Identifier: MIT
// Copyright (c) 2026 Picroom Contributors

//! WebP encoder.

use super::{Processor, ProcessorError, ProcessorOutput};
use crate::PipelineContext;
use async_trait::async_trait;
use bytes::Bytes;

/// Encodes images to WebP.
#[derive(Debug, Clone)]
pub struct WebpProcessor {
    quality: u8,
}

impl WebpProcessor {
    /// Creates a WebP processor with the given quality (1–100).
    pub const fn new(quality: u8) -> Self {
        Self { quality }
    }

    /// Returns the configured quality.
    pub const fn quality(&self) -> u8 {
        self.quality
    }
}

#[async_trait]
impl Processor for WebpProcessor {
    fn name(&self) -> &'static str {
        "webp"
    }

    async fn process(
        &self,
        _ctx: &PipelineContext,
        input: Bytes,
    ) -> Result<ProcessorOutput, ProcessorError> {
        // Placeholder: real implementation uses `image::codecs::webp`.
        Ok(ProcessorOutput::Variant {
            kind: "webp".to_string(),
            bytes: input,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_stores_quality() {
        let p = WebpProcessor::new(80);
        assert_eq!(p.quality(), 80);
    }
}
