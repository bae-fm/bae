//! Cover art resizing for storage and export.

use tracing::debug;

/// Maximum dimension for a stored/embedded cover thumbnail.
const COVER_MAX_SIZE: u32 = 600;

/// Resize cover art to fit within COVER_MAX_SIZE, encoded as JPEG.
pub fn resize_cover(data: &[u8]) -> Result<Vec<u8>, String> {
    let img = image::load_from_memory(data)
        .map_err(|e| format!("Failed to decode cover image: {}", e))?;

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

    /// Encode a solid-color image of the given dimensions to PNG bytes, so the
    /// resizer's input is a real decodable image of a non-JPEG format.
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
        // The load-bearing assertion: a sub-600 source keeps its dimensions —
        // resize downscales only, it never enlarges.
        let out = resize_cover(&png_source(300, 300)).unwrap();
        assert_eq!(decoded_dims(&out), (300, 300));
    }

    #[test]
    fn garbage_bytes_error() {
        assert!(resize_cover(&[0u8; 16]).is_err());
    }
}
