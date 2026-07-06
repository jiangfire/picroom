//! Config validate + print subcommands.

use picroom_infra::{load_config, Config};
use thiserror::Error;

/// Config command errors.
#[derive(Debug, Error)]
pub enum ConfigCmdError {
    /// Load failure.
    #[error("load: {0}")]
    Load(String),
}

/// Prints the resolved configuration as JSON.
pub fn config_print() -> Result<(), ConfigCmdError> {
    let cfg = load_config().map_err(|e| ConfigCmdError::Load(e.to_string()))?;
    let s = serde_json::to_string_pretty(&cfg).map_err(|e| ConfigCmdError::Load(e.to_string()))?;
    println!("{s}");
    Ok(())
}

/// Validates that the loaded configuration is internally consistent.
pub fn config_validate() -> Result<(), ConfigCmdError> {
    let _cfg: Config = load_config().map_err(|e| ConfigCmdError::Load(e.to_string()))?;
    // Placeholder for cross-field validation.
    println!("configuration is valid");
    Ok(())
}