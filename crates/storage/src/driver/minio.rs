// SPDX-License-Identifier: MIT
// Copyright (c) 2026 Picroom Contributors

//! `MinIO` driver — convenience constructor for [`S3Driver`].
//!
//! Alias for [`S3Driver`] with sensible defaults for `MinIO` deployments.

use crate::driver::s3::{S3Config, S3Driver};

/// Convenience constructor for `MinIO`.
pub async fn minio(
    bucket: impl Into<String>,
    region: impl Into<String>,
    access_key_id: impl Into<String>,
    secret_access_key: impl Into<String>,
    endpoint: impl Into<String>,
) -> Result<S3Driver, crate::StorageError> {
    let config = S3Config::new(bucket, region, access_key_id, secret_access_key)
        .with_endpoint(endpoint)
        .with_path_style(true);
    S3Driver::new(config).await
}

/// Alias for [`S3Driver`] (semantically the same; `MinIO` is S3-compatible).
pub type MinioDriver = S3Driver;
