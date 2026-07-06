//! AVIF encoder.

use super::{Processor, ProcessorError, ProcessorOutput};
use crate::PipelineContext;
use async_trait::async_trait;
use bytes::Bytes;

/// Encodes images to AVIF.
#[derive(Debug, Clone)]
pub struct AvifProcessor {
    quality: u8,
    speed: u8,
}

impl AvifProcessor {
    /// Creates an AVIF processor with the given quality (1–100) and speed (0–10).
    pub const fn new(quality: u8, speed: u8) -> Self {
        Self { quality, speed }
    }

    /// Returns the configured quality.
    pub const fn quality(&self) -> u8 {
        self.quality
    }

    /// Returns the configured speed.
    pub const fn speed(&self) -> u8 {
        self.speed
    }
}

#[async_trait]
impl Processor for AvifProcessor {
    fn name(&self) -> &'static str {
        "avif"
    }

    async fn process(
        &self,
        _ctx: &PipelineContext,
        input: Bytes,
    ) -> Result<ProcessorOutput, ProcessorError> {
        // Placeholder: real implementation uses `ravif::Encoder`.
        Ok(ProcessorOutput::Variant {
            kind: "avif".to_string(),
            bytes: input,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_stores_quality_and_speed() {
        let p = AvifProcessor::new(60, 6);
        assert_eq!(p.quality(), 60);
        assert_eq!(p.speed(), 6);
    }
}
