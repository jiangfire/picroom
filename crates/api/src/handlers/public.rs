// SPDX-License-Identifier: MIT
// Copyright (c) 2026 Picroom Contributors

//! Public (unauthenticated) image-byte serving at `/i/*key`.
//!
//! Implements the "公链" (public link) capability: anyone who knows a storage
//! key may read its bytes without authentication. Keys are unguessable
//! (`img/{uuid_v7}.bin`), so knowing the URL is the access grant — the same
//! model used by Lsky Pro / `EasyImage`. See `docs/spec-admin-client.md` §3.3
//! and ADR 0008.

use crate::error::ApiError;
use crate::state::AppState;
use axum::extract::{Path, State};
use axum::http::{header, StatusCode};
use axum::response::{IntoResponse, Response};
use picroom_domain::StorageKey;
use picroom_storage::StorageError;
use std::sync::Arc;

/// `GET /i/*key` — serve raw object bytes with no auth.
///
/// Content-Type is sniffed from the leading magic bytes, because upload keys
/// are `img/{uuid}.bin` and carry no usable extension, and the route must also
/// serve variants (avif/webp/thumb) without per-object DB lookups.
pub async fn serve_object(
    State(state): State<Arc<AppState>>,
    Path(key_raw): Path<String>,
) -> Result<Response, ApiError> {
    // The wildcard capture may include a leading slash depending on the
    // router version; strip it so `img/a.bin` and `/img/a.bin` resolve
    // identically.
    let key_raw = key_raw.trim_start_matches('/');
    let key = StorageKey::parse(key_raw)
        .map_err(|e| ApiError::bad_request(format!("invalid key: {e}")))?;

    let bytes = state.storage.get(&key).await.map_err(map_storage_error)?;

    let content_type = sniff_content_type(&bytes);
    Ok((
        StatusCode::OK,
        [
            (header::CONTENT_TYPE, content_type),
            (header::CACHE_CONTROL, "public, max-age=31536000, immutable"),
        ],
        bytes,
    )
        .into_response())
}

/// Maps a storage error to an API error. `NotFound` → 404; everything else is
/// logged server-side and returned as a generic 500 (no backend detail leak).
fn map_storage_error(e: StorageError) -> ApiError {
    match e {
        StorageError::NotFound(_) => {
            ApiError::new(StatusCode::NOT_FOUND, "not_found", "no such object")
        }
        other => ApiError::internal(other.to_string()),
    }
}

/// Sniffs the Content-Type from leading magic bytes.
///
/// Dependency-free: covers the image formats Picroom produces or accepts
/// (JPEG, PNG, GIF, WebP, AVIF). Falls back to `application/octet-stream`.
fn sniff_content_type(b: &[u8]) -> &'static str {
    if b.starts_with(&[0xFF, 0xD8, 0xFF]) {
        "image/jpeg"
    } else if b.starts_with(&[0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A]) {
        "image/png"
    } else if b.starts_with(b"GIF87a") || b.starts_with(b"GIF89a") {
        "image/gif"
    } else if b.len() >= 12 && &b[0..4] == b"RIFF" && &b[8..12] == b"WEBP" {
        "image/webp"
    } else if is_avif(b) {
        "image/avif"
    } else {
        "application/octet-stream"
    }
}

/// AVIF/HEIF detection via the ISO base media file format `ftyp` box brand.
/// The brand sits at bytes 8..12 (after the 4-byte size + `"ftyp"`).
fn is_avif(b: &[u8]) -> bool {
    if b.len() < 12 || &b[4..8] != b"ftyp" {
        return false;
    }
    let brand = &b[8..12];
    brand == b"avif" || brand == b"avis" || brand == b"mif1"
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sniff_png() {
        let png = [0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A, 0x00, 0x00];
        assert_eq!(sniff_content_type(&png), "image/png");
    }

    #[test]
    fn sniff_jpeg() {
        let jpeg = [0xFF, 0xD8, 0xFF, 0xE0];
        assert_eq!(sniff_content_type(&jpeg), "image/jpeg");
    }

    #[test]
    fn sniff_webp() {
        let mut webp = vec![0x52, 0x49, 0x46, 0x46, 0x00, 0x00, 0x00, 0x00];
        webp.extend_from_slice(b"WEBP");
        assert_eq!(sniff_content_type(&webp), "image/webp");
    }

    #[test]
    fn sniff_avif() {
        let mut avif = vec![0x00, 0x00, 0x00, 0x20]; // box size (ignored)
        avif.extend_from_slice(b"ftyp");
        avif.extend_from_slice(b"avif");
        assert_eq!(sniff_content_type(&avif), "image/avif");
    }

    #[test]
    fn sniff_unknown_falls_back_to_octet_stream() {
        assert_eq!(
            sniff_content_type(b"not an image"),
            "application/octet-stream"
        );
    }

    #[test]
    fn sniff_empty_is_octet_stream() {
        assert_eq!(sniff_content_type(&[]), "application/octet-stream");
    }
}
