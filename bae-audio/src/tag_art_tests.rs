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
