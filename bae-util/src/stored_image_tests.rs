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

/// CRC-32 (ISO-HDLC), as a PNG chunk's trailer holds it.
fn crc32(bytes: &[u8]) -> u32 {
    let mut crc = !0u32;
    for byte in bytes {
        crc ^= u32::from(*byte);
        for _ in 0..8 {
            crc = if crc & 1 == 1 { (crc >> 1) ^ 0xEDB8_8320 } else { crc >> 1 };
        }
    }
    !crc
}

/// A 16-bit RGBA PNG inside the dimension limit can still need more than
/// the decode may allocate: 6000 by 6000 at 8 bytes a pixel. It is refused
/// on its header, before its pixels are read, so the image is a 1-pixel PNG
/// whose header claims that size.
#[test]
fn a_png_needing_more_than_the_decode_allocation_is_refused() {
    let mut png = Cursor::new(Vec::new());
    image::DynamicImage::ImageRgba16(image::ImageBuffer::from_pixel(1, 1, image::Rgba([1u16; 4])))
        .write_to(&mut png, image::ImageFormat::Png)
        .unwrap();
    let mut bytes = png.into_inner();
    // The IHDR chunk follows the 8-byte signature: its length, its type,
    // then width and height, and its CRC over type and data.
    assert_eq!(&bytes[12..16], b"IHDR");
    bytes[16..20].copy_from_slice(&6000u32.to_be_bytes());
    bytes[20..24].copy_from_slice(&6000u32.to_be_bytes());
    let crc = crc32(&bytes[12..29]);
    bytes[29..33].copy_from_slice(&crc.to_be_bytes());

    let error = stored_jpeg(&bytes).unwrap_err();
    assert!(error.contains("Memory limit"), "{error}");
}

/// A 48-megapixel camera photo, 8064 by 6048, is inside the dimension limit,
/// so its decode fits the decode's memory limit too and it is stored
/// downscaled. Its pixels vary so the JPEG holds a real photo's entropy.
#[test]
fn a_48_megapixel_photo_is_stored() {
    let (width, height) = (8064, 6048);
    let photo = image::RgbImage::from_fn(width, height, |x, y| {
        let mixed = x.wrapping_mul(2_654_435_761) ^ y.wrapping_mul(2_246_822_519);
        let [r, g, b, _] = mixed.to_le_bytes();
        image::Rgb([r, g, b])
    });
    let mut jpeg = Cursor::new(Vec::new());
    image::DynamicImage::ImageRgb8(photo).write_to(&mut jpeg, image::ImageFormat::Jpeg).unwrap();

    let out = stored_jpeg(jpeg.get_ref()).unwrap();
    assert_eq!(decoded_dims(&out), (600, 450));
}

/// A JPEG of a gradient, `width` by `height`.
fn jpeg_source(width: u32, height: u32) -> Vec<u8> {
    let photo = image::RgbImage::from_fn(width, height, |x, y| {
        image::Rgb([(x % 251) as u8, (y % 241) as u8, 90])
    });
    let mut jpeg = Cursor::new(Vec::new());
    image::DynamicImage::ImageRgb8(photo).write_to(&mut jpeg, image::ImageFormat::Jpeg).unwrap();
    jpeg.into_inner()
}

/// A JPEG decodes at the largest 1/2, 1/4 or 1/8 scale whose longer side still
/// covers the 600-pixel output, so the decoded pixels follow the output and
/// not the photo.
#[test]
fn a_jpeg_decodes_at_the_largest_scale_that_covers_the_output() {
    for (source, decoded) in [
        ((4800, 3600), (600, 450)),
        ((8064, 6048), (1008, 756)),
        ((3000, 2000), (750, 500)),
        ((1500, 1000), (750, 500)),
        ((1000, 700), (1000, 700)),
    ] {
        let (image, _) = decode_image(&jpeg_source(source.0, source.1)).unwrap();
        assert_eq!((image.width(), image.height()), decoded, "a {source:?} JPEG");
    }
}

/// A scaled decode of a large JPEG still stores a 600-pixel image.
#[test]
fn a_large_jpeg_is_stored_at_600_pixels_from_its_scaled_decode() {
    let out = stored_jpeg(&jpeg_source(4800, 3600)).unwrap();
    assert_eq!(decoded_dims(&out), (600, 450));
}
