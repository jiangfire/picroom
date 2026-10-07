// SPDX-License-Identifier: MIT
// Copyright (c) 2026 Picroom Contributors

//! AVIF encoder — the single implementation backing the worker's
//! `EncodeAvif` jobs (Q-7: the worker's private copy was deleted).

use bytes::Bytes;
use image::DynamicImage;

/// Encodes a decoded image to AVIF at `quality` (0–100, ravif scale).
pub fn encode_avif(img: &DynamicImage, quality: f32) -> Result<Bytes, String> {
    use ravif::{Img, RGB8};
    let w = img.width() as usize;
    let h = img.height() as usize;
    let rgb = img.to_rgb8();
    let pixels: Vec<RGB8> = rgb
        .pixels()
        .map(|p| {
            let ch = p.0;
            RGB8 {
                r: ch[0],
                g: ch[1],
                b: ch[2],
            }
        })
        .collect();
    let enc = ravif::Encoder::new()
        .with_quality(quality.clamp(0.0, 100.0))
        .with_speed(6)
        .encode_rgb(Img::new(pixels.as_slice(), w, h))
        .map_err(|e| format!("ravif encode: {e:?}"))?;
    Ok(Bytes::from(enc.avif_file))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_img() -> DynamicImage {
        let img = image::RgbImage::from_fn(24, 16, |x, y| image::Rgb([x as u8, y as u8, 64]));
        DynamicImage::ImageRgb8(img)
    }

    #[test]
    fn encodes_valid_avif_at_quality() {
        let out = encode_avif(&test_img(), 60.0).unwrap();
        assert!(out.len() > 16);
        // FTAV brand box marker.
        assert_eq!(&out[4..12], b"ftypavif");
    }

    #[test]
    fn quality_changes_output() {
        let img = test_img();
        let low = encode_avif(&img, 10.0).unwrap();
        let high = encode_avif(&img, 95.0).unwrap();
        assert_ne!(low, high);
    }
}
