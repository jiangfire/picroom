// SPDX-License-Identifier: MIT
// Copyright (c) 2026 Picroom Contributors

//! Thumbnail generation — the single implementation backing the worker's
//! `GenerateThumbnail` jobs (Q-7).

use bytes::Bytes;
use image::DynamicImage;

/// Renders a JPEG thumbnail whose longest side is at most `size`
/// (shrink-only: smaller sources are never upscaled).
pub fn encode_thumbnail(img: &DynamicImage, size: u32, jpeg_quality: u8) -> Result<Bytes, String> {
    let resized = {
        let longest = img.width().max(img.height());
        if longest <= size {
            img.clone()
        } else {
            img.resize(size, size, image::imageops::FilterType::Triangle)
        }
    };
    let mut out = Vec::new();
    let encoder =
        image::codecs::jpeg::JpegEncoder::new_with_quality(&mut out, jpeg_quality.clamp(1, 100));
    resized
        .to_rgb8()
        .write_with_encoder(encoder)
        .map_err(|e| e.to_string())?;
    Ok(Bytes::from(out))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bounds_longest_side_and_never_upscales() {
        let img = image::RgbImage::from_fn(100, 80, |x, y| image::Rgb([x as u8, y as u8, 64]));
        let out = encode_thumbnail(&DynamicImage::ImageRgb8(img), 32, 85).unwrap();
        let decoded = image::load_from_memory(&out).unwrap();
        assert_eq!(decoded.width().max(decoded.height()), 32);

        // Smaller source is passed through, not upscaled.
        let small = image::RgbImage::from_fn(10, 8, |x, y| image::Rgb([x as u8, y as u8, 64]));
        let out = encode_thumbnail(&DynamicImage::ImageRgb8(small), 32, 85).unwrap();
        let decoded = image::load_from_memory(&out).unwrap();
        assert_eq!(decoded.width(), 10);
    }
}
