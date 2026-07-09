// SPDX-License-Identifier: MIT
// Copyright (c) 2026 Picroom Contributors

//! # Picroom Infra
//!
//! Infrastructure layer: configuration loading, DB pools, logging,
//! telemetry, ID generation, time.

#![allow(missing_docs)]

pub mod cache;
pub mod clock;
pub mod config;
pub mod db;
pub mod id;
pub mod logging;
pub mod telemetry;

pub use cache::Cache;
pub use clock::SystemClock;
pub use config::{load_config, load_config_from, require_strong_jwt_secret, Config};
pub use db::{Database, DbError};
pub use id::IdGenerator;
pub use logging::init_logging;
pub use telemetry::{init_metrics, render_metrics};
