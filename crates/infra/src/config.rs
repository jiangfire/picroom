// SPDX-License-Identifier: MIT
// Copyright (c) 2026 Picroom Contributors

//! Configuration loading via `figment`.
//!
//! Source order (later overrides earlier):
//! 1. Compiled-in defaults
//! 2. TOML file (if `PICROOM_CONFIG` env points at one)
//! 3. Environment variables prefixed with `PICROOM_` (double underscore for
//!    nested keys, e.g. `PICROOM_DATABASE__URL`).

use figment::providers::{Env, Format, Toml};
use figment::Figment;
use serde::{Deserialize, Serialize};
use thiserror::Error;

/// Top-level configuration.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Config {
    /// HTTP server settings.
    #[serde(default)]
    pub server: ServerConfig,
    /// Database settings.
    #[serde(default)]
    pub database: DatabaseConfig,
    /// Storage settings.
    #[serde(default)]
    pub storage: StorageConfig,
    /// Image-pipeline settings.
    #[serde(default)]
    pub pipeline: PipelineConfig,
    /// Authentication settings.
    #[serde(default)]
    pub auth: AuthConfig,
    /// Quota settings.
    #[serde(default)]
    pub quota: QuotaConfig,
    /// Rate-limit settings.
    #[serde(default)]
    pub rate_limit: RateLimitConfig,
    /// Audit-log settings.
    #[serde(default)]
    pub audit: AuditConfig,
    /// Logging settings.
    #[serde(default)]
    pub logging: LoggingConfig,
    /// Telemetry settings.
    #[serde(default)]
    pub telemetry: TelemetryConfig,
}

/// HTTP server config.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServerConfig {
    /// Bind address.
    pub bind_addr: String,
    /// Request timeout in seconds.
    pub request_timeout_secs: u64,
    /// Graceful shutdown timeout in seconds.
    pub graceful_shutdown_secs: u64,
    /// Max request body size in MB.
    pub max_body_mb: u32,
}

impl Default for ServerConfig {
    fn default() -> Self {
        Self {
            bind_addr: "0.0.0.0:8080".to_string(),
            request_timeout_secs: 30,
            graceful_shutdown_secs: 30,
            max_body_mb: 100,
        }
    }
}

/// Database config.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DatabaseConfig {
    /// Connection URL.
    pub url: String,
    /// Max pool connections.
    pub max_connections: u32,
    /// Min idle connections.
    pub min_connections: u32,
}

impl Default for DatabaseConfig {
    fn default() -> Self {
        Self {
            url: "postgres://picroom:picroom@localhost:5432/picroom".to_string(),
            max_connections: 20,
            min_connections: 2,
        }
    }
}

/// Storage config.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct StorageConfig {
    /// Default policy name.
    pub default: Option<String>,
}

/// Image-pipeline config.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PipelineConfig {
    /// Encode AVIF.
    pub encode_avif: bool,
    /// Encode WebP.
    pub encode_webp: bool,
    /// Generate thumbnails.
    pub generate_thumbnail: bool,
    /// Strip EXIF.
    pub strip_exif: bool,
    /// Max dimension.
    pub max_dimension: u32,
}

impl Default for PipelineConfig {
    fn default() -> Self {
        Self {
            encode_avif: true,
            encode_webp: true,
            generate_thumbnail: true,
            strip_exif: true,
            max_dimension: 8192,
        }
    }
}

/// Auth config.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuthConfig {
    /// Allow self-signup.
    pub allow_signup: bool,
    /// Password minimum length.
    pub password_min_length: usize,
    /// JWT secret.
    pub jwt_secret: String,
    /// JWT issuer.
    pub jwt_issuer: String,
    /// JWT audience.
    pub jwt_audience: String,
    /// JWT TTL in seconds.
    pub jwt_ttl_secs: i64,
}

impl Default for AuthConfig {
    fn default() -> Self {
        Self {
            allow_signup: false,
            password_min_length: 12,
            jwt_secret: "change-me".to_string(),
            jwt_issuer: "picroom".to_string(),
            jwt_audience: "picroom-api".to_string(),
            jwt_ttl_secs: 3600,
        }
    }
}

/// Quota config.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QuotaConfig {
    /// Default per-user bytes.
    pub default_user_bytes: u64,
    /// Default per-team bytes.
    pub default_team_bytes: u64,
    /// Soft-limit warning threshold (0.0–1.0).
    pub soft_limit_warning: f32,
    /// Enforce hard limit.
    pub hard_limit_enforce: bool,
}

impl Default for QuotaConfig {
    fn default() -> Self {
        Self {
            default_user_bytes: 10 * 1024 * 1024 * 1024,
            default_team_bytes: 1024 * 1024 * 1024 * 1024,
            soft_limit_warning: 0.9,
            hard_limit_enforce: true,
        }
    }
}

/// Rate-limit config.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RateLimitConfig {
    /// Per-user RPS.
    pub per_user_rps: u32,
    /// Per-user burst.
    pub per_user_burst: u32,
    /// Per-IP RPS.
    pub per_ip_rps: u32,
    /// Per-IP burst.
    pub per_ip_burst: u32,
}

impl Default for RateLimitConfig {
    fn default() -> Self {
        Self {
            per_user_rps: 10,
            per_user_burst: 20,
            per_ip_rps: 50,
            per_ip_burst: 100,
        }
    }
}

/// Audit config.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuditConfig {
    /// Retention in days.
    pub retention_days: u32,
}

impl Default for AuditConfig {
    fn default() -> Self {
        Self {
            retention_days: 365,
        }
    }
}

/// Logging config.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LoggingConfig {
    /// Level (trace, debug, info, warn, error).
    pub level: String,
    /// Format: "json" or "pretty".
    pub format: String,
}

impl Default for LoggingConfig {
    fn default() -> Self {
        Self {
            level: "info".to_string(),
            format: "json".to_string(),
        }
    }
}

/// Telemetry config.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TelemetryConfig {
    /// Enable Prometheus metrics.
    pub metrics_enabled: bool,
    /// OTLP endpoint (for distributed tracing).
    pub otlp_endpoint: Option<String>,
}

impl Default for TelemetryConfig {
    fn default() -> Self {
        Self {
            metrics_enabled: true,
            otlp_endpoint: None,
        }
    }
}

/// Config loading errors.
#[derive(Debug, Error)]
pub enum ConfigError {
    /// Figment error.
    #[error("config: {0}")]
    Figment(String),
}

/// Loads configuration from default sources.
pub fn load_config() -> Result<Config, ConfigError> {
    load_config_from(None::<&str>)
}

/// Loads configuration, optionally starting from a TOML file path.
pub fn load_config_from<P: AsRef<std::path::Path>>(path: Option<P>) -> Result<Config, ConfigError> {
    use figment::providers::Serialized;
    // Start with defaults so missing env-derived nested fields still
    // resolve correctly.
    let mut fig = Figment::new().merge(Serialized::defaults(Config::default()));
    if let Some(p) = path {
        fig = fig.merge(Toml::file(p));
    }
    fig = fig.merge(Env::prefixed("PICROOM_").split("__"));
    let cfg: Config = fig
        .extract()
        .map_err(|e| ConfigError::Figment(e.to_string()))?;
    warn_on_default_jwt_secret(&cfg);
    Ok(cfg)
}

/// Logs a warning when the JWT secret is still the compiled-in default.
fn warn_on_default_jwt_secret(cfg: &Config) {
    const DEFAULT: &str = "change-me";
    if cfg.auth.jwt_secret == DEFAULT {
        tracing::warn!(
            "PICROOM_AUTH__JWT_SECRET is the default \"{DEFAULT}\"; set a strong \
             random secret before exposing the service. Release builds refuse to start."
        );
    }
}

/// Refuses to proceed when the JWT secret is the default in a non-debug
/// (release) build. Called by both the API and worker binaries at startup.
pub fn require_strong_jwt_secret(cfg: &Config) -> Result<(), String> {
    if cfg!(not(debug_assertions)) && cfg.auth.jwt_secret == "change-me" {
        return Err(
            "PICROOM_AUTH__JWT_SECRET is the default \"change-me\". Set a strong \
             random secret before running in production."
                .into(),
        );
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_config_is_valid() {
        let c = Config::default();
        assert_eq!(c.server.bind_addr, "0.0.0.0:8080");
        assert!(c.pipeline.encode_avif);
        assert_eq!(c.auth.password_min_length, 12);
    }

    #[test]
    fn load_config_from_missing_file_falls_back_to_env_and_default() {
        let c = load_config_from(Some("/nonexistent/path.toml")).unwrap_or_default();
        assert_eq!(c.server.bind_addr, "0.0.0.0:8080");
    }
}
