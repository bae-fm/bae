use super::*;

use ContentType as C;

/// The canonical variant/string mapping. Every variant's `as_str` MIME must
/// round-trip through `from_mime` — if that breaks, stored DB rows read back
/// as `Other(...)` and lose the codec.
#[test]
fn mime_display_name_and_extension() {
    for (ct, mime, display_name, extension) in [
        (C::Flac, "audio/flac", "FLAC", "flac"),
        (C::Mp3, "audio/mpeg", "MP3", "mp3"),
        (C::Ape, "audio/x-ape", "APE", "ape"),
        (C::Alac, "audio/alac", "ALAC", "m4a"),
        (C::Aac, "audio/aac", "AAC", "m4a"),
        (C::Pcm, "audio/pcm", "PCM", "wav"),
        (C::Opus, "audio/opus", "Opus", "opus"),
        (C::Vorbis, "audio/vorbis", "Vorbis", "ogg"),
        (C::WavPack, "audio/wavpack", "WavPack", "wv"),
        (C::Dsd, "audio/dsd", "DSD", "dsf"),
        (C::Jpeg, "image/jpeg", "JPEG", "jpg"),
        (C::Png, "image/png", "PNG", "png"),
        (C::Gif, "image/gif", "GIF", "gif"),
        (C::Webp, "image/webp", "WebP", "webp"),
        (C::Bmp, "image/bmp", "BMP", "bmp"),
        (C::Svg, "image/svg+xml", "SVG", "svg"),
        (C::PlainText, "text/plain", "Text", "txt"),
        (C::Pdf, "application/pdf", "PDF", "pdf"),
        (C::OctetStream, "application/octet-stream", "Binary", "bin"),
    ] {
        assert_eq!(ct.as_str(), mime, "{ct:?}");
        assert_eq!(C::from_mime(mime), ct, "round trip failed for {ct:?}");
        assert_eq!(ct.display_name(), display_name, "{ct:?}");
        assert_eq!(ct.file_extension(), extension, "{ct:?}");
    }
}

#[test]
fn ape_alternative_mime() {
    // "audio/ape" is an informal variant seen in the wild; "audio/x-ape"
    // is what we emit. Both must map to `Ape`.
    assert_eq!(C::from_mime("audio/ape"), C::Ape);
    assert_eq!(C::from_mime("audio/x-ape"), C::Ape);
}

#[test]
fn unknown_mime_lands_in_other() {
    for mime in ["audio/ogg", "audio/mp4", "video/mp4"] {
        assert_eq!(C::from_mime(mime), C::Other(mime.to_string()));
    }
}

#[test]
fn is_audio_membership() {
    for ct in [
        C::Flac,
        C::Mp3,
        C::Ape,
        C::Alac,
        C::Aac,
        C::Pcm,
        C::Opus,
        C::Vorbis,
        C::WavPack,
        C::Dsd,
        // Forward-compat escape hatch: any `Other("audio/...")` is still audio.
        C::Other("audio/opus".to_string()),
    ] {
        assert!(ct.is_audio(), "{ct:?}");
    }
    for ct in [C::Jpeg, C::PlainText, C::Pdf, C::OctetStream, C::Other("video/mp4".to_string())] {
        assert!(!ct.is_audio(), "{ct:?}");
    }
}

#[test]
fn is_image_membership() {
    for ct in [C::Jpeg, C::Png, C::Gif, C::Webp, C::Bmp, C::Svg, C::Other("image/heic".to_string())]
    {
        assert!(ct.is_image(), "{ct:?}");
    }
    for ct in [C::Flac, C::PlainText, C::Other("audio/flac".to_string())] {
        assert!(!ct.is_image(), "{ct:?}");
    }
}

#[test]
fn lossless_audio_membership() {
    for ct in [C::Flac, C::Ape, C::Alac, C::Pcm, C::WavPack, C::Dsd] {
        assert!(ct.is_lossless_audio(), "{ct:?}");
    }
    for ct in [C::Mp3, C::Aac, C::Opus, C::Vorbis] {
        assert!(!ct.is_lossless_audio(), "{ct:?}");
    }
}

#[test]
fn admitted_audio_content_type_gates_unlisted_codecs() {
    // Dispatch check: an admitted codec passes the gate.
    assert!(ContentType::Flac.is_supported_audio());
    // The gate's job — reject anything not on the import allowlist.
    assert!(!ContentType::Other("codec:AV_CODEC_ID_SPEEX".to_string()).is_supported_audio());
    assert!(!ContentType::Other("audio/x-ms-wma".to_string()).is_supported_audio());
}

#[test]
fn each_image_header_opens_as_its_own_format_only() {
    let headers: [(C, &[u8]); 5] = [
        (C::Jpeg, &[0xFF, 0xD8, 0xFF, 0xE0, 0x00, 0x10]),
        (C::Png, &[0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A]),
        (C::Webp, b"RIFF\x00\x00\x00\x00WEBP"),
        (C::Gif, b"GIF89a"),
        (C::Bmp, b"BM\x00\x00"),
    ];
    for (format, _) in &headers {
        for (opened, header) in &headers {
            assert_eq!(format.opens_with(header), Some(format == opened), "{format:?} {opened:?}");
        }
    }
}

#[test]
fn a_header_shorter_than_the_magic_opens_as_nothing() {
    assert_eq!(C::Jpeg.opens_with(&[0xFF, 0xD8]), Some(false));
    assert_eq!(C::Webp.opens_with(b"RIFF\x00\x00\x00\x00WEB"), Some(false));
}

#[test]
fn a_type_with_no_magic_is_not_checked() {
    assert_eq!(C::Svg.opens_with(b"<svg"), None);
    assert_eq!(C::Flac.opens_with(b"fLaC"), None);
}
