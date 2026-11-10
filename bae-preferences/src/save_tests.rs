use super::*;

#[test]
fn a_lossy_format_saves_only_at_the_bitrates_its_encoder_writes() {
    let preset = |codec: SaveCodec| SavePreset {
        id: "preset".to_string(),
        name: "Preset".to_string(),
        codec,
        embed_cover: true,
    };

    preset(SaveCodec::Aac { bitrate_kbps: 96 }).validate().unwrap();
    preset(SaveCodec::Aac { bitrate_kbps: 320 }).validate().unwrap();
    // Between two listed rates the encoder rounds; the format refuses instead.
    let err = preset(SaveCodec::Aac { bitrate_kbps: 100 })
        .validate()
        .expect_err("a bitrate between two listed rates must be rejected");
    assert!(err.to_string().contains("unsupported AAC bitrate"));
    let err = preset(SaveCodec::Aac { bitrate_kbps: 64 }).validate().expect_err("AAC starts at 96");
    assert!(err.to_string().contains("unsupported AAC bitrate"));
    preset(SaveCodec::OpusOgg { bitrate_kbps: 64 }).validate().unwrap();
    preset(SaveCodec::OpusOgg { bitrate_kbps: 256 }).validate().unwrap();
    preset(SaveCodec::OpusOgg { bitrate_kbps: 320 }).validate().expect_err("Opus ends at 256");
    preset(SaveCodec::Mp3 { bitrate_kbps: 40 }).validate().unwrap();
    preset(SaveCodec::Mp3 { bitrate_kbps: 100 }).validate().expect_err("MP3 has no 100");
}

#[test]
fn each_lossy_format_lists_the_bitrates_its_encoder_writes() {
    assert_eq!(
        SaveFormat::Mp3.bitrates_kbps(),
        Some(&[32, 40, 48, 56, 64, 80, 96, 112, 128, 160, 192, 224, 256, 320][..])
    );
    assert_eq!(SaveFormat::Aac.bitrates_kbps(), Some(&[96, 128, 160, 192, 256, 320][..]));
    assert_eq!(SaveFormat::OpusOgg.bitrates_kbps(), Some(&[64, 96, 128, 160, 192, 256][..]));
    for lossless in [SaveFormat::Flac, SaveFormat::Wav, SaveFormat::Aiff] {
        assert_eq!(lossless.bitrates_kbps(), None);
    }
}

#[test]
fn formats_are_offered_lossless_first_then_lossy() {
    assert_eq!(
        SaveFormat::ALL,
        [
            SaveFormat::Flac,
            SaveFormat::Wav,
            SaveFormat::Aiff,
            SaveFormat::Mp3,
            SaveFormat::Aac,
            SaveFormat::OpusOgg,
        ]
    );
    assert_eq!(
        SaveFormat::groups(),
        vec![
            vec![SaveFormat::Flac, SaveFormat::Wav, SaveFormat::Aiff],
            vec![SaveFormat::Mp3, SaveFormat::Aac, SaveFormat::OpusOgg],
        ]
    );
}

/// A lossless format starts at the source's depth; 16 bits is the other
/// choice. A source with no depth of its own (lossy audio) writes 16 bits,
/// the depth a decoder hands out for it.
#[test]
fn a_bit_depth_resolves_against_the_source_track() {
    use std::num::NonZeroU32;
    let depth = |bits| NonZeroU32::new(bits);
    assert_eq!(SaveBitDepth::SameAsSource.bits(depth(24)), 24);
    assert_eq!(SaveBitDepth::SameAsSource.bits(depth(16)), 16);
    assert_eq!(SaveBitDepth::SameAsSource.bits(depth(32)), 32);
    assert_eq!(SaveBitDepth::SameAsSource.bits(None), 16);
    assert_eq!(SaveBitDepth::Bits16.bits(depth(24)), 16);
    assert_eq!(SaveBitDepth::Bits16.bits(None), 16);
}

/// Switching a format carries the setting that still applies: the bit depth
/// across lossless formats, the bitrate across lossy ones (to the nearest the
/// new format writes); crossing between the two starts the new format's codec.
#[test]
fn switching_format_carries_the_setting_that_still_applies() {
    assert_eq!(
        SaveCodec::Flac { bit_depth: SaveBitDepth::Bits16 }.switched_to(SaveFormat::Aiff),
        SaveCodec::Aiff { bit_depth: SaveBitDepth::Bits16 }
    );
    assert_eq!(
        SaveCodec::Aac { bitrate_kbps: 320 }.switched_to(SaveFormat::Mp3),
        SaveCodec::Mp3 { bitrate_kbps: 320 }
    );
    assert_eq!(
        SaveCodec::Mp3 { bitrate_kbps: 40 }.switched_to(SaveFormat::Aac),
        SaveCodec::Aac { bitrate_kbps: 96 }
    );
    assert_eq!(
        SaveCodec::Mp3 { bitrate_kbps: 320 }.switched_to(SaveFormat::OpusOgg),
        SaveCodec::OpusOgg { bitrate_kbps: 256 }
    );
    assert_eq!(
        SaveCodec::Flac { bit_depth: SaveBitDepth::Bits16 }.switched_to(SaveFormat::Aac),
        SaveCodec::Aac { bitrate_kbps: 256 }
    );
    assert_eq!(
        SaveCodec::OpusOgg { bitrate_kbps: 96 }.switched_to(SaveFormat::Wav),
        SaveCodec::Wav { bit_depth: SaveBitDepth::SameAsSource }
    );
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
            for bitrate_kbps in accepted {
                preset(at(*bitrate_kbps)).validate().unwrap();
            }
            preset(at(accepted[0] - 1)).validate().expect_err("below the list");
            preset(at(accepted[accepted.len() - 1] + 1)).validate().expect_err("above the list");
        }
    }
}

/// Add format's preset: FLAC at its starting depth, named for the format,
#[test]
fn a_new_preset_is_a_valid_flac_preset_with_its_own_id() {
    let ids = coven::SequentialIds::new();
    let preset = SavePreset::new_flac(&ids, &[]);
    let other = SavePreset::new_flac(&ids, &[]);

    preset.validate().unwrap();
    assert_ne!(preset.id, other.id, "each new preset has an id of its own");
    assert_eq!(
        SavePreset { id: "new".to_string(), ..preset },
        SavePreset {
            id: "new".to_string(),
            name: "FLAC".to_string(),
            codec: SaveCodec::Flac { bit_depth: SaveBitDepth::SameAsSource },
            embed_cover: true,
        }
    );
}

/// A new format takes its format's name, then the format's name and the next
/// free number from 2: a name taken by another format, whatever its codec, is
/// skipped.
#[test]
fn a_new_format_is_named_with_the_next_free_number() {
    let ids = coven::SequentialIds::new();
    let named =
        |name: &str| SavePreset { name: name.to_string(), ..SavePreset::new_flac(&ids, &[]) };

    assert_eq!(SavePreset::new_flac(&ids, &[]).name, "FLAC");
    assert_eq!(SavePreset::new_flac(&ids, &[named("FLAC")]).name, "FLAC (2)");
    assert_eq!(SavePreset::new_flac(&ids, &[named("FLAC"), named("FLAC (2)")]).name, "FLAC (3)");
    assert_eq!(
        SavePreset::new_flac(&ids, &[named("FLAC"), named("FLAC (3)")]).name,
        "FLAC (2)",
        "the first free number, not the one after the last"
    );
    assert_eq!(SavePreset::new_flac(&ids, &[named("Phone")]).name, "FLAC");
}

/// Changing a format re-prefills its name while the name is still a prefill of
/// the format it had, and leaves a name the person wrote alone.
#[test]
fn a_prefilled_name_follows_a_format_change() {
    let change =
        |name: &str, from, to, taken: &[&str]| SaveFormat::name_after_change(name, from, to, taken);
    assert_eq!(change("FLAC", SaveFormat::Flac, SaveFormat::Mp3, &[]), "MP3");
    assert_eq!(change("FLAC (2)", SaveFormat::Flac, SaveFormat::Mp3, &["MP3"]), "MP3 (2)");
    assert_eq!(change("MP3 (2)", SaveFormat::Mp3, SaveFormat::Flac, &["FLAC"]), "FLAC (2)");
    assert_eq!(change("Phone", SaveFormat::Flac, SaveFormat::Mp3, &["MP3"]), "Phone");
    assert_eq!(change("FLAC (1)", SaveFormat::Flac, SaveFormat::Mp3, &[]), "FLAC (1)");
    assert_eq!(change("FLAC of mine", SaveFormat::Flac, SaveFormat::Mp3, &[]), "FLAC of mine");
}

/// A name is taken when another format has it; a format's own name is not.
#[test]
fn a_name_is_taken_only_by_another_format() {
    let ids = coven::SequentialIds::new();
    let flac = SavePreset::new_flac(&ids, &[]);
    let mp3 = SavePreset { name: "MP3".to_string(), ..SavePreset::new_flac(&ids, &[]) };
    let presets = vec![flac.clone(), mp3.clone()];

    assert!(SavePreset::name_taken("MP3", &flac.id, &presets));
    assert!(!SavePreset::name_taken("MP3", &mp3.id, &presets));
    assert!(!SavePreset::name_taken("Phone", &flac.id, &presets));
}
