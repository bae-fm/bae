//! The `preferences` row and its save presets: what a new library starts
//! with, what each edit writes and reads back, what the live query delivers,
//! and what the schema refuses.

use super::*;
use bae_store::test_fixtures::*;
use bae_util::test_channels::HANG_GUARD;
use std::num::NonZeroU32;

/// Edit the preferences as a setter does, refusing nothing.
async fn set(db: &Store, edit: impl FnOnce(&mut Preferences) + Send + 'static) {
    PreferencesRow::new(db.clone())
        .edit(|prefs| {
            edit(prefs);
            Ok(())
        })
        .await
        .unwrap();
}

/// The next value `values` delivers, failing the test if none ever comes.
async fn next(values: &mut coven::LiveQuery<Preferences, bae_store::DbError>) -> Preferences {
    tokio::time::timeout(HANG_GUARD, values.next())
        .await
        .expect("the live query delivers")
        .expect("the store is open")
        .unwrap()
}

/// A new library's row holds the defaults Appendix C lists, at the transfer
/// limits a library is loaded from storage under.
#[tokio::test]
async fn a_new_library_starts_at_the_defaults() {
    let (db, _tmp) = temp_db(crate::TEST_SCHEMA).await;
    let prefs = preferences(&db).await.unwrap();
    assert_eq!(prefs, Preferences::default());
    assert_eq!(prefs.transfer_limits(), bae_app_dir::default_transfer_limits());
    assert_eq!(prefs.max_concurrent_uploads.get(), bae_app_dir::DEFAULT_CONCURRENT_TRANSFERS);
    assert_eq!(
        prefs.save_presets[0].codec,
        SaveCodec::Flac { bit_depth: SaveBitDepth::SameAsSource },
        "the default FLAC preset writes the source's depth"
    );
    assert_eq!(
        prefs.save_filename_pattern.pieces(),
        [
            NamePiece::Value(SaveFilenameValue::TrackNumber),
            NamePiece::Text(" ".to_string()),
            NamePiece::Value(SaveFilenameValue::Title),
        ],
        "every format starts named by track number, a space, then title"
    );
    assert_eq!(
        prefs.save_folder_pattern.pieces(),
        [
            NamePiece::Value(SaveFolderValue::Artist),
            NamePiece::Text(" - ".to_string()),
            NamePiece::Value(SaveFolderValue::Album),
        ],
        "a release save starts naming its folder by artist, \" - \", then album"
    );
}

/// Every preference set away from its default reads back as set. Spelled
/// without a struct-update default, so a new preference needs its own value
/// here.
#[tokio::test]
async fn every_preference_reads_back_as_set() {
    let (db, _tmp) = temp_db(crate::TEST_SCHEMA).await;
    let edited = Preferences {
        discogs: Some(DiscogsValidation::Valid),
        replay_gain_mode: ReplayGainMode::Album,
        save_presets: vec![
            SavePreset {
                id: "wav".to_string(),
                name: "WAV \"24-bit\" — 設定".to_string(),
                codec: SaveCodec::Wav { bit_depth: SaveBitDepth::SameAsSource },
            },
            SavePreset {
                id: "sixteen".to_string(),
                name: "FLAC 16-bit".to_string(),
                codec: SaveCodec::Flac { bit_depth: SaveBitDepth::Bits16 },
            },
        ],
        save_filename_pattern: NamePattern::new(vec![
            NamePiece::Value(SaveFilenameValue::Artist),
            NamePiece::Text("  /  ".to_string()),
            NamePiece::Value(SaveFilenameValue::Album),
            NamePiece::Value(SaveFilenameValue::AlbumYear),
            NamePiece::Value(SaveFilenameValue::ReleaseYear),
            NamePiece::Value(SaveFilenameValue::DiscNumber),
            NamePiece::Value(SaveFilenameValue::TrackNumber),
            NamePiece::Value(SaveFilenameValue::TrackTotal),
            NamePiece::Text(".\\ \"設定\"".to_string()),
            NamePiece::Value(SaveFilenameValue::Title),
        ]),
        save_folder_pattern: NamePattern::new(vec![
            NamePiece::Text(" [".to_string()),
            NamePiece::Value(SaveFolderValue::CatalogNumber),
            NamePiece::Value(SaveFolderValue::Label),
            NamePiece::Value(SaveFolderValue::AlbumYear),
            NamePiece::Value(SaveFolderValue::ReleaseYear),
            NamePiece::Value(SaveFolderValue::Album),
            NamePiece::Value(SaveFolderValue::Artist),
        ]),
        save_extras: SaveExtras { embed_cover: false, cover_file: false, loudness_tags: false },
        last_save_preset: "sixteen".to_string(),
        pause_between_sides: false,
        side_pause_countdown: SidePauseCountdown::Seconds30,
        max_concurrent_uploads: NonZeroU32::new(7).unwrap(),
        max_concurrent_downloads: NonZeroU32::new(4).unwrap(),
        show_remaining_time: true,
        library_full_width: true,
        import_destination: ImportDestination::ThisDevice,
        identification: IdentificationPreferences {
            automatic: false,
            import_when_identified: true,
            catalogs: LookupCatalogPreferences { musicbrainz: false, discogs: true },
        },
        cast_enabled: true,
        cast_transcode_format: CastTranscodeFormat::Wav,
        mcp: McpConfig { enabled: true, port: 47778 },
        subsonic: SubsonicConfig {
            enabled: true,
            port: 4534,
            username: "listener".to_string(),
            network: SubsonicNetwork::Interface("en0".to_string()),
        },
    };
    let written = edited.clone();
    set(&db, move |prefs| *prefs = written).await;
    assert_eq!(preferences(&db).await.unwrap(), edited);
}

/// Each value of each enumerated setting is stored and read back unchanged.
#[tokio::test]
async fn each_choice_of_each_setting_reads_back() {
    let (db, _tmp) = temp_db(crate::TEST_SCHEMA).await;
    for discogs in [
        None,
        Some(DiscogsValidation::Unvalidated),
        Some(DiscogsValidation::Valid),
        Some(DiscogsValidation::Rejected),
    ] {
        set(&db, move |prefs| prefs.discogs = discogs).await;
        assert_eq!(preferences(&db).await.unwrap().discogs, discogs);
    }
    for destination in [ImportDestination::ThisDevice, ImportDestination::Cloud] {
        set(&db, move |prefs| prefs.import_destination = destination).await;
        assert_eq!(preferences(&db).await.unwrap().import_destination, destination);
    }
    for mode in [ReplayGainMode::Off, ReplayGainMode::Track, ReplayGainMode::Album] {
        set(&db, move |prefs| prefs.replay_gain_mode = mode).await;
        assert_eq!(preferences(&db).await.unwrap().replay_gain_mode, mode);
    }
    for countdown in [
        SidePauseCountdown::Off,
        SidePauseCountdown::Seconds5,
        SidePauseCountdown::Seconds15,
        SidePauseCountdown::Seconds30,
        SidePauseCountdown::Seconds45,
        SidePauseCountdown::Seconds60,
    ] {
        set(&db, move |prefs| prefs.side_pause_countdown = countdown).await;
        assert_eq!(preferences(&db).await.unwrap().side_pause_countdown, countdown);
    }
    for codec in [
        SaveCodec::Flac { bit_depth: SaveBitDepth::Bits16 },
        SaveCodec::Mp3 { bitrate_kbps: 192 },
        SaveCodec::Aac { bitrate_kbps: 256 },
        SaveCodec::OpusOgg { bitrate_kbps: 128 },
        SaveCodec::Wav { bit_depth: SaveBitDepth::SameAsSource },
        SaveCodec::Aiff { bit_depth: SaveBitDepth::Bits16 },
    ] {
        let stored = codec.clone();
        set(&db, move |prefs| prefs.save_presets[0].codec = stored).await;
        assert_eq!(preferences(&db).await.unwrap().save_presets[0].codec, codec);
    }
}

/// An edit that refuses writes nothing, and the live query delivers only
/// what a write changed: an edit that leaves the preferences as they were
/// delivers nothing.
#[tokio::test]
async fn the_live_query_follows_only_what_changes() {
    let (db, _tmp) = temp_db(crate::TEST_SCHEMA).await;
    let mut values = subscribe_preferences(&db);
    assert_eq!(next(&mut values).await, Preferences::default());

    let refused = PreferencesRow::new(db.clone())
        .edit(|prefs| {
            prefs.cast_enabled = true;
            Err(PreferencesError::Invalid("refused".to_string()))
        })
        .await;
    assert!(matches!(refused, Err(PreferencesError::Invalid(_))), "{refused:?}");
    set(&db, |prefs| prefs.show_remaining_time = false).await;
    set(&db, |prefs| prefs.show_remaining_time = true).await;

    let delivered = next(&mut values).await;
    assert!(!delivered.cast_enabled, "the refused edit wrote nothing");
    assert!(delivered.show_remaining_time);
}

/// Two edits racing each keep the other's change: each edits the stored row
/// inside its own write.
#[tokio::test]
async fn concurrent_edits_keep_both_changes() {
    let (db, _tmp) = temp_db(crate::TEST_SCHEMA).await;
    let (cast, pause) = tokio::join!(
        set(&db, |prefs| prefs.cast_enabled = true),
        set(&db, |prefs| prefs.pause_between_sides = false),
    );
    let ((), ()) = (cast, pause);
    let prefs = preferences(&db).await.unwrap();
    assert!(prefs.cast_enabled);
    assert!(!prefs.pause_between_sides);
}

/// The schema refuses what the setters check before writing: a last-used format
/// that names no preset, a network named nothing, a preset its codec
/// can't write, and a pattern piece of typed text with nothing in it.
#[tokio::test]
async fn the_schema_refuses_preferences_no_setter_writes() {
    let (db, _tmp) = temp_db(crate::TEST_SCHEMA).await;
    for (what, edit) in [
        (
            "a last-used format naming no preset",
            Box::new(|prefs: &mut Preferences| prefs.last_save_preset = "gone".to_string())
                as Box<dyn FnOnce(&mut Preferences) + Send>,
        ),
        (
            "removing the preset the last-used format names",
            Box::new(|prefs: &mut Preferences| {
                prefs.save_presets.retain(|preset| preset.id != "flac")
            }),
        ),
        (
            "two formats of one name",
            Box::new(|prefs: &mut Preferences| {
                prefs.save_presets[1].name = prefs.save_presets[0].name.clone();
            }),
        ),
        (
            "a Subsonic network that is an interface named nothing",
            Box::new(|prefs: &mut Preferences| {
                prefs.subsonic.network = SubsonicNetwork::Interface(String::new());
            }),
        ),
        (
            "an MP3 bitrate past 320",
            Box::new(|prefs: &mut Preferences| {
                prefs.save_presets[1].codec = SaveCodec::Mp3 { bitrate_kbps: 384 };
            }),
        ),
        (
            "an MP3 bitrate between two the encoder writes",
            Box::new(|prefs: &mut Preferences| {
                prefs.save_presets[1].codec = SaveCodec::Mp3 { bitrate_kbps: 100 };
            }),
        ),
        (
            "an AAC bitrate below 96",
            Box::new(|prefs: &mut Preferences| {
                prefs.save_presets[1].codec = SaveCodec::Aac { bitrate_kbps: 64 };
            }),
        ),
        (
            "typed text of nothing in the file name pattern",
            Box::new(|prefs: &mut Preferences| {
                prefs.save_filename_pattern =
                    NamePattern::new(vec![NamePiece::Text(String::new())]);
            }),
        ),
        (
            "typed text of nothing in the folder name pattern",
            Box::new(|prefs: &mut Preferences| {
                prefs.save_folder_pattern = NamePattern::new(vec![
                    NamePiece::Value(SaveFolderValue::Album),
                    NamePiece::Text(String::new()),
                ]);
            }),
        ),
        (
            "an Opus bitrate above 256",
            Box::new(|prefs: &mut Preferences| {
                prefs.save_presets[1].codec = SaveCodec::OpusOgg { bitrate_kbps: 320 };
            }),
        ),
    ] {
        let refused = PreferencesRow::new(db.clone())
            .edit(|prefs| {
                edit(prefs);
                Ok(())
            })
            .await;
        assert!(matches!(refused, Err(PreferencesError::Database(_))), "{what}: {refused:?}");
        assert_eq!(preferences(&db).await.unwrap(), Preferences::default(), "{what} wrote nothing");
    }
}

/// The Subsonic settings are kept as the person set them, whether or not the
/// server can run on them: switched on with no username, and a port outside
/// 1 to 65535.
#[tokio::test]
async fn subsonic_settings_are_stored_whether_or_not_the_server_can_run_on_them() {
    let (db, _tmp) = temp_db(crate::TEST_SCHEMA).await;
    let unrunnable = SubsonicConfig {
        enabled: true,
        port: 70000,
        username: String::new(),
        network: SubsonicNetwork::Every,
    };
    let written = unrunnable.clone();
    set(&db, move |prefs| prefs.subsonic = written).await;
    assert_eq!(preferences(&db).await.unwrap().subsonic, unrunnable);
}

/// A pattern that can't make a name is stored as typed, as is one with no
/// pieces: the schema refuses only a piece that is not one thing.
#[tokio::test]
async fn a_pattern_that_cant_make_a_name_is_stored_as_typed() {
    let (db, _tmp) = temp_db(crate::TEST_SCHEMA).await;
    let invalid = NamePattern::new(vec![
        NamePiece::Text(".a/b".to_string()),
        NamePiece::Value(SaveFilenameValue::Title),
    ]);
    let edited = invalid.clone();
    set(&db, move |prefs| {
        prefs.save_filename_pattern = edited;
        prefs.save_folder_pattern = NamePattern::new(vec![]);
    })
    .await;
    let prefs = preferences(&db).await.unwrap();
    assert_eq!(prefs.save_filename_pattern, invalid);
    assert!(prefs.save_folder_pattern.pieces().is_empty());
}
