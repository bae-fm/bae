use super::*;

#[test]
fn single_file_cue_preset_is_release_only() {
    let preset = SavePreset {
        id: "flac-image".to_string(),
        name: "FLAC image".to_string(),
        codec: SaveCodec::Flac { bit_depth: SaveBitDepth::Bits24 },
        filename_tokens: default_save_filename_tokens(),
        pregap_placement: SavePregapPlacement::SingleFileWithCue,
        applies_to_track: true,
        applies_to_release: true,
        embed_cover: true,
    };

    let err = preset.validate().expect_err("single-file CUE cannot be a track export preset");
    assert!(err.to_string().contains("cannot apply to track export"));

    let release_only = SavePreset { applies_to_track: false, ..preset };
    release_only.validate().unwrap();

    let opus_image = SavePreset {
        id: "opus-image".to_string(),
        name: "Opus image".to_string(),
        codec: SaveCodec::OpusOgg { bitrate_kbps: 192 },
        filename_tokens: default_save_filename_tokens(),
        pregap_placement: SavePregapPlacement::SingleFileWithCue,
        applies_to_track: false,
        applies_to_release: true,
        embed_cover: true,
    };
    let err = opus_image.validate().expect_err("single-file CUE requires a CUE-compatible codec");
    assert!(err.to_string().contains("unsupported codec"));
}

#[test]
fn aac_bitrate_is_validated_against_its_bounds() {
    let preset = |bitrate_kbps: u32| SavePreset {
        id: "aac".to_string(),
        name: "AAC".to_string(),
        codec: SaveCodec::Aac { bitrate_kbps },
        filename_tokens: default_save_filename_tokens(),
        pregap_placement: default_export_pregap_placement(),
        applies_to_track: true,
        applies_to_release: true,
        embed_cover: true,
    };

    preset(32).validate().unwrap();
    preset(256).validate().unwrap();
    preset(512).validate().unwrap();

    let err = preset(31).validate().expect_err("a bitrate below the minimum must be rejected");
    assert!(err.to_string().contains("unsupported AAC bitrate"));
    let err = preset(513).validate().expect_err("a bitrate above the maximum must be rejected");
    assert!(err.to_string().contains("unsupported AAC bitrate"));
}
