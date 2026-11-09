use super::*;

/// The formats cross in core's order, each with the codec it starts at and,
/// when lossy, the bitrates core's preset check accepts.
#[test]
fn each_format_crosses_with_cores_default_and_bitrates() {
    let formats = bridge_save_formats();
    assert_eq!(
        formats,
        vec![
            BridgeSaveFormat::Flac,
            BridgeSaveFormat::Mp3,
            BridgeSaveFormat::Aac,
            BridgeSaveFormat::OpusOgg,
            BridgeSaveFormat::Wav,
            BridgeSaveFormat::Aiff,
        ]
    );
    for format in formats {
        let codec = bridge_save_format_default_codec(format);
        assert_eq!(bridge_save_codec_format(&codec), format);
    }
    assert_eq!(
        bridge_save_format_default_codec(BridgeSaveFormat::Mp3),
        BridgeSaveCodec::Mp3 { bitrate_kbps: 320 }
    );
    assert_eq!(
        bridge_save_format_bitrates_kbps(BridgeSaveFormat::Mp3),
        Some(BridgeKbpsRange { min: 32, max: 320 })
    );
    assert_eq!(
        bridge_save_format_bitrates_kbps(BridgeSaveFormat::OpusOgg),
        Some(BridgeKbpsRange { min: 32, max: 512 })
    );
    assert_eq!(bridge_save_format_bitrates_kbps(BridgeSaveFormat::Flac), None);
}

/// A codec's extension and whether it can write one file with a CUE sheet
/// cross as core gives them.
#[test]
fn each_codecs_facts_cross_as_core_gives_them() {
    for format in bridge_save_formats() {
        let codec = bridge_save_format_default_codec(format);
        let core = codec.clone().into_core();
        assert_eq!(bridge_save_codec_extension(&codec), core.extension());
        assert_eq!(
            bridge_save_codec_supports_single_file_cue(&codec),
            core.supports_single_file_cue()
        );
        assert_eq!(bridge_save_format_name(format), format.into_core().name());
    }
    assert_eq!(bridge_save_codec_extension(&BridgeSaveCodec::Aac { bitrate_kbps: 256 }), "m4a");
    assert!(!bridge_save_codec_supports_single_file_cue(&BridgeSaveCodec::OpusOgg {
        bitrate_kbps: 192
    }));
}
