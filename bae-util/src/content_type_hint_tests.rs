use super::*;

use ContentType as C;
use ContentTypeHint as H;

#[test]
fn from_extension_classifies_by_extension() {
    for (ext, expected) in [
        ("flac", H::Flac),
        ("FLAC", H::Flac),
        ("mp3", H::Mp3),
        ("MP3", H::Mp3),
        ("ape", H::Ape),
        ("APE", H::Ape),
        ("m4a", H::Mp4Container),
        ("M4A", H::Mp4Container),
        ("wav", H::WavContainer),
        ("aif", H::AiffContainer),
        ("aiff", H::AiffContainer),
        ("aifc", H::AiffContainer),
        ("ogg", H::OggContainer),
        ("oga", H::OggContainer),
        ("opus", H::OpusContainer),
        ("wv", H::WavPack),
        ("dsf", H::DsdContainer),
        ("dff", H::DsdContainer),
        ("jpg", H::Jpeg),
        ("jpeg", H::Jpeg),
        ("JPG", H::Jpeg),
        ("png", H::Png),
        ("gif", H::Gif),
        ("webp", H::Webp),
        ("bmp", H::Bmp),
        ("svg", H::Svg),
        ("txt", H::PlainText),
        ("cue", H::PlainText),
        ("log", H::PlainText),
        ("accurip", H::PlainText),
        ("m3u", H::PlainText),
        ("m3u8", H::PlainText),
        ("pdf", H::Pdf),
        ("PDF", H::Pdf),
        ("aac", H::Unknown("aac".to_string())),
        ("xyz", H::Unknown("xyz".to_string())),
        // Uppercase in, lowercase preserved.
        ("XYZ", H::Unknown("xyz".to_string())),
    ] {
        assert_eq!(H::from_extension(ext), expected, "{ext}");
    }
}

#[test]
fn is_audio_membership() {
    for hint in [
        H::Flac,
        H::Mp3,
        H::Ape,
        H::Mp4Container,
        H::WavContainer,
        H::AiffContainer,
        H::OggContainer,
        H::OpusContainer,
        H::WavPack,
        H::DsdContainer,
    ] {
        assert!(hint.is_audio(), "{hint:?}");
    }
    for hint in [
        H::Jpeg,
        H::Png,
        H::Gif,
        H::Webp,
        H::Bmp,
        H::Svg,
        H::PlainText,
        H::Pdf,
        H::Unknown("wma".to_string()),
    ] {
        assert!(!hint.is_audio(), "{hint:?}");
    }
}

#[test]
fn is_image_membership() {
    for hint in [H::Jpeg, H::Png, H::Gif, H::Webp, H::Bmp, H::Svg] {
        assert!(hint.is_image(), "{hint:?}");
    }
    for hint in [
        H::Flac,
        H::Mp3,
        H::Ape,
        H::Mp4Container,
        H::WavContainer,
        H::AiffContainer,
        H::OggContainer,
        H::OpusContainer,
        H::WavPack,
        H::DsdContainer,
        H::PlainText,
        H::Pdf,
        H::Unknown("svg2".to_string()),
    ] {
        assert!(!hint.is_image(), "{hint:?}");
    }
}

#[test]
fn is_raster_image_membership() {
    for hint in [H::Jpeg, H::Png, H::Gif, H::Webp, H::Bmp] {
        assert!(hint.is_raster_image(), "{hint:?}");
    }
    for hint in [H::Svg, H::Flac, H::PlainText, H::Pdf, H::Unknown("svg2".to_string())] {
        assert!(!hint.is_raster_image(), "{hint:?}");
    }
}

#[test]
fn path_is_raster_image_membership() {
    assert!(H::path_is_raster_image(Path::new("cover.bmp")));
    assert!(!H::path_is_raster_image(Path::new("cover.svg")));
    assert!(!H::path_is_raster_image(Path::new("README")));
}

#[test]
fn image_content_type_promotes_only_image_hints() {
    for (hint, expected) in [
        (H::Jpeg, Some(C::Jpeg)),
        (H::Png, Some(C::Png)),
        (H::Gif, Some(C::Gif)),
        (H::Webp, Some(C::Webp)),
        (H::Bmp, Some(C::Bmp)),
        (H::Svg, Some(C::Svg)),
        (H::Flac, None),
        (H::Mp3, None),
        (H::Mp4Container, None),
        (H::WavContainer, None),
        (H::AiffContainer, None),
        (H::OggContainer, None),
        (H::OpusContainer, None),
        (H::WavPack, None),
        (H::DsdContainer, None),
        (H::PlainText, None),
        (H::Pdf, None),
        (H::Unknown("xyz".to_string()), None),
    ] {
        assert_eq!(hint.image_content_type(), expected, "{hint:?}");
    }
}
