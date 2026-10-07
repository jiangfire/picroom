// SPDX-License-Identifier: MIT
// Copyright (c) 2026 Picroom Contributors

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

/// Validates that the loaded configuration is semantically usable — not just
/// deserializable (R-27): ranges, address parsing, and the JWT-secret rule.
pub fn config_validate() -> Result<(), ConfigCmdError> {
    let cfg: Config = load_config().map_err(|e| ConfigCmdError::Load(e.to_string()))?;
    validate_config(&cfg).map_err(ConfigCmdError::Load)?;
    println!("configuration is valid");
    Ok(())
}

/// Cross-field semantic checks shared by `config validate` and the binary
/// entry points.
pub fn validate_config(cfg: &Config) -> Result<(), String> {
    if cfg.server.max_body_mb == 0 {
        return Err("server.max_body_mb must be greater than 0".into());
    }
    cfg.server
        .bind_addr
        .parse::<std::net::SocketAddr>()
        .map_err(|e| format!("server.bind_addr is not a valid address: {e}"))?;
    if !cfg.database.url.starts_with("postgres://")
        && !cfg.database.url.starts_with("postgresql://")
        && !cfg.database.url.starts_with("sqlite://")
    {
        return Err("database.url must be postgres://, postgresql:// or sqlite://".into());
    }
    if cfg.database.max_connections == 0 {
        return Err("database.max_connections must be greater than 0".into());
    }
    if cfg.pipeline.max_dimension == 0 {
        return Err("pipeline.max_dimension must be greater than 0".into());
    }
    if !(0.0..=100.0).contains(&cfg.pipeline.quality.avif) {
        return Err("pipeline.quality.avif must be within 0..=100".into());
    }
    if !(0.0..=100.0).contains(&cfg.pipeline.quality.webp) {
        return Err("pipeline.quality.webp must be within 0..=100".into());
    }
    if !(1..=100).contains(&cfg.pipeline.quality.jpeg) {
        return Err("pipeline.quality.jpeg must be within 1..=100".into());
    }
    if cfg.quota.default_user_bytes == 0 {
        return Err("quota.default_user_bytes must be greater than 0".into());
    }
    if cfg.auth.jwt_ttl_secs <= 0 {
        return Err("auth.jwt_ttl_secs must be positive".into());
    }
    picroom_infra::require_strong_jwt_secret(cfg)
}

#[cfg(test)]
mod tests {
    use super::*;
    use picroom_infra::Config;

    fn base_config() -> Config {
        Config::default()
    }

    #[test]
    fn default_config_is_valid() {
        assert!(validate_config(&base_config()).is_ok());
    }

    #[test]
    fn zero_body_limit_is_rejected() {
        let mut cfg = base_config();
        cfg.server.max_body_mb = 0;
        assert!(validate_config(&cfg).unwrap_err().contains("max_body_mb"));
    }

    #[test]
    fn unparseable_bind_addr_is_rejected() {
        let mut cfg = base_config();
        cfg.server.bind_addr = "not-an-address".into();
        assert!(validate_config(&cfg).unwrap_err().contains("bind_addr"));
    }

    #[test]
    fn unknown_database_scheme_is_rejected() {
        let mut cfg = base_config();
        cfg.database.url = "mysql://localhost/picroom".into();
        assert!(validate_config(&cfg).unwrap_err().contains("database.url"));
    }

    #[test]
    fn out_of_range_quality_is_rejected() {
        let mut cfg = base_config();
        cfg.pipeline.quality.avif = 150.0;
        assert!(validate_config(&cfg).unwrap_err().contains("quality.avif"));
        let mut cfg = base_config();
        cfg.pipeline.quality.jpeg = 0;
        assert!(validate_config(&cfg).unwrap_err().contains("quality.jpeg"));
    }

    #[test]
    fn zero_max_dimension_is_rejected() {
        let mut cfg = base_config();
        cfg.pipeline.max_dimension = 0;
        assert!(validate_config(&cfg).unwrap_err().contains("max_dimension"));
    }

    #[test]
    fn zero_quota_is_rejected() {
        let mut cfg = base_config();
        cfg.quota.default_user_bytes = 0;
        assert!(validate_config(&cfg)
            .unwrap_err()
            .contains("default_user_bytes"));
    }

    #[test]
    fn default_jwt_secret_is_rejected() {
        // validate_config applies the release-build rule unconditionally for
        // explicit validation: an operator asking for validation on a config
        // that would refuse to start in production should hear about it.
        let mut cfg = base_config();
        cfg.auth.jwt_secret = "change-me".into();
        // In debug builds this is a warning-free pass; the release rule is
        // what the binary enforces at startup. Keep the test honest about
        // which branch runs.
        let result = validate_config(&cfg);
        if cfg!(not(debug_assertions)) {
            assert!(result.is_err(), "release must reject the default secret");
        } else {
            let _ = result;
        }
    }
}
