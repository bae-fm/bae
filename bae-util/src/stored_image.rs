//! The image file bae stores for a cover, an artist picture or an avatar:
//! whatever format it arrived in, decoded and downscaled to a JPEG at most 600
//! pixels on its longer side.

use std::io::Cursor;

use image::ImageReader;
use tracing::debug;

use crate::content_type::ContentType;

/// The longest side, in pixels, of an image bae stores.
pub const STORED_MAX_EDGE: u32 = 600;
/// The longest side, in pixels, of an image bae decodes: a camera's photo
/// fits (a 48-megapixel one is 8064 by 6048).
const MAX_DECODE_DIMENSION: u32 = 8_192;
/// The most a decode may allocate: the largest image the dimension limit
/// admits, at 8-bit RGBA, so every JPEG, GIF and WebP within that limit
/// decodes. A 16-bit PNG takes twice that per pixel, so one is refused past
/// about 5,800 pixels square.
const MAX_DECODE_ALLOC_BYTES: u64 = MAX_DECODE_DIMENSION as u64 * MAX_DECODE_DIMENSION as u64 * 4;

/// Decode the first image using the formats and resource limits shared by
/// remote validation and [`stored_jpeg`].
pub fn decode_image(data: &[u8]) -> Result<(image::DynamicImage, ContentType), String> {
    let mut reader = ImageReader::new(Cursor::new(data))
        .with_guessed_format()
        .map_err(|e| format!("Failed to read image: {}", e))?;
    let format = reader.format().ok_or_else(|| "Unrecognized image format".to_string())?;
    let content_type = ContentType::from_mime(format.to_mime_type());
    if !content_type.is_supported_cover() {
        return Err(format!("Image format {format:?} is not supported"));
    }

    let mut limits = image::Limits::default();
    limits.max_image_width = Some(MAX_DECODE_DIMENSION);
    limits.max_image_height = Some(MAX_DECODE_DIMENSION);
    limits.max_alloc = Some(MAX_DECODE_ALLOC_BYTES);
    reader.limits(limits);

    let img = reader.decode().map_err(|e| format!("Failed to decode image: {}", e))?;
    Ok((img, content_type))
}

/// The JPEG bae stores for `data`: the image downscaled to fit within 600
/// pixels on each side, never upscaled.
pub fn stored_jpeg(data: &[u8]) -> Result<Vec<u8>, String> {
    let (img, _) = decode_image(data)?;
    let (w, h) = (img.width(), img.height());

    let img = if w > STORED_MAX_EDGE || h > STORED_MAX_EDGE {
        debug!("Resizing image from {}x{} to fit {}x{}", w, h, STORED_MAX_EDGE, STORED_MAX_EDGE);
        img.resize(STORED_MAX_EDGE, STORED_MAX_EDGE, image::imageops::FilterType::Lanczos3)
    } else {
        img
    };

    let mut buf = std::io::Cursor::new(Vec::new());
    img.write_to(&mut buf, image::ImageFormat::Jpeg)
        .map_err(|e| format!("Failed to encode image as JPEG: {}", e))?;
    Ok(buf.into_inner())
}

#[cfg(test)]
#[path = "stored_image_tests.rs"]
mod tests;
