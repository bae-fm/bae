use super::*;

use AudioContainer as C;

/// Every container's stored name reads back as it, and each names the type
/// its bytes are served as.
#[test]
fn stored_name_and_http_mime() {
    for (container, name, mime) in [
        (C::Flac, "flac", "audio/flac"),
        (C::Mp3, "mp3", "audio/mpeg"),
        (C::Wav, "wav", "audio/wav"),
        (C::Aiff, "aiff", "audio/aiff"),
        (C::Ogg, "ogg", "audio/ogg"),
        (C::Mp4, "mp4", "audio/mp4"),
        (C::Adts, "adts", "audio/aac"),
        (C::Ape, "ape", "audio/x-ape"),
        (C::WavPack, "wavpack", "audio/x-wavpack"),
        (C::Dsf, "dsf", "audio/x-dsf"),
        (C::Dsdiff, "dsdiff", "audio/x-dff"),
    ] {
        assert_eq!(container.as_str(), name, "{container:?}");
        assert_eq!(C::from_name(name), container, "round trip failed for {container:?}");
        assert_eq!(container.http_mime(), mime, "{container:?}");
    }
}

/// A container bae names no type for keeps the probe's name through storage
/// and is served as plain bytes.
#[test]
fn an_unnamed_container_keeps_its_name_and_is_plain_bytes() {
    let container = C::Other("demuxer:w64".to_string());
    assert_eq!(C::from_name(container.as_str()), container);
    assert_eq!(container.http_mime(), "application/octet-stream");
}
