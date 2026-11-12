use super::*;

/// The formats cross in core's groups and order, each with the codec it
/// starts at and, when lossy, the bitrates core's preset check accepts.
#[test]
fn each_format_crosses_with_cores_default_and_bitrates() {
    let groups = bridge_save_format_groups();
    assert_eq!(
        groups,
        vec![
            vec![BridgeSaveFormat::Flac, BridgeSaveFormat::Wav, BridgeSaveFormat::Aiff],
            vec![BridgeSaveFormat::Mp3, BridgeSaveFormat::Aac, BridgeSaveFormat::OpusOgg],
        ]
    );
    for format in groups.into_iter().flatten() {
        let codec = bridge_save_format_default_codec(format);
        assert_eq!(bridge_save_codec_format(&codec), format);
    }
    assert_eq!(
        bridge_save_format_default_codec(BridgeSaveFormat::Mp3),
        BridgeSaveCodec::Mp3 { bitrate_kbps: 320 }
    );
    assert_eq!(
        bridge_save_format_bitrates_kbps(BridgeSaveFormat::Mp3),
        Some(vec![32, 40, 48, 56, 64, 80, 96, 112, 128, 160, 192, 224, 256, 320])
    );
    assert_eq!(
        bridge_save_format_bitrates_kbps(BridgeSaveFormat::OpusOgg),
        Some(vec![64, 96, 128, 160, 192, 256])
    );
    assert_eq!(bridge_save_format_bitrates_kbps(BridgeSaveFormat::Flac), None);
}

/// Switching format crosses as core carries the setting over.
#[test]
fn switching_a_codec_to_a_format_crosses_as_core_does_it() {
    assert_eq!(
        bridge_save_codec_switched_to(
            &BridgeSaveCodec::Flac { bit_depth: BridgeSaveBitDepth::Bits16 },
            BridgeSaveFormat::Aiff
        ),
        BridgeSaveCodec::Aiff { bit_depth: BridgeSaveBitDepth::Bits16 }
    );
    assert_eq!(
        bridge_save_codec_switched_to(
            &BridgeSaveCodec::Mp3 { bitrate_kbps: 40 },
            BridgeSaveFormat::Aac
        ),
        BridgeSaveCodec::Aac { bitrate_kbps: 96 }
    );
}

/// A codec's extension crosses as core gives it.
#[test]
fn each_codecs_extension_and_name_cross_as_core_gives_them() {
    for format in bridge_save_format_groups().into_iter().flatten() {
        let codec = bridge_save_format_default_codec(format);
        let core = codec.clone().into_core();
        assert_eq!(bridge_save_codec_extension(&codec), core.extension());
        assert_eq!(bridge_save_format_name(format), format.into_core().name());
    }
    assert_eq!(bridge_save_codec_extension(&BridgeSaveCodec::Aac { bitrate_kbps: 256 }), "m4a");
}

/// The name rules cross as core states them: a prefilled name follows a
/// format change, and a name is taken only by another format.
#[test]
fn the_name_rules_cross_as_core_states_them() {
    assert_eq!(
        bridge_save_name_after_format_change(
            "FLAC (2)".to_string(),
            BridgeSaveFormat::Flac,
            BridgeSaveFormat::Mp3,
            vec!["MP3".to_string()],
            "0".to_string(),
        )
        .unwrap(),
        "MP3 (2)"
    );
    assert!(bridge_save_name_after_format_change(
        "FLAC".to_string(),
        BridgeSaveFormat::Flac,
        BridgeSaveFormat::Mp3,
        Vec::new(),
        "x".to_string(),
    )
    .is_err());
    let preset = |id: &str, name: &str| BridgeSavePreset {
        id: id.to_string(),
        name: name.to_string(),
        codec: bridge_save_format_default_codec(BridgeSaveFormat::Flac),
    };
    let presets = vec![preset("a", "FLAC"), preset("b", "MP3")];
    assert!(bridge_save_name_taken("MP3".to_string(), "a".to_string(), presets.clone()));
    assert!(!bridge_save_name_taken("MP3".to_string(), "b".to_string(), presets.clone()));
    assert!(bridge_save_preset_can_delete(presets.clone()));
    assert!(!bridge_save_preset_can_delete(presets[..1].to_vec()));
}
