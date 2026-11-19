use super::*;
use bae_util::test_files::TestFolder;
use std::path::PathBuf;

/// A folder holding one file named `image.<extension>` with `data`, and the
/// file's path.
fn write_temp_file(extension: &str, data: &[u8]) -> (TestFolder, PathBuf) {
    let folder = TestFolder::new();
    let path = folder.write(format!("image.{extension}"), data);
    (folder, path)
}

#[test]
fn test_valid_jpeg_magic() {
    let data = [0xFF, 0xD8, 0xFF, 0xE0, 0x00, 0x10];
    let (_folder, file) = write_temp_file("jpg", &data);
    assert!(is_valid_image(&file, bae_util::test_files::place()).unwrap());
}

#[test]
fn test_valid_png_magic() {
    let data = [0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A, 0x00];
    let (_folder, file) = write_temp_file("png", &data);
    assert!(is_valid_image(&file, bae_util::test_files::place()).unwrap());
}

#[test]
fn test_valid_webp_magic() {
    let data = b"RIFF\x00\x00\x00\x00WEBP";
    let (_folder, file) = write_temp_file("webp", data);
    assert!(is_valid_image(&file, bae_util::test_files::place()).unwrap());
}

#[test]
fn test_valid_gif_magic() {
    let data = b"GIF89a\x00\x00";
    let (_folder, file) = write_temp_file("gif", data);
    assert!(is_valid_image(&file, bae_util::test_files::place()).unwrap());
}

#[test]
fn test_valid_bmp_magic() {
    let data = b"BM\x00\x00\x00\x00";
    let (_folder, file) = write_temp_file("bmp", data);
    assert!(is_valid_image(&file, bae_util::test_files::place()).unwrap());
}

#[test]
fn test_invalid_image_magic() {
    // Random bytes that don't match JPEG magic
    let data = [0x00, 0x01, 0x02, 0x03, 0x04, 0x05];
    let (_folder, file) = write_temp_file("jpg", &data);
    assert!(!is_valid_image(&file, bae_util::test_files::place()).unwrap());
}

#[test]
fn test_invalid_png_magic() {
    let data = [0x00, 0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07, 0x08];
    let (_folder, file) = write_temp_file("png", &data);
    assert!(!is_valid_image(&file, bae_util::test_files::place()).unwrap());
}

#[test]
fn truncated_image_headers_are_rejected() {
    // Each image magic is checked against `bytes_read`, so a file shorter
    // than its magic (webp=12, gif=4, bmp=2) is rejected without an
    // out-of-bounds read.
    for (ext, bytes) in [
        ("webp", &b"RIFF\x00\x00\x00\x00WEB"[..]), // 11 bytes, needs 12
        ("gif", &b"GIF"[..]),                      // 3 bytes, needs 4
        ("bmp", &b"B"[..]),                        // 1 byte, needs 2
    ] {
        let (_folder, file) = write_temp_file(ext, bytes);
        assert!(
            !is_valid_image(&file, bae_util::test_files::place()).unwrap(),
            ".{ext} with a truncated magic must be rejected",
        );
    }
}

#[test]
fn test_unknown_image_extension_assumed_valid() {
    let data = [0x00, 0x01, 0x02, 0x03];
    let (_folder, file) = write_temp_file("tiff", &data);
    assert!(is_valid_image(&file, bae_util::test_files::place()).unwrap());
}

#[test]
fn zero_byte_images_are_rejected() {
    for ext in ["jpg", "png", "webp", "gif", "bmp"] {
        let (_folder, file) = write_temp_file(ext, &[]);
        assert!(
            !is_valid_image(&file, bae_util::test_files::place()).unwrap(),
            "is_valid_image must reject a zero-byte .{ext} image",
        );
    }
}
