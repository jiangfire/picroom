// SPDX-License-Identifier: MIT
// Copyright (c) 2026 Picroom Contributors

//! WebP encoder — the single implementation backing the worker's
//! `EncodeWebp` jobs (Q-7). The `image` crate's WebP encoder is
//! lossless-only, so output is lossless WebP regardless of any quality knob.

use bytes::Bytes;
use image::DynamicImage;

/// Encodes a decoded image to (lossless) WebP.
pub fn encode_webp(img: &DynamicImage) -> Result<Bytes, String> {
    let mut out = Vec::new();
    let mut cur = std::io::Cursor::new(&mut out);
    img.to_rgb8()
        .write_to(&mut cur, image::ImageFormat::WebP)
        .map_err(|e| e.to_string())?;
    Ok(Bytes::from(out))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encodes_valid_webp() {
        let img = image::RgbImage::from_fn(24, 16, |x, y| image::Rgb([x as u8, y as u8, 64]));
        let out = encode_webp(&DynamicImage::ImageRgb8(img)).unwrap();
        assert!(out.starts_with(b"RIFF"));
        assert_eq!(&out[8..12], b"WEBP");
    }
}
