// SPDX-License-Identifier: MIT
// Copyright (c) 2026 Picroom Contributors

//! # Picroom Imaging
//!
//! Image processing pipeline.
//!
//! Provides a `Processor` trait and concrete processors for:
//!
//! - Probe (read EXIF, dimensions)
//! - Resize (max-dimension scaling)
//! - AVIF encoding
//! - WebP encoding
//! - Thumbnail generation
//! - Watermark
//!
//! A `Pipeline` runs processors sequentially with shared context.

#![allow(missing_docs)]

pub mod pipeline;
pub mod processor;

pub use pipeline::{Pipeline, PipelineContext};
pub use processor::{ProbeProcessor, Processor, ProcessorOutput};
