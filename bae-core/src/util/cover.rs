//! Cover art resizing for storage and export.

use std::io::Cursor;

use image::ImageReader;
use tracing::debug;

use super::content_type::ContentType;

/// Maximum dimension for a stored/embedded cover thumbnail.
const COVER_MAX_SIZE: u32 = 600;
const MAX_DECODE_DIMENSION: u32 = 8_192;
const MAX_DECODE_ALLOC_BYTES: u64 = 128 * 1024 * 1024;

/// Decode the first image using the cover format and resource limits shared by
/// remote validation and stored-cover normalization.
pub(crate) fn decode_cover(data: &[u8]) -> Result<(image::DynamicImage, ContentType), String> {
    let mut reader = ImageReader::new(Cursor::new(data))
        .with_guessed_format()
        .map_err(|e| format!("Failed to read cover image: {}", e))?;
    let format = reader
        .format()
        .ok_or_else(|| "Unrecognized cover image format".to_string())?;
    let content_type = ContentType::from_mime(format.to_mime_type());
    if !content_type.is_supported_cover() {
        return Err(format!("Cover image format {format:?} is not supported"));
    }

    let mut limits = image::Limits::default();
    limits.max_image_width = Some(MAX_DECODE_DIMENSION);
    limits.max_image_height = Some(MAX_DECODE_DIMENSION);
    limits.max_alloc = Some(MAX_DECODE_ALLOC_BYTES);
    reader.limits(limits);

    let img = reader
        .decode()
        .map_err(|e| format!("Failed to decode cover image: {}", e))?;
    Ok((img, content_type))
}

/// Resize cover art to fit within COVER_MAX_SIZE (downscale only), as JPEG.
pub fn resize_cover(data: &[u8]) -> Result<Vec<u8>, String> {
    let (img, _) = decode_cover(data)?;
    let (w, h) = (img.width(), img.height());

    let img = if w > COVER_MAX_SIZE || h > COVER_MAX_SIZE {
        debug!(
            "Resizing cover art from {}x{} to fit {}x{}",
            w, h, COVER_MAX_SIZE, COVER_MAX_SIZE
        );
        img.resize(
            COVER_MAX_SIZE,
            COVER_MAX_SIZE,
            image::imageops::FilterType::Lanczos3,
        )
    } else {
        img
    };

    let mut buf = std::io::Cursor::new(Vec::new());
    img.write_to(&mut buf, image::ImageFormat::Jpeg)
        .map_err(|e| format!("Failed to encode cover as JPEG: {}", e))?;
    Ok(buf.into_inner())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A solid-color PNG, so the resizer's input is a real decodable image in a
    /// format other than its JPEG output.
    fn png_source(width: u32, height: u32) -> Vec<u8> {
        let img = image::RgbImage::from_pixel(width, height, image::Rgb([120, 40, 200]));
        let mut buf = std::io::Cursor::new(Vec::new());
        image::DynamicImage::ImageRgb8(img)
            .write_to(&mut buf, image::ImageFormat::Png)
            .unwrap();
        buf.into_inner()
    }

    /// Decode resized bytes to their real pixel dimensions and confirm JPEG.
    fn decoded_dims(bytes: &[u8]) -> (u32, u32) {
        let format = image::guess_format(bytes).unwrap();
        assert_eq!(format, image::ImageFormat::Jpeg, "output must be JPEG");
        let img = image::load_from_memory(bytes).unwrap();
        (img.width(), img.height())
    }

    #[test]
    fn square_large_downscales_to_600() {
        let out = resize_cover(&png_source(1200, 1200)).unwrap();
        assert_eq!(decoded_dims(&out), (600, 600));
    }

    #[test]
    fn wide_source_keeps_aspect_no_pad() {
        let out = resize_cover(&png_source(1200, 600)).unwrap();
        assert_eq!(decoded_dims(&out), (600, 300));
    }

    #[test]
    fn small_source_is_not_upscaled() {
        let out = resize_cover(&png_source(300, 300)).unwrap();
        assert_eq!(decoded_dims(&out), (300, 300));
    }

    #[test]
    fn garbage_bytes_error() {
        assert!(resize_cover(&[0u8; 16]).is_err());
    }

    #[test]
    fn oversized_source_dimensions_error_before_decode() {
        assert!(resize_cover(&png_source(MAX_DECODE_DIMENSION + 1, 1)).is_err());
    }

    #[test]
    fn gif_and_webp_covers_normalize_the_first_image_to_jpeg() {
        for (name, bytes, dimensions) in [
            (
                "GIF",
                include_bytes!("../../test-fixtures/cover-art/solid.gif").as_slice(),
                (16, 8),
            ),
            (
                "WebP",
                include_bytes!("../../test-fixtures/cover-art/solid.webp").as_slice(),
                (16, 8),
            ),
            (
                "animated GIF",
                include_bytes!("../../test-fixtures/cover-art/animated.gif").as_slice(),
                (600, 300),
            ),
            (
                "animated WebP",
                include_bytes!("../../test-fixtures/cover-art/animated.webp").as_slice(),
                (600, 300),
            ),
        ] {
            let output = resize_cover(bytes).unwrap_or_else(|error| panic!("{name}: {error}"));
            assert_eq!(decoded_dims(&output), dimensions, "{name}");
            let pixel = image::load_from_memory(&output)
                .unwrap()
                .to_rgb8()
                .get_pixel(0, 0)
                .0;
            for (actual, expected) in pixel.into_iter().zip([120u8, 40, 200]) {
                assert!(actual.abs_diff(expected) <= 3, "{name}: {pixel:?}");
            }
        }
    }

    #[test]
    fn transparent_covers_encode_as_jpeg() {
        let mut png = Cursor::new(Vec::new());
        image::DynamicImage::ImageRgba8(image::RgbaImage::from_pixel(
            16,
            8,
            image::Rgba([120, 40, 200, 128]),
        ))
        .write_to(&mut png, image::ImageFormat::Png)
        .unwrap();
        for (bytes, right_pixel) in [
            (png.get_ref().as_slice(), [120u8, 40, 200]),
            (
                include_bytes!("../../test-fixtures/cover-art/transparent.webp").as_slice(),
                [120, 40, 200],
            ),
            // This GIF's transparent palette entry is black. Dropping alpha
            // retains that color, without introducing a background composite.
            (
                include_bytes!("../../test-fixtures/cover-art/transparent.gif").as_slice(),
                [0, 0, 0],
            ),
        ] {
            let output = resize_cover(bytes).unwrap();
            assert_eq!(decoded_dims(&output), (16, 8));
            let pixel = image::load_from_memory(&output)
                .unwrap()
                .to_rgb8()
                .get_pixel(0, 0)
                .0;
            for (actual, expected) in pixel.into_iter().zip([120u8, 40, 200]) {
                assert!(actual.abs_diff(expected) <= 3, "{pixel:?}");
            }
            let pixel = image::load_from_memory(&output)
                .unwrap()
                .to_rgb8()
                .get_pixel(15, 0)
                .0;
            for (actual, expected) in pixel.into_iter().zip(right_pixel) {
                assert!(actual.abs_diff(expected) <= 3, "{pixel:?}");
            }
        }
    }

    #[test]
    fn gif_canvas_exceeding_decode_allocation_is_rejected() {
        let mut bytes = include_bytes!("../../test-fixtures/cover-art/solid.gif").to_vec();
        bytes[6..8].copy_from_slice(&6000u16.to_le_bytes());
        bytes[8..10].copy_from_slice(&6000u16.to_le_bytes());
        let error = resize_cover(&bytes).unwrap_err();
        assert!(error.contains("Memory limit"), "{error}");
    }
}
