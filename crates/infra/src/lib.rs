//! # Picroom Infra
//!
//! Infrastructure layer: configuration loading, DB pools, logging,
//! telemetry, ID generation, time.

#![warn(missing_docs)]

pub mod cache;
pub mod clock;
pub mod config;
pub mod db;
pub mod id;
pub mod logging;
pub mod telemetry;

pub use cache::Cache;
pub use clock::SystemClock;
pub use config::{load_config, load_config_from, Config};
pub use db::{Database, DbError};
pub use id::IdGenerator;
pub use logging::init_logging;
pub use telemetry::init_metrics;
