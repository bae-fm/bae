use super::*;

const DEFAULT_PREFERENCES: &str = include_str!("../../test-fixtures/preferences/default.toml");

fn read_preferences_text(content: &str) -> Result<Preferences, ConfigError> {
    let tmp = TempDir::new().unwrap();
    std::fs::write(tmp.path().join(PREFERENCES_FILENAME), content).unwrap();
    read_preferences(tmp.path())
}

fn default_preferences_value() -> toml::Value {
    toml::from_str(DEFAULT_PREFERENCES).unwrap()
}

fn read_preferences_value(value: &toml::Value) -> Result<Preferences, ConfigError> {
    read_preferences_text(&toml::to_string_pretty(value).unwrap())
}

/// The hand-editable file spells every default, including the absence of a
/// Discogs key. Opening it and writing it again must keep every preference.
#[test]
fn defaults_roundtrip_through_preferences_toml() {
    assert_eq!(PREFERENCES_FILENAME, "preferences.toml");
    let tmp = TempDir::new().unwrap();
    let config = make_test_config(tmp.path().to_path_buf());
    config.save_preferences().unwrap();

    let written = std::fs::read_to_string(tmp.path().join("preferences.toml")).unwrap();
    assert_eq!(
        toml::from_str::<toml::Value>(&written).unwrap(),
        default_preferences_value()
    );
    assert_eq!(read_preferences(tmp.path()).unwrap(), config.prefs);
    assert_eq!(
        read_preferences_text(DEFAULT_PREFERENCES).unwrap(),
        config.prefs
    );
}

/// Spell every field without a struct-update default, so adding a preference
/// requires choosing its non-default test value too.
#[test]
fn every_preference_roundtrips_when_set() {
    let tmp = TempDir::new().unwrap();
    let mut config = make_test_config(tmp.path().to_path_buf());
    config.prefs = Preferences {
        discogs: Some(DiscogsValidation::Valid),
        replay_gain_mode: ReplayGainMode::Album,
        save_presets: vec![SavePreset {
            id: "wav".to_string(),
            name: "WAV \"24-bit\" — 設定".to_string(),
            codec: SaveCodec::Wav {
                bit_depth: SaveBitDepth::Bits24,
            },
            filename_tokens: vec![
                SaveFilenameToken::Artist,
                SaveFilenameToken::Album,
                SaveFilenameToken::Year,
                SaveFilenameToken::DiscNumber,
                SaveFilenameToken::TrackNumber,
                SaveFilenameToken::TrackTotal,
                SaveFilenameToken::Title,
            ],
            pregap_placement: SavePregapPlacement::Exclude,
            applies_to_track: true,
            applies_to_release: true,
            embed_cover: false,
        }],
        default_track_save_preset: "wav".to_string(),
        default_release_save_preset: "wav".to_string(),
        pause_between_sides: false,
        side_pause_countdown: SidePauseCountdown::Seconds30,
        max_concurrent_uploads: NonZeroU32::new(7).unwrap(),
        max_concurrent_downloads: NonZeroU32::new(4).unwrap(),
        show_remaining_time: true,
        library_full_width: true,
        verify_decode_on_import: false,
        import_storage: ImportStoragePreferences {
            cloud: false,
            pinned: false,
        },
        identification: IdentificationPreferences {
            automatic: false,
            import_when_identified: true,
            catalogs: LookupCatalogPreferences {
                musicbrainz: false,
                discogs: false,
            },
        },
        cast_enabled: true,
        cast_transcode_format: CastTranscodeFormat::Wav,
        mcp: McpConfig {
            enabled: true,
            port: 47778,
        },
        subsonic: SubsonicConfig {
            enabled: true,
            port: 4534,
            username: "listener".to_string(),
            bind_address: "::1".to_string(),
        },
    };
    config.save_preferences().unwrap();
    assert_eq!(read_preferences(tmp.path()).unwrap(), config.prefs);
}

#[test]
fn discogs_states_roundtrip_with_explicit_spelling() {
    let tmp = TempDir::new().unwrap();
    let mut config = make_test_config(tmp.path().to_path_buf());
    for (state, spelling) in [
        (Some(DiscogsValidation::Unvalidated), "Unvalidated"),
        (Some(DiscogsValidation::Valid), "Valid"),
        (Some(DiscogsValidation::Rejected), "Rejected"),
        (None, "NotConfigured"),
    ] {
        config.prefs.discogs = state;
        config.save_preferences().unwrap();
        let written = std::fs::read_to_string(tmp.path().join(PREFERENCES_FILENAME)).unwrap();
        let value: toml::Value = toml::from_str(&written).unwrap();
        assert_eq!(value["discogs"].as_str(), Some(spelling));
        assert_eq!(read_preferences(tmp.path()).unwrap().discogs, state);
    }
}

#[test]
fn replay_gain_modes_roundtrip() {
    let tmp = TempDir::new().unwrap();
    let mut config = make_test_config(tmp.path().to_path_buf());
    for mode in [
        ReplayGainMode::Off,
        ReplayGainMode::Track,
        ReplayGainMode::Album,
    ] {
        config.prefs.replay_gain_mode = mode;
        config.save_preferences().unwrap();
        assert_eq!(read_preferences(tmp.path()).unwrap().replay_gain_mode, mode);
    }
}

#[test]
fn save_codec_variants_roundtrip() {
    let tmp = TempDir::new().unwrap();
    let mut config = make_test_config(tmp.path().to_path_buf());
    for codec in [
        SaveCodec::Flac {
            bit_depth: SaveBitDepth::Source,
        },
        SaveCodec::Mp3 { bitrate_kbps: 192 },
        SaveCodec::Aac { bitrate_kbps: 256 },
        SaveCodec::OpusOgg { bitrate_kbps: 128 },
        SaveCodec::Wav {
            bit_depth: SaveBitDepth::Bits16,
        },
        SaveCodec::Aiff {
            bit_depth: SaveBitDepth::Bits32,
        },
    ] {
        config.prefs.save_presets[0].codec = codec.clone();
        config.save_preferences().unwrap();
        assert_eq!(
            read_preferences(tmp.path()).unwrap().save_presets[0].codec,
            codec
        );
    }
}

#[test]
fn missing_preferences_file_means_defaults() {
    let tmp = TempDir::new().unwrap();
    assert_eq!(
        read_preferences(tmp.path()).unwrap(),
        Preferences::default()
    );
}

#[test]
fn invalid_preferences_fail_with_the_file_path() {
    for content in ["", "discogs = [", "discogs = 42", "discogs = \"Unknown\""] {
        let tmp = TempDir::new().unwrap();
        let path = tmp.path().join(PREFERENCES_FILENAME);
        std::fs::write(&path, content).unwrap();
        let error = read_preferences(tmp.path()).unwrap_err();
        assert!(matches!(error, ConfigError::Serialization(_)));
        assert!(error.to_string().contains(path.to_str().unwrap()));
    }
}

#[test]
fn unreadable_preferences_fail_with_the_file_path() {
    let tmp = TempDir::new().unwrap();
    let path = tmp.path().join(PREFERENCES_FILENAME);
    std::fs::create_dir(&path).unwrap();
    let error = read_preferences(tmp.path()).unwrap_err();
    assert!(matches!(error, ConfigError::Io(_)));
    assert!(error.to_string().contains(path.to_str().unwrap()));
}

#[test]
fn preferences_require_every_field() {
    let defaults = default_preferences_value();
    for key in defaults.as_table().unwrap().keys() {
        let mut value = defaults.clone();
        value.as_table_mut().unwrap().remove(key).unwrap();
        assert!(
            read_preferences_value(&value).is_err(),
            "preferences should fail without {key}"
        );
    }
    for table in ["import_storage", "identification", "mcp", "subsonic"] {
        for key in defaults[table].as_table().unwrap().keys() {
            let mut value = defaults.clone();
            value[table].as_table_mut().unwrap().remove(key).unwrap();
            assert!(
                read_preferences_value(&value).is_err(),
                "preferences should fail without {table}.{key}"
            );
        }
    }
    for key in defaults["identification"]["catalogs"]
        .as_table()
        .unwrap()
        .keys()
    {
        let mut value = defaults.clone();
        value["identification"]["catalogs"]
            .as_table_mut()
            .unwrap()
            .remove(key)
            .unwrap();
        assert!(
            read_preferences_value(&value).is_err(),
            "preferences should fail without identification.catalogs.{key}"
        );
    }
}

/// Extra keys edited in by hand do not prevent loading, at either the root
/// or inside a settings table.
#[test]
fn preferences_carrying_an_unrecognized_key_load() {
    let mut value = default_preferences_value();
    value
        .as_table_mut()
        .unwrap()
        .insert("unrecognized_setting".to_string(), "value".into());
    for table in ["import_storage", "identification", "mcp", "subsonic"] {
        value[table]
            .as_table_mut()
            .unwrap()
            .insert("unrecognized_setting".to_string(), "value".into());
    }
    assert_eq!(
        read_preferences_value(&value).unwrap(),
        Preferences::default()
    );
}

#[test]
fn a_zero_concurrency_fails_to_load() {
    for key in ["max_concurrent_uploads", "max_concurrent_downloads"] {
        let mut value = default_preferences_value();
        value[key] = 0.into();
        assert!(
            read_preferences_value(&value).is_err(),
            "{key} cannot be zero"
        );
    }
}
