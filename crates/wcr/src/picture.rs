//! SPDX-License-Identifier: Apache-2.0
//! Pictures sent in direct chats are shrunk to fit the file limit.
//!
//! A picture (JPEG, PNG, GIF, WebP or BMP) that is too big is turned the
//! right way up, made smaller and saved again as JPEG, stepping down in
//! size and quality until it fits. The new JPEG carries no metadata, so a
//! phone photo's location does not go on the air.

use image::codecs::jpeg::JpegEncoder;
use image::{DynamicImage, ImageDecoder, ImageFormat, ImageReader};
use std::io::Cursor;

/// Longest side we start from: bigger than a 4 KB picture ever ends up.
const START_SIDE: u32 = 640;
/// Smallest longest side worth sending.
const MIN_SIDE: u32 = 40;
/// JPEG qualities tried at each size, best first. Below ~35 the blocks
/// show more than a smaller picture would.
const QUALITIES: [u8; 4] = [75, 60, 45, 35];

/// True when `data` is a picture we can shrink.
pub fn is_picture(data: &[u8]) -> bool {
    matches!(
        image::guess_format(data),
        Ok(ImageFormat::Jpeg
            | ImageFormat::Png
            | ImageFormat::Gif
            | ImageFormat::WebP
            | ImageFormat::Bmp)
    )
}

/// A picture that fits in `max_bytes`, plus its width and height.
pub struct Shrunk {
    pub jpeg: Vec<u8>,
    pub width: u32,
    pub height: u32,
}

/// Make `data` (a picture) fit in `max_bytes` as a JPEG. The biggest size
/// that fits wins; at that size, the best quality that fits.
pub fn shrink(data: &[u8], max_bytes: usize) -> Result<Shrunk, String> {
    let img = decode(data)?;
    let longest = img.width().max(img.height());
    let mut side = longest.min(START_SIDE);
    while side >= MIN_SIDE {
        let small = if side < longest {
            img.resize(side, side, image::imageops::FilterType::Triangle)
        } else {
            img.clone()
        };
        let rgb = DynamicImage::ImageRgb8(small.to_rgb8());
        for q in QUALITIES {
            let mut out = Vec::new();
            JpegEncoder::new_with_quality(&mut out, q)
                .encode_image(&rgb)
                .map_err(|e| format!("could not save the picture: {e}"))?;
            if out.len() <= max_bytes {
                return Ok(Shrunk {
                    jpeg: out,
                    width: rgb.width(),
                    height: rgb.height(),
                });
            }
        }
        side = side * 85 / 100;
    }
    Err("the picture would not shrink small enough to send".into())
}

/// Decode and turn the right way up (phone photos store the turn in EXIF).
fn decode(data: &[u8]) -> Result<DynamicImage, String> {
    let bad = |e: image::ImageError| format!("could not read the picture: {e}");
    let mut dec = ImageReader::new(Cursor::new(data))
        .with_guessed_format()
        .map_err(|e| format!("could not read the picture: {e}"))?
        .into_decoder()
        .map_err(bad)?;
    let turn = dec
        .orientation()
        .unwrap_or(image::metadata::Orientation::NoTransforms);
    let mut img = DynamicImage::from_decoder(dec).map_err(bad)?;
    img.apply_orientation(turn);
    Ok(img)
}

/// `holiday.png` becomes `holiday.jpg`.
pub fn jpeg_name(name: &str) -> String {
    let stem = match name.rfind('.') {
        Some(i) if i > 0 => &name[..i],
        _ => name,
    };
    format!("{stem}.jpg")
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A noisy colour picture that does not compress well, like a photo.
    fn photo(w: u32, h: u32) -> Vec<u8> {
        let img = image::RgbImage::from_fn(w, h, |x, y| {
            let n = (x.wrapping_mul(2654435761) ^ y.wrapping_mul(40503)) % 97;
            image::Rgb([(x % 256) as u8, (y % 256) as u8, (n * 2) as u8])
        });
        let mut out = Vec::new();
        DynamicImage::ImageRgb8(img)
            .write_to(&mut Cursor::new(&mut out), ImageFormat::Png)
            .unwrap();
        out
    }

    #[test]
    fn a_big_photo_shrinks_under_the_limit() {
        let png = photo(1600, 1200);
        assert!(is_picture(&png));
        assert!(png.len() > 4096);
        let s = shrink(&png, 4096).unwrap();
        assert!(s.jpeg.len() <= 4096, "{} bytes", s.jpeg.len());
        assert!(is_picture(&s.jpeg));
        // The shape is kept.
        assert!(s.width > s.height);
    }

    #[test]
    fn text_is_not_a_picture() {
        assert!(!is_picture(b"hello, this is a note"));
        assert!(shrink(b"hello", 4096).is_err());
    }

    #[test]
    fn names_end_in_jpg() {
        assert_eq!(jpeg_name("holiday.png"), "holiday.jpg");
        assert_eq!(jpeg_name("cat"), "cat.jpg");
        assert_eq!(jpeg_name("a.b.webp"), "a.b.jpg");
    }
}
