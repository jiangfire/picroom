//! Image processors.

pub mod avif;
pub mod probe;
pub mod resize;
pub mod thumbnail;
pub mod watermark;
pub mod webp;

pub use avif::AvifProcessor;
pub use probe::ProbeProcessor;
pub use resize::ResizeProcessor;
pub use thumbnail::ThumbnailProcessor;
pub use watermark::WatermarkProcessor;
pub use webp::WebpProcessor;

use async_trait::async_trait;
use bytes::Bytes;
use thiserror::Error;
use uuid::Uuid;

/// Output of a processor.
#[derive(Debug, Clone)]
pub enum ProcessorOutput {
    /// Pass-through bytes (probe, metadata-only).
    Bytes(Bytes),
    /// Produced an encoded image (variant).
    Variant {
        /// Variant kind (avif, webp, thumb, …).
        kind: String,
        /// Variant bytes.
        bytes: Bytes,
    },
}

/// Image-processing errors.
#[derive(Debug, Error)]
pub enum ProcessorError {
    /// Image format not supported.
    #[error("unsupported format: {0}")]
    UnsupportedFormat(String),
    /// Decode error.
    #[error("decode error: {0}")]
    Decode(String),
    /// Encode error.
    #[error("encode error: {0}")]
    Encode(String),
    /// I/O error.
    #[error("io error: {0}")]
    Io(String),
    /// Generic internal error.
    #[error("internal: {0}")]
    Internal(String),
}

impl ProcessorError {
    /// Convert to a string suitable for logging.
    pub fn as_log(&self) -> String {
        format!("{self}")
    }
}

/// Trait implemented by every image processor.
#[async_trait]
pub trait Processor: Send + Sync {
    /// Returns a stable identifier used in pipeline configuration.
    fn name(&self) -> &'static str;

    /// Runs the processor on `input`, returning the output.
    async fn process(
        &self,
        ctx: &PipelineContext,
        input: Bytes,
    ) -> Result<ProcessorOutput, ProcessorError>;
}

/// Pipeline context propagated through processors.
#[derive(Debug, Clone, Default)]
pub struct PipelineContext {
    /// Source image ID (`UUIDv7`).
    pub image_id: Option<Uuid>,
    /// Storage key for the original.
    pub original_key: Option<String>,
    /// Detected MIME type.
    pub mime_type: Option<String>,
    /// Detected width (post-probe).
    pub width: Option<u32>,
    /// Detected height (post-probe).
    pub height: Option<u32>,
}
