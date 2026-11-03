use super::*;

/// A solid-color PNG, so the resizer's input is a real decodable image in a
/// format other than its JPEG output.
fn png_source(width: u32, height: u32) -> Vec<u8> {
    let img = image::RgbImage::from_pixel(width, height, image::Rgb([120, 40, 200]));
    let mut buf = std::io::Cursor::new(Vec::new());
    image::DynamicImage::ImageRgb8(img).write_to(&mut buf, image::ImageFormat::Png).unwrap();
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
    let out = stored_jpeg(&png_source(1200, 1200)).unwrap();
    assert_eq!(decoded_dims(&out), (600, 600));
}

#[test]
fn wide_source_keeps_aspect_no_pad() {
    let out = stored_jpeg(&png_source(1200, 600)).unwrap();
    assert_eq!(decoded_dims(&out), (600, 300));
}

#[test]
fn small_source_is_not_upscaled() {
    let out = stored_jpeg(&png_source(300, 300)).unwrap();
    assert_eq!(decoded_dims(&out), (300, 300));
}

#[test]
fn garbage_bytes_error() {
    assert!(stored_jpeg(&[0u8; 16]).is_err());
}

#[test]
fn oversized_source_dimensions_error_before_decode() {
    assert!(stored_jpeg(&png_source(MAX_DECODE_DIMENSION + 1, 1)).is_err());
}

#[test]
fn gif_and_webp_images_normalize_the_first_image_to_jpeg() {
    for (name, bytes, dimensions) in [
        ("GIF", crate::test_cover_art::SOLID_GIF, (16, 8)),
        ("WebP", crate::test_cover_art::SOLID_WEBP, (16, 8)),
        ("animated GIF", crate::test_cover_art::ANIMATED_GIF, (600, 300)),
        ("animated WebP", crate::test_cover_art::ANIMATED_WEBP, (600, 300)),
    ] {
        let output = stored_jpeg(bytes).unwrap_or_else(|error| panic!("{name}: {error}"));
        assert_eq!(decoded_dims(&output), dimensions, "{name}");
        let pixel = image::load_from_memory(&output).unwrap().to_rgb8().get_pixel(0, 0).0;
        for (actual, expected) in pixel.into_iter().zip([120u8, 40, 200]) {
            assert!(actual.abs_diff(expected) <= 3, "{name}: {pixel:?}");
        }
    }
}

#[test]
fn transparent_images_encode_as_jpeg() {
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
        (crate::test_cover_art::TRANSPARENT_WEBP, [120, 40, 200]),
        // This GIF's transparent palette entry is black. Dropping alpha
        // retains that color, without introducing a background composite.
        (crate::test_cover_art::TRANSPARENT_GIF, [0, 0, 0]),
    ] {
        let output = stored_jpeg(bytes).unwrap();
        assert_eq!(decoded_dims(&output), (16, 8));
        let pixel = image::load_from_memory(&output).unwrap().to_rgb8().get_pixel(0, 0).0;
        for (actual, expected) in pixel.into_iter().zip([120u8, 40, 200]) {
            assert!(actual.abs_diff(expected) <= 3, "{pixel:?}");
        }
        let pixel = image::load_from_memory(&output).unwrap().to_rgb8().get_pixel(15, 0).0;
        for (actual, expected) in pixel.into_iter().zip(right_pixel) {
            assert!(actual.abs_diff(expected) <= 3, "{pixel:?}");
        }
    }
}

#[test]
fn gif_canvas_exceeding_decode_allocation_is_rejected() {
    let mut bytes = crate::test_cover_art::SOLID_GIF.to_vec();
    bytes[6..8].copy_from_slice(&6000u16.to_le_bytes());
    bytes[8..10].copy_from_slice(&6000u16.to_le_bytes());
    let error = stored_jpeg(&bytes).unwrap_err();
    assert!(error.contains("Memory limit"), "{error}");
}
