//! Bucket dispatch.

use picroom_domain::StorageKey;
use std::str::FromStr;
use thiserror::Error;

/// Bucket dispatch errors.
#[derive(Debug, Error)]
pub enum BucketError {
    /// Empty bucket name.
    #[error("empty bucket name")]
    Empty,
    /// Invalid characters.
    #[error("invalid bucket name: {0}")]
    Invalid(String),
}

/// A validated bucket name.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct BucketName(String);

impl BucketName {
    /// Returns the bucket name as a string slice.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl FromStr for BucketName {
    type Err = BucketError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        if s.is_empty() {
            return Err(BucketError::Empty);
        }
        if s.len() > 63 {
            return Err(BucketError::Invalid("too long".into()));
        }
        if !s
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-' || c == '.')
        {
            return Err(BucketError::Invalid("invalid characters".into()));
        }
        Ok(Self(s.to_string()))
    }
}

/// Splits a path into (bucket, key).
pub fn split_path(path: &str) -> Result<(BucketName, StorageKey), BucketError> {
    let path = path.trim_start_matches('/');
    let (bucket, rest) = path
        .split_once('/')
        .ok_or_else(|| BucketError::Invalid("missing key".into()))?;
    let bucket = BucketName::from_str(bucket)?;
    let key = StorageKey::parse(rest).map_err(|e| BucketError::Invalid(e.to_string()))?;
    Ok((bucket, key))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn split_path_valid() {
        let (b, k) = split_path("my-bucket/path/to/img.jpg").unwrap();
        assert_eq!(b.as_str(), "my-bucket");
        assert_eq!(k.as_str(), "path/to/img.jpg");
    }

    #[test]
    fn split_path_rejects_empty_bucket() {
        assert!(split_path("/x.jpg").is_err());
    }
}
