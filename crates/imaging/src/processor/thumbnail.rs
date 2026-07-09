// SPDX-License-Identifier: MIT
// Copyright (c) 2026 Picroom Contributors

//! Thumbnail processor — generates multiple smaller variants.

use super::{Processor, ProcessorError, ProcessorOutput};
use crate::PipelineContext;
use async_trait::async_trait;
use bytes::Bytes;

/// Generates thumbnails at the configured sizes.
#[derive(Debug, Clone)]
pub struct ThumbnailProcessor {
    sizes: Vec<u32>,
}

impl ThumbnailProcessor {
    /// Creates a thumbnail processor with the given sizes (e.g. `[200, 400, 800]`).
    pub const fn new(sizes: Vec<u32>) -> Self {
        Self { sizes }
    }

    /// Returns the configured sizes.
    pub fn sizes(&self) -> &[u32] {
        &self.sizes
    }
}

#[async_trait]
impl Processor for ThumbnailProcessor {
    fn name(&self) -> &'static str {
        "thumbnail"
    }

    async fn process(
        &self,
        _ctx: &PipelineContext,
        input: Bytes,
    ) -> Result<ProcessorOutput, ProcessorError> {
        // Placeholder: real implementation runs `ResizeProcessor` per size.
        Ok(ProcessorOutput::Bytes(input))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_stores_sizes() {
        let p = ThumbnailProcessor::new(vec![200, 400, 800]);
        assert_eq!(p.sizes(), &[200, 400, 800]);
    }
}
