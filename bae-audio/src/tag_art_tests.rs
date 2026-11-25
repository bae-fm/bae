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
/// not a failure to parse, though lofty puts it behind its own.
#[test]
fn a_failed_read_is_a_read_error_though_the_parser_met_it() {
    /// A FLAC file that answers the read that finds its format and fails every
    /// read after it.
    struct FailsAfterTheFirstRead {
        file: std::io::Cursor<Vec<u8>>,
        reads: usize,
    }
    impl Read for FailsAfterTheFirstRead {
        fn read(&mut self, buffer: &mut [u8]) -> std::io::Result<usize> {
            self.reads += 1;
            if self.reads > 1 {
                return Err(std::io::Error::other("the share went away"));
            }
            self.file.read(buffer)
        }
    }
    impl Seek for FailsAfterTheFirstRead {
        fn seek(&mut self, position: std::io::SeekFrom) -> std::io::Result<u64> {
            self.file.seek(position)
        }
    }
    // The magic, then a STREAMINFO block header of 34 bytes.
    let mut file = b"fLaC\x00\x00\x00\x22".to_vec();
    file.resize(64, 0);

    let error = read_tag_art(FailsAfterTheFirstRead { file: std::io::Cursor::new(file), reads: 0 })
        .unwrap_err();

    assert!(matches!(error, TagArtError::Read(_)), "{error:?}");
}
