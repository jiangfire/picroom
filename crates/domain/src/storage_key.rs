//! Validated storage key.

use serde::{Deserialize, Serialize};
use std::str::FromStr;
use thiserror::Error;

/// Storage-key validation errors.
#[derive(Debug, Error, PartialEq, Eq)]
pub enum StorageKeyError {
    /// Key is empty.
    #[error("storage key cannot be empty")]
    Empty,
    /// Key is too long (max 1024 characters).
    #[error("storage key too long (max 1024)")]
    TooLong,
    /// Key contains a leading slash.
    #[error("storage key must not start with '/'")]
    LeadingSlash,
    /// Key contains path traversal.
    #[error("storage key must not contain '..'")]
    PathTraversal,
    /// Key contains invalid characters.
    #[error("storage key contains invalid characters: {0:?}")]
    InvalidChars(char),
}

/// A validated storage key (object path within a bucket).
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct StorageKey(String);

impl StorageKey {
    /// Maximum allowed key length.
    pub const MAX_LEN: usize = 1024;

    /// Returns the underlying string slice.
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// Parses a string into a validated `StorageKey`.
    pub fn parse(s: &str) -> Result<Self, StorageKeyError> {
        if s.is_empty() {
            return Err(StorageKeyError::Empty);
        }
        if s.len() > Self::MAX_LEN {
            return Err(StorageKeyError::TooLong);
        }
        if s.starts_with('/') {
            return Err(StorageKeyError::LeadingSlash);
        }
        if s.split('/').any(|seg| seg == "..") {
            return Err(StorageKeyError::PathTraversal);
        }
        for c in s.chars() {
            if !(c.is_ascii_alphanumeric() || matches!(c, '/' | '-' | '_' | '.' | ' ')) {
                return Err(StorageKeyError::InvalidChars(c));
            }
        }
        Ok(Self(s.to_string()))
    }
}

impl FromStr for StorageKey {
    type Err = StorageKeyError;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Self::parse(s)
    }
}

impl std::fmt::Display for StorageKey {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

impl AsRef<str> for StorageKey {
    fn as_ref(&self) -> &str {
        &self.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn valid_key_accepted() {
        assert!(StorageKey::parse("path/to/img.jpg").is_ok());
        assert!(StorageKey::parse("img_2026-07-05.avif").is_ok());
    }

    #[test]
    fn empty_key_rejected() {
        assert_eq!(StorageKey::parse(""), Err(StorageKeyError::Empty));
    }

    #[test]
    fn too_long_key_rejected() {
        let s = "a".repeat(StorageKey::MAX_LEN + 1);
        assert_eq!(StorageKey::parse(&s), Err(StorageKeyError::TooLong));
    }

    #[test]
    fn leading_slash_rejected() {
        assert_eq!(
            StorageKey::parse("/x.jpg"),
            Err(StorageKeyError::LeadingSlash)
        );
    }

    #[test]
    fn path_traversal_rejected() {
        assert_eq!(
            StorageKey::parse("a/../b"),
            Err(StorageKeyError::PathTraversal)
        );
    }

    #[test]
    fn invalid_chars_rejected() {
        assert!(matches!(
            StorageKey::parse("a$b"),
            Err(StorageKeyError::InvalidChars('$'))
        ));
    }
}