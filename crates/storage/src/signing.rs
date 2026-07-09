// SPDX-License-Identifier: MIT
// Copyright (c) 2026 Picroom Contributors

//! `SigV4` and other signing helpers.
//!
//! Placeholder for Phase 3 — real implementation arrives in Phase 10.
//! The `aws-sigv4` 1.x crate has refactored its public surface; we'll
//! define the higher-level `SignableRequest` wrapper in Phase 10.

/// AWS `SigV4` signing parameters (placeholder).
#[derive(Debug, Clone)]
pub struct SigV4Params {
    /// Access key id.
    pub access_key_id: String,
    /// Secret access key.
    pub secret_access_key: String,
    /// AWS region.
    pub region: String,
    /// AWS service name (e.g. `s3`).
    pub service: String,
    /// Request TTL.
    pub ttl: std::time::Duration,
}

impl SigV4Params {
    /// Creates a new params struct.
    pub fn new(
        access_key_id: impl Into<String>,
        secret_access_key: impl Into<String>,
        region: impl Into<String>,
        service: impl Into<String>,
        ttl: std::time::Duration,
    ) -> Self {
        Self {
            access_key_id: access_key_id.into(),
            secret_access_key: secret_access_key.into(),
            region: region.into(),
            service: service.into(),
            ttl,
        }
    }
}

/// Builds the canonical string-to-sign from a request (placeholder).
pub const fn canonical_request(
    method: &str,
    uri: &str,
    query: &str,
    headers: &[(&str, &str)],
    payload_hash: &str,
) -> String {
    let _ = (method, uri, query, headers, payload_hash);
    String::new()
}

/// `SigV4` verification (placeholder — real impl in Phase 10).
pub fn verify(_params: &SigV4Params) -> Result<(), String> {
    Err("not implemented (skeleton)".to_string())
}
