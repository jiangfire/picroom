// SPDX-License-Identifier: MIT
// Copyright (c) 2026 Picroom Contributors

//! Image entity.

use crate::storage_key::StorageKey;
use crate::user::UserId;
use serde::{Deserialize, Serialize};
use time::OffsetDateTime;
use uuid::Uuid;

/// Stable identifier for an image.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct ImageId(pub Uuid);

impl ImageId {
    /// Returns the underlying UUID.
    pub const fn as_uuid(&self) -> Uuid {
        self.0
    }
}

impl std::fmt::Display for ImageId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl From<Uuid> for ImageId {
    fn from(u: Uuid) -> Self {
        Self(u)
    }
}

/// A variant produced from an image (AVIF, WebP, thumbnail, …).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "kind", content = "size")]
pub enum ImageVariant {
    /// AVIF variant.
    Avif,
    /// WebP variant.
    Webp,
    /// Thumbnail at the given size.
    Thumbnail(u32),
    /// Watermarked variant.
    Watermark,
}

/// Image entity.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Image {
    /// Stable id.
    pub id: ImageId,
    /// Owner user.
    pub owner_id: UserId,
    /// Storage key.
    pub key: StorageKey,
    /// Content type (MIME).
    pub content_type: String,
    /// Size in bytes.
    pub bytes: u64,
    /// Width in pixels.
    pub width: u32,
    /// Height in pixels.
    pub height: u32,
    /// Hex SHA-256 of the bytes, if available.
    pub sha256: Option<String>,
    /// Available variants.
    pub variants: Vec<ImageVariant>,
    /// Creation timestamp.
    pub created_at: OffsetDateTime,
}

impl Image {
    /// Aspect ratio as a float; returns `None` if height is zero.
    pub fn aspect_ratio(&self) -> Option<f32> {
        if self.height == 0 {
            None
        } else {
            Some(self.width as f32 / self.height as f32)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn aspect_ratio_returns_none_when_height_is_zero() {
        let img = Image {
            id: ImageId(Uuid::nil()),
            owner_id: UserId(Uuid::nil()),
            key: StorageKey::parse("x.jpg").unwrap(),
            content_type: "image/jpeg".into(),
            bytes: 1,
            width: 1920,
            height: 0,
            sha256: None,
            variants: vec![],
            created_at: OffsetDateTime::UNIX_EPOCH,
        };
        assert_eq!(img.aspect_ratio(), None);
    }

    #[test]
    fn aspect_ratio_computes_correctly() {
        let img = Image {
            id: ImageId(Uuid::nil()),
            owner_id: UserId(Uuid::nil()),
            key: StorageKey::parse("x.jpg").unwrap(),
            content_type: "image/jpeg".into(),
            bytes: 1,
            width: 1920,
            height: 1080,
            sha256: None,
            variants: vec![],
            created_at: OffsetDateTime::UNIX_EPOCH,
        };
        assert_eq!(img.aspect_ratio(), Some(16.0 / 9.0));
    }
}
