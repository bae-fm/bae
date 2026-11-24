use super::*;

#[test]
fn image_content_type_maps_known_and_rejects_non_images() {
    use lofty::picture::MimeType;
    assert_eq!(image_content_type(&MimeType::Jpeg), Some(ContentType::Jpeg));
    assert_eq!(image_content_type(&MimeType::Png), Some(ContentType::Png));
    assert_eq!(image_content_type(&MimeType::Gif), Some(ContentType::Gif));
    assert_eq!(image_content_type(&MimeType::Bmp), None);
    // WebP reaches us only as Unknown — lofty has no WebP variant.
    assert_eq!(
        image_content_type(&MimeType::Unknown("image/webp".to_string())),
        Some(ContentType::Webp)
    );
    // A non-image Unknown mime is rejected.
    assert_eq!(
        image_content_type(&MimeType::Unknown("application/octet-stream".to_string())),
        None
    );
    // A non-image known variant is rejected too.
    assert_eq!(image_content_type(&MimeType::Tiff), None);
}

/// A read that fails while the parser reads the tags is the read's own error,
/// not a failure to parse.
#[test]
fn a_failed_read_is_a_read_error_though_the_parser_met_it() {
    struct Unreadable;
    impl Read for Unreadable {
        fn read(&mut self, _: &mut [u8]) -> std::io::Result<usize> {
            Err(std::io::Error::other("the share went away"))
        }
    }
    impl Seek for Unreadable {
        fn seek(&mut self, _: std::io::SeekFrom) -> std::io::Result<u64> {
            Ok(0)
        }
    }

    let error = read_tag_art(Unreadable).unwrap_err();

    assert!(matches!(error, TagArtError::Read(_)), "{error:?}");
}
