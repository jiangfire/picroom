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
//! Processors run sequentially, each receiving a shared `PipelineContext`.

#![allow(missing_docs)]

pub mod processor;

pub use processor::PipelineContext;
pub use processor::{ProbeProcessor, Processor, ProcessorOutput};
