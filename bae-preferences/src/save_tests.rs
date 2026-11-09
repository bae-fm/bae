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

/// A preset switched to any format starts at a codec of that format that
/// saves, and a lossy format accepts exactly the bitrates it offers.
#[test]
fn each_format_starts_at_a_codec_of_its_own_that_saves() {
    for format in SaveFormat::ALL {
        let codec = format.default_codec();
        assert_eq!(codec.format(), format);
        let preset = |codec: SaveCodec| SavePreset {
            id: "preset".to_string(),
            name: "Preset".to_string(),
            codec,
            filename_tokens: default_save_filename_tokens(),
            pregap_placement: default_export_pregap_placement(),
            applies_to_track: true,
            applies_to_release: true,
            embed_cover: true,
        };
        preset(codec.clone()).validate().unwrap();
        if let Some(accepted) = format.bitrates_kbps() {
            let at = |bitrate_kbps| match codec {
                SaveCodec::Mp3 { .. } => SaveCodec::Mp3 { bitrate_kbps },
                SaveCodec::Aac { .. } => SaveCodec::Aac { bitrate_kbps },
                SaveCodec::OpusOgg { .. } => SaveCodec::OpusOgg { bitrate_kbps },
                SaveCodec::Flac { .. } | SaveCodec::Wav { .. } | SaveCodec::Aiff { .. } => {
                    panic!("{format:?} is lossless and has no bitrates")
                }
            };
            preset(at(*accepted.start())).validate().unwrap();
            preset(at(*accepted.end())).validate().unwrap();
            preset(at(accepted.start() - 1)).validate().expect_err("below the range");
            preset(at(accepted.end() + 1)).validate().expect_err("above the range");
        }
    }
}

/// Add format's preset: FLAC at its starting depth, named for the format,
/// numbered by title, pregaps appended except the hidden first-track audio,
/// for tracks and releases, with the cover, under an id of its own.
#[test]
fn a_new_preset_is_a_valid_flac_preset_with_its_own_id() {
    let ids = coven::SequentialIds::new();
    let preset = SavePreset::new_flac(&ids);
    let other = SavePreset::new_flac(&ids);

    preset.validate().unwrap();
    assert_ne!(preset.id, other.id, "each new preset has an id of its own");
    assert_eq!(
        SavePreset { id: "new".to_string(), ..preset },
        SavePreset {
            id: "new".to_string(),
            name: "FLAC".to_string(),
            codec: SaveCodec::Flac { bit_depth: SaveBitDepth::Bits24 },
            filename_tokens: vec![SaveFilenameToken::TrackNumber, SaveFilenameToken::Title],
            pregap_placement: SavePregapPlacement::AppendToPreviousExceptHtoa,
            applies_to_track: true,
            applies_to_release: true,
            embed_cover: true,
        }
    );
}
