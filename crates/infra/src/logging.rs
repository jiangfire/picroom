//! Logging initialisation.

use tracing_subscriber::{fmt, prelude::*, EnvFilter};

/// Initialises structured logging.
///
/// Honours `RUST_LOG` (e.g. `info`, `picroom=debug,sqlx=warn`).
pub fn init_logging(level: &str, format: &str) {
    let env_filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new(level));

    let registry = tracing_subscriber::registry().with(env_filter);

    match format {
        "pretty" => {
            registry.with(fmt::layer().pretty()).init();
        }
        _ => {
            registry.with(fmt::layer().json()).init();
        }
    }
}
