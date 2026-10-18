use super::*;
use std::sync::{Arc, Barrier};
use std::time::Duration;
use tempfile::TempDir;

fn make_test_config(library_path: PathBuf) -> Config {
    Config::with_defaults(coven::StoreId(uuid::Uuid::new_v4()), library_path)
}

#[test]
fn export_settings_survive_toml_roundtrip() {
    let tmp = TempDir::new().unwrap();
    let mut config = make_test_config(tmp.path().to_path_buf());
    // Filename tokens are per-preset now; a preset's edited pattern survives.
    config.prefs.save_presets[0].filename_tokens =
        vec![SaveFilenameToken::Artist, SaveFilenameToken::Title];
    config.save_preferences().unwrap();

    let prefs = read_preferences(tmp.path()).unwrap();
    assert_eq!(
        prefs.save_presets[0].filename_tokens,
        vec![SaveFilenameToken::Artist, SaveFilenameToken::Title]
    );
    assert_eq!(prefs.save_presets, config.prefs.save_presets);
    assert_eq!(
        prefs.default_track_save_preset,
        config.prefs.default_track_save_preset
    );
    assert_eq!(
        prefs.default_release_save_preset,
        config.prefs.default_release_save_preset
    );
}

#[test]
fn transfer_concurrency_defaults_to_three() {
    let tmp = TempDir::new().unwrap();
    let config = make_test_config(tmp.path().to_path_buf());
    assert_eq!(config.prefs.max_concurrent_uploads.get(), 3);
    assert_eq!(config.prefs.max_concurrent_downloads.get(), 3);
}

#[test]
fn validate_concurrency_bounds() {
    assert!(
        validate_concurrency(0).is_err(),
        "0 deadlocks coven's drain"
    );
    assert_eq!(validate_concurrency(1).unwrap().get(), 1);
    assert_eq!(
        validate_concurrency(MAX_CONCURRENT_TRANSFERS)
            .unwrap()
            .get(),
        MAX_CONCURRENT_TRANSFERS
    );
    assert!(
        validate_concurrency(MAX_CONCURRENT_TRANSFERS + 1).is_err(),
        "above the range is refused"
    );
}

/// The wire this feature exists for: the value stored in `Config` is what the
/// coven builder is handed. `usize_bound` is the exact conversion the
/// `Coven::builder` chain applies to each field at open; if it dropped or
/// clamped the value, the knob would move nothing.
#[test]
fn stored_concurrency_widens_to_the_builder_bound() {
    let tmp = TempDir::new().unwrap();
    let mut config = make_test_config(tmp.path().to_path_buf());
    config.prefs.max_concurrent_uploads = NonZeroU32::new(5).unwrap();
    config.prefs.max_concurrent_downloads = NonZeroU32::new(2).unwrap();
    assert_eq!(usize_bound(config.prefs.max_concurrent_uploads).get(), 5);
    assert_eq!(usize_bound(config.prefs.max_concurrent_downloads).get(), 2);
}

#[test]
fn transfer_concurrency_survives_toml_roundtrip() {
    let tmp = TempDir::new().unwrap();
    let mut config = make_test_config(tmp.path().to_path_buf());
    config.prefs.max_concurrent_uploads = NonZeroU32::new(7).unwrap();
    config.prefs.max_concurrent_downloads = NonZeroU32::new(4).unwrap();
    config.save_preferences().unwrap();

    let prefs = read_preferences(tmp.path()).unwrap();
    assert_eq!(prefs.max_concurrent_uploads.get(), 7);
    assert_eq!(prefs.max_concurrent_downloads.get(), 4);
}

#[test]
fn a_new_library_identifies_automatically() {
    let tmp = TempDir::new().unwrap();
    let config = make_test_config(tmp.path().to_path_buf());

    assert!(config.prefs.identification.automatic);
}

/// Nothing is imported on its own until the person says so.
#[test]
fn a_new_library_imports_nothing_on_its_own() {
    let prefs = IdentificationPreferences::default();
    assert!(!prefs.import_when_identified);
    assert!(!prefs.imports_when_identified());
}

/// Importing when identified is read only while identification runs on its
/// own, and kept while it does not.
#[test]
fn importing_when_identified_needs_automatic_identification() {
    let mut prefs = IdentificationPreferences {
        import_when_identified: true,
        ..IdentificationPreferences::default()
    };
    assert!(prefs.imports_when_identified());
    prefs.automatic = false;
    assert!(!prefs.imports_when_identified());
    assert!(prefs.import_when_identified, "the choice is kept");
}

/// An import goes where the stored choice says, pinned as it says, and to the
/// cloud only when there is a home to go to.
#[test]
fn an_import_s_destination_is_the_stored_choice() {
    use crate::import::ImportDestination;
    let tmp = TempDir::new().unwrap();
    let mut config = make_test_config(tmp.path().to_path_buf());
    assert_eq!(
        config.import_destination(false),
        ImportDestination::Local,
        "without storage, imports stay local"
    );
    assert_eq!(
        config.import_destination(true),
        ImportDestination::Remote { pin: true }
    );
    config.prefs.import_storage.pinned = false;
    assert_eq!(
        config.import_destination(true),
        ImportDestination::Remote { pin: false }
    );
    config.prefs.import_storage.cloud = false;
    assert_eq!(config.import_destination(true), ImportDestination::Local);
}

#[test]
fn identify_automatically_roundtrips() {
    for identify in [true, false] {
        let tmp = TempDir::new().unwrap();
        let mut config = make_test_config(tmp.path().to_path_buf());
        config.prefs.identification.automatic = identify;
        config.save_preferences().unwrap();

        let prefs = read_preferences(tmp.path()).unwrap();

        assert_eq!(prefs.identification.automatic, identify);
    }
}

#[test]
fn subsonic_config_rejects_port_zero() {
    let config = SubsonicConfig {
        enabled: true,
        port: 0,
        username: "listener".to_string(),
        bind_address: "127.0.0.1".to_string(),
    };
    assert!(config.validate().is_err(), "port 0 is not a real endpoint");
}

#[test]
fn subsonic_config_rejects_enabled_without_username() {
    let config = SubsonicConfig {
        enabled: true,
        port: SUBSONIC_DEFAULT_PORT,
        username: String::new(),
        bind_address: "127.0.0.1".to_string(),
    };
    assert!(
        config.validate().is_err(),
        "an enabled server with no username authenticates no one"
    );
}

#[test]
fn subsonic_config_rejects_non_ip_bind_address() {
    let config = SubsonicConfig {
        enabled: true,
        port: SUBSONIC_DEFAULT_PORT,
        username: "listener".to_string(),
        bind_address: "not-an-ip".to_string(),
    };
    assert!(
        config.validate().is_err(),
        "a bind address that isn't an IP must be rejected"
    );
}

#[test]
fn subsonic_config_allows_lan_bind_address() {
    let config = SubsonicConfig {
        enabled: true,
        port: SUBSONIC_DEFAULT_PORT,
        username: "listener".to_string(),
        bind_address: "0.0.0.0".to_string(),
    };
    assert!(
        config.validate().is_ok(),
        "0.0.0.0 opens the server to the network and is valid"
    );
}

#[test]
fn subsonic_config_allows_disabled_without_username() {
    let config = SubsonicConfig::disabled_default();
    assert!(config.username.is_empty());
    assert_eq!(config.bind_address, "127.0.0.1");
    assert!(
        config.validate().is_ok(),
        "a disabled server needs no username"
    );
}

/// Casting reaches the local network, so it is opt-in: a fresh library has
/// it off, and the choice survives a write/read of preferences.toml.
#[test]
fn cast_is_off_by_default_and_survives_toml_roundtrip() {
    let tmp = TempDir::new().unwrap();
    let mut config = make_test_config(tmp.path().to_path_buf());
    assert!(!config.prefs.cast_enabled, "casting is opt-in");

    config.prefs.cast_enabled = true;
    config.save_preferences().unwrap();

    assert!(read_preferences(tmp.path()).unwrap().cast_enabled);
}

/// What a device is sent when a track must be converted is MP3 until the
/// person picks WAV, and the pick survives a write/read of preferences.toml.
#[test]
fn cast_transcode_format_is_mp3_by_default_and_survives_toml_roundtrip() {
    let tmp = TempDir::new().unwrap();
    let mut config = make_test_config(tmp.path().to_path_buf());
    assert_eq!(config.prefs.cast_transcode_format, CastTranscodeFormat::Mp3);

    config.prefs.cast_transcode_format = CastTranscodeFormat::Wav;
    config.save_preferences().unwrap();

    assert_eq!(
        read_preferences(tmp.path()).unwrap().cast_transcode_format,
        CastTranscodeFormat::Wav
    );
}

/// The TOML table names its fields, but nothing reads them by name: the
/// accessors are total over the catalogs bae asks, so each has an answer and a
/// new one cannot be silently forgotten.
#[test]
fn lookup_catalog_preferences_are_total_over_the_asked_catalogs() {
    let mut prefs = LookupCatalogPreferences::default();
    for catalog in crate::import::Catalog::LOOKUP {
        assert!(prefs.enabled(catalog), "every asked catalog starts asked");
    }

    prefs.set(crate::import::Catalog::Discogs, false);
    assert!(prefs.enabled(crate::import::Catalog::MusicBrainz));
    assert!(!prefs.enabled(crate::import::Catalog::Discogs));
}

/// The availability list folds two questions that are not the same — whether
/// this library holds what a source needs, and whether the person wants it
/// asked — into the one answer everything that asks the sources together
/// reads. Unreachable beats switched-off, so a source with no credential says
/// which of the two to fix while the person's switch waits underneath it.
#[test]
fn metadata_sources_fold_the_credential_and_the_switch() {
    use crate::import::{Catalog, SourceAvailability};

    let mut config = make_test_config(PathBuf::from("unused"));
    assert_eq!(
        config
            .metadata_sources()
            .into_iter()
            .map(|entry| (entry.catalog, entry.state))
            .collect::<Vec<_>>(),
        vec![
            (Catalog::MusicBrainz, SourceAvailability::On),
            (Catalog::Discogs, SourceAvailability::NotConfigured),
        ],
        "a fresh library has no Discogs key, so it cannot ask Discogs"
    );

    config.prefs.discogs = Some(DiscogsValidation::Valid);
    config
        .prefs
        .identification
        .catalogs
        .set(Catalog::Discogs, false);
    assert_eq!(
        config.metadata_sources()[1].state,
        SourceAvailability::Off,
        "with a key, the switch is what answers"
    );

    config.prefs.discogs = Some(DiscogsValidation::Rejected);
    assert_eq!(
        config.metadata_sources()[1].state,
        SourceAvailability::NotConfigured,
        "a rejected key is no key, whatever the switch says"
    );
}

/// The rule that decides both the refusal and the greyed-out switch: switching
/// off the only source still being asked would leave nothing to ask.
#[test]
fn the_only_asked_source_is_the_one_that_cannot_be_switched_off() {
    use crate::import::{is_the_only_asked_source, Catalog};

    let mut config = make_test_config(PathBuf::from("unused"));
    let sources = config.metadata_sources();
    assert!(is_the_only_asked_source(&sources, Catalog::MusicBrainz));
    assert!(
        !is_the_only_asked_source(&sources, Catalog::Discogs),
        "a source nothing is asking is not the last one asked"
    );

    config.prefs.discogs = Some(DiscogsValidation::Valid);
    let sources = config.metadata_sources();
    for catalog in Catalog::LOOKUP {
        assert!(
            !is_the_only_asked_source(&sources, catalog),
            "with both asked, neither is the last"
        );
    }
}

/// `is_usable` is the single source of truth for whether Discogs can be a
/// metadata source: a stored key is usable optimistically unless rejected.
#[test]
fn discogs_token_status_usability() {
    assert!(DiscogsTokenStatus::Valid.is_usable());
    assert!(DiscogsTokenStatus::Unvalidated.is_usable());
    assert!(!DiscogsTokenStatus::Rejected.is_usable());
    assert!(!DiscogsTokenStatus::NotConfigured.is_usable());
}

/// `discogs_token_status` derives `NotConfigured` from `None` and maps the
/// inner validation otherwise — no key means not configured, with no
/// sentinel validation standing in.
#[test]
fn discogs_token_status_derives_from_option() {
    let tmp = TempDir::new().unwrap();
    let mut config = make_test_config(tmp.path().to_path_buf());

    assert!(config.prefs.discogs.is_none());
    assert!(matches!(
        config.discogs_token_status(),
        DiscogsTokenStatus::NotConfigured
    ));

    config.prefs.discogs = Some(DiscogsValidation::Unvalidated);
    assert!(matches!(
        config.discogs_token_status(),
        DiscogsTokenStatus::Unvalidated
    ));

    config.prefs.discogs = Some(DiscogsValidation::Rejected);
    assert!(matches!(
        config.discogs_token_status(),
        DiscogsTokenStatus::Rejected
    ));
}

/// Every countdown choice a person can pick is written to `preferences.toml`
/// and read back unchanged.
#[tokio::test]
async fn side_pause_countdown_round_trips_through_preferences_toml() {
    for countdown in [
        SidePauseCountdown::Off,
        SidePauseCountdown::Seconds5,
        SidePauseCountdown::Seconds15,
        SidePauseCountdown::Seconds30,
        SidePauseCountdown::Seconds45,
        SidePauseCountdown::Seconds60,
    ] {
        let tmp = TempDir::new().unwrap();
        let config = make_test_config(tmp.path().to_path_buf());
        let handle = Arc::new(ConfigHandle::new(config));

        handle
            .update_preferences(move |prefs| prefs.side_pause_countdown = countdown)
            .await
            .unwrap();

        assert_eq!(
            read_preferences(tmp.path()).unwrap().side_pause_countdown,
            countdown
        );
    }
}

#[test]
fn side_pause_countdown_duration_is_the_offered_length() {
    assert_eq!(SidePauseCountdown::Off.duration(), None);
    assert_eq!(
        SidePauseCountdown::Seconds5.duration(),
        Some(Duration::from_secs(5))
    );
    assert_eq!(
        SidePauseCountdown::Seconds15.duration(),
        Some(Duration::from_secs(15))
    );
    assert_eq!(
        SidePauseCountdown::Seconds30.duration(),
        Some(Duration::from_secs(30))
    );
    assert_eq!(
        SidePauseCountdown::Seconds45.duration(),
        Some(Duration::from_secs(45))
    );
    assert_eq!(
        SidePauseCountdown::Seconds60.duration(),
        Some(Duration::from_secs(60))
    );
}

#[path = "config/preferences_tests.rs"]
mod preferences;

#[test]
fn preferences_load_by_store_id_without_duplicating_store_settings() {
    let tmp = TempDir::new().unwrap();
    let app_dir = AppDir::at(tmp.path());
    let id = coven::StoreId(uuid::Uuid::new_v4());
    let mut config = Config::with_defaults(id, app_dir.registered_library(&id.to_string()));
    config.prefs.cast_enabled = true;
    config.save_preferences().unwrap();
    let loaded = Config::load_registered_library(&app_dir, &id.to_string()).unwrap();
    assert_eq!(loaded.store_id, id);
    assert_eq!(loaded.prefs, config.prefs);
    assert!(!config.library_path().join("config.yaml").exists());
}

#[test]
fn load_registered_library_rejects_a_path_or_non_uuid() {
    let tmp = TempDir::new().unwrap();
    let app_dir = AppDir::at(tmp.path());
    for id in ["../outside", "invalid"] {
        assert!(matches!(
            Config::load_registered_library(&app_dir, id),
            Err(ConfigError::Config(_))
        ));
    }
}

#[test]
fn missing_preferences_are_defaults_and_invalid_preferences_are_errors() {
    let tmp = TempDir::new().unwrap();
    let app_dir = AppDir::at(tmp.path());
    let id = coven::StoreId(uuid::Uuid::new_v4()).to_string();
    assert_eq!(
        Config::load_registered_library(&app_dir, &id)
            .unwrap()
            .prefs,
        Preferences::default()
    );
    let directory = app_dir.registered_library(&id);
    std::fs::create_dir_all(&directory).unwrap();
    std::fs::write(directory.join(PREFERENCES_FILENAME), "invalid = [").unwrap();
    assert!(Config::load_registered_library(&app_dir, &id).is_err());
}

#[tokio::test]
async fn discovery_uses_coven_names_and_sorts_the_active_store_first() {
    let tmp = TempDir::new().unwrap();
    let app_dir = AppDir::at(tmp.path());
    let installation = coven::TestCoven::new();
    let first = crate::library::configure_store(installation.builder(app_dir.store_layout()))
        .create_store("First")
        .await
        .unwrap();
    let second = crate::library::configure_store(installation.builder(app_dir.store_layout()))
        .create_store("Second")
        .await
        .unwrap();
    let first_id = first.store_id();
    let second_id = second.store_id();
    Config::with_defaults(
        second_id,
        app_dir.registered_library(&second_id.to_string()),
    )
    .save_active_library(&app_dir)
    .unwrap();
    first.close().await.unwrap();
    second.close().await.unwrap();
    let libraries = Config::discover_libraries(&app_dir).await.unwrap();
    assert_eq!(
        libraries
            .iter()
            .map(|l| (l.id.as_str(), l.name.as_str(), l.is_active))
            .collect::<Vec<_>>(),
        vec![
            (second_id.to_string().as_str(), "Second", true),
            (first_id.to_string().as_str(), "First", false)
        ]
    );
    assert!(libraries.iter().all(|l| l.cloud_provider.is_none()));
    assert_eq!(
        libraries[0].path,
        app_dir.registered_library(&second_id.to_string())
    );
}

#[tokio::test]
async fn discovery_reports_a_broken_store_instead_of_omitting_it() {
    let tmp = TempDir::new().unwrap();
    let app_dir = AppDir::at(tmp.path());
    let id = coven::StoreId(uuid::Uuid::new_v4());
    app_dir
        .store_layout()
        .create_store_dir(id, &coven::UuidIds)
        .unwrap();
    let error = Config::discover_libraries(&app_dir).await.unwrap_err();
    assert!(error.to_string().contains(&id.to_string()), "{error}");
}

#[tokio::test]
async fn discovery_reports_invalid_or_unreadable_active_pointers() {
    let tmp = TempDir::new().unwrap();
    let app_dir = AppDir::at(tmp.path());
    std::fs::write(app_dir.active_library_pointer(), " \n").unwrap();
    assert!(Config::discover_libraries(&app_dir).await.is_err());
    std::fs::remove_file(app_dir.active_library_pointer()).unwrap();
    std::fs::create_dir(app_dir.active_library_pointer()).unwrap();
    assert!(Config::discover_libraries(&app_dir).await.is_err());
}

#[tokio::test]
async fn preference_updates_publish_after_persistence() {
    let tmp = TempDir::new().unwrap();
    let handle = Arc::new(ConfigHandle::new(make_test_config(tmp.path().into())));
    let mut changes = handle.subscribe();
    handle
        .update_preferences(|prefs| prefs.discogs = Some(DiscogsValidation::Valid))
        .await
        .unwrap();
    changes.changed().await.unwrap();
    assert_eq!(
        changes.borrow().prefs.discogs,
        Some(DiscogsValidation::Valid)
    );
    assert_eq!(read_preferences(tmp.path()).unwrap(), handle.config().prefs);
}

#[test]
fn concurrent_preference_edits_preserve_both_changes() {
    let tmp = TempDir::new().unwrap();
    let handle = Arc::new(ConfigHandle::new(make_test_config(tmp.path().into())));
    let start = Arc::new(Barrier::new(3));
    let workers: Vec<_> = [true, false]
        .into_iter()
        .map(|cast| {
            let handle = handle.clone();
            let start = start.clone();
            std::thread::spawn(move || {
                start.wait();
                handle
                    .update_preferences_now(|prefs| {
                        if cast {
                            prefs.cast_enabled = true;
                        } else {
                            prefs.pause_between_sides = false;
                        }
                    })
                    .unwrap();
            })
        })
        .collect();
    start.wait();
    for worker in workers {
        worker.join().unwrap();
    }
    let prefs = read_preferences(tmp.path()).unwrap();
    assert!(prefs.cast_enabled);
    assert!(!prefs.pause_between_sides);
    assert_eq!(prefs, handle.config().prefs);
}

#[test]
fn installation_id_is_shared_by_libraries_and_survives_reopening() {
    let tmp = TempDir::new().unwrap();
    let app_dir = AppDir::at(tmp.path());
    let ids = coven::SequentialIds::new();
    let first = app_dir.install_id(&ids).unwrap();
    assert_eq!(app_dir.install_id(&ids).unwrap(), first);
    assert_eq!(AppDir::at(tmp.path()).install_id(&ids).unwrap(), first);
    std::fs::write(tmp.path().join("install-id"), "invalid").unwrap();
    assert!(app_dir.install_id(&ids).is_err());
}
