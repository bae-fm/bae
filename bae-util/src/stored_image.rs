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
/// remote validation and [`stored_jpeg`]. A JPEG decodes at the largest 1/2,
/// 1/4 or 1/8 scale whose longer side is still at least [`STORED_MAX_EDGE`]
/// pixels, so its pixels follow the stored size and not the photo's.
pub fn decode_image(data: &[u8]) -> Result<(image::DynamicImage, ContentType), String> {
    let mut reader = ImageReader::new(Cursor::new(data))
        .with_guessed_format()
        .map_err(|e| format!("Failed to read image: {}", e))?;
    let format = reader.format().ok_or_else(|| "Unrecognized image format".to_string())?;
    let content_type = ContentType::from_mime(format.to_mime_type());
    if !content_type.is_supported_cover() {
        return Err(format!("Image format {format:?} is not supported"));
    }

    if format == image::ImageFormat::Jpeg {
        return decode_jpeg_scaled(data).map(|img| (img, content_type));
    }

    let mut limits = image::Limits::default();
    limits.max_image_width = Some(MAX_DECODE_DIMENSION);
    limits.max_image_height = Some(MAX_DECODE_DIMENSION);
    limits.max_alloc = Some(MAX_DECODE_ALLOC_BYTES);
    reader.limits(limits);

    let img = reader.decode().map_err(|e| format!("Failed to decode image: {}", e))?;
    Ok((img, content_type))
}

/// Decode a JPEG with the decoder's scaled decode, which skips the inverse
/// transform's detail for the scale instead of decoding every pixel and
/// shrinking afterward. The dimension limit applies to the photo's own size.
fn decode_jpeg_scaled(data: &[u8]) -> Result<image::DynamicImage, String> {
    let mut decoder = jpeg_decoder::Decoder::new(Cursor::new(data));
    decoder.read_info().map_err(|e| format!("Failed to decode image: {}", e))?;
    let info =
        decoder.info().ok_or_else(|| "Failed to decode image: no JPEG header".to_string())?;
    if u32::from(info.width) > MAX_DECODE_DIMENSION || u32::from(info.height) > MAX_DECODE_DIMENSION
    {
        return Err(format!(
            "Failed to decode image: {}x{} is over the {MAX_DECODE_DIMENSION} pixel limit",
            info.width, info.height
        ));
    }
    let edge = STORED_MAX_EDGE as u16;
    let (width, height) =
        decoder.scale(edge, edge).map_err(|e| format!("Failed to decode image: {}", e))?;
    let pixels = decoder.decode().map_err(|e| format!("Failed to decode image: {}", e))?;
    let info =
        decoder.info().ok_or_else(|| "Failed to decode image: no JPEG header".to_string())?;
    let (width, height) = (u32::from(width), u32::from(height));
    let bad_buffer =
        || "Failed to decode image: the decoded pixels do not fit their size".to_string();
    match info.pixel_format {
        jpeg_decoder::PixelFormat::L8 => image::GrayImage::from_raw(width, height, pixels)
            .map(image::DynamicImage::ImageLuma8)
            .ok_or_else(bad_buffer),
        jpeg_decoder::PixelFormat::L16 => {
            Err("Failed to decode image: 16-bit JPEG is not supported".to_string())
        }
        jpeg_decoder::PixelFormat::RGB24 => image::RgbImage::from_raw(width, height, pixels)
            .map(image::DynamicImage::ImageRgb8)
            .ok_or_else(bad_buffer),
        jpeg_decoder::PixelFormat::CMYK32 => {
            let rgb = pixels
                .chunks_exact(4)
                .flat_map(|cmyk| {
                    // The decoder reports ink amounts, 255 being full ink.
                    let light = 255 - u32::from(cmyk[3]);
                    [0, 1, 2].map(|i| ((255 - u32::from(cmyk[i])) * light / 255) as u8)
                })
                .collect();
            image::RgbImage::from_raw(width, height, rgb)
                .map(image::DynamicImage::ImageRgb8)
                .ok_or_else(bad_buffer)
        }
    }
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
