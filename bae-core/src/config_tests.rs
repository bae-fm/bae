use super::*;
use std::num::NonZeroU64;
use std::sync::{Arc, Barrier};
use std::time::Duration;
use tempfile::TempDir;

fn make_test_config(library_id: &str, library_path: PathBuf) -> Config {
    Config::with_defaults(
        library_id.to_string(),
        "test-device-id".to_string(),
        library_path,
        "Test Library".to_string(),
    )
}

/// Default preferences as a `serde_yaml::Value` mapping, for tests that edit
/// one key of the file.
fn default_preferences_value() -> serde_yaml::Value {
    serde_yaml::to_value(Preferences::default()).unwrap()
}

/// Write `value` as a library directory's `preferences.yaml` and read it back
/// the way opening the library does.
fn read_preferences_value(value: &serde_yaml::Value) -> Result<Preferences, ConfigError> {
    let tmp = TempDir::new().unwrap();
    std::fs::write(
        tmp.path().join(PREFERENCES_FILENAME),
        serde_yaml::to_string(value).unwrap(),
    )
    .unwrap();
    read_preferences(tmp.path())
}

/// Read default preferences with one top-level key removed.
fn read_preferences_without(key: &str) -> Result<Preferences, ConfigError> {
    let mut value = default_preferences_value();
    value
        .as_mapping_mut()
        .unwrap()
        .remove(serde_yaml::Value::String(key.to_string()))
        .unwrap_or_else(|| panic!("{key} not in serialized preferences"));
    read_preferences_value(&value)
}

#[test]
fn export_settings_survive_yaml_roundtrip() {
    let tmp = TempDir::new().unwrap();
    let mut config = make_test_config("lib", tmp.path().to_path_buf());
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
    let config = make_test_config("lib", tmp.path().to_path_buf());
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
    let mut config = make_test_config("lib", tmp.path().to_path_buf());
    config.prefs.max_concurrent_uploads = NonZeroU32::new(5).unwrap();
    config.prefs.max_concurrent_downloads = NonZeroU32::new(2).unwrap();
    assert_eq!(usize_bound(config.prefs.max_concurrent_uploads).get(), 5);
    assert_eq!(usize_bound(config.prefs.max_concurrent_downloads).get(), 2);
}

#[test]
fn transfer_concurrency_survives_yaml_roundtrip() {
    let tmp = TempDir::new().unwrap();
    let mut config = make_test_config("lib", tmp.path().to_path_buf());
    config.prefs.max_concurrent_uploads = NonZeroU32::new(7).unwrap();
    config.prefs.max_concurrent_downloads = NonZeroU32::new(4).unwrap();
    config.save_preferences().unwrap();

    let prefs = read_preferences(tmp.path()).unwrap();
    assert_eq!(prefs.max_concurrent_uploads.get(), 7);
    assert_eq!(prefs.max_concurrent_downloads.get(), 4);
}

#[test]
fn a_new_library_pre_fills_with_tags_and_identifies_automatically() {
    let tmp = TempDir::new().unwrap();
    let config = make_test_config("lib", tmp.path().to_path_buf());

    assert!(config.prefs.identification.automatic);
    assert!(config.prefs.prefill_with_file_metadata);
}

/// Every identification step is taken until the person says otherwise: the
/// defaults are what identification did before any of it was a setting.
#[test]
fn a_new_library_takes_every_identification_step() {
    let prefs = IdentificationPreferences::default();
    for step in IdentificationStep::ALL {
        assert!(prefs.steps.takes(step), "{step:?} starts on");
    }
}

/// The step accessors are total over the steps, and each flag is its own.
#[test]
fn identification_steps_are_total_and_independent() {
    for off in IdentificationStep::ALL {
        let mut steps = IdentificationSteps::default();
        steps.set(off, false);
        for step in IdentificationStep::ALL {
            assert_eq!(steps.takes(step), step != off, "{off:?} off, {step:?}");
        }
    }
}

/// An import goes to the cloud only when there is a home to go to.
#[test]
fn an_import_goes_to_the_cloud_only_with_a_home() {
    let tmp = TempDir::new().unwrap();
    let mut config = make_test_config("lib", tmp.path().to_path_buf());
    assert!(
        !config.imports_to_cloud(),
        "no cloud home: local, whatever the choice"
    );
    config.cloud_home.provider = Some(CloudProvider::Dropbox);
    assert!(config.imports_to_cloud());
    config.prefs.import_storage.cloud = false;
    assert!(!config.imports_to_cloud());
}

#[test]
fn prefill_with_file_metadata_and_identify_automatically_roundtrip_independently() {
    for (prefill, identify) in [(false, true), (true, false), (false, false)] {
        let tmp = TempDir::new().unwrap();
        let mut config = make_test_config("lib", tmp.path().to_path_buf());
        config.prefs.identification.automatic = identify;
        config.prefs.prefill_with_file_metadata = prefill;
        config.save_preferences().unwrap();

        let prefs = read_preferences(tmp.path()).unwrap();

        assert_eq!(prefs.identification.automatic, identify);
        assert_eq!(prefs.prefill_with_file_metadata, prefill);
    }
}

/// The typed read is strict about the keys it needs, not about the keys it
/// finds: a file carrying something extra — a key edited in by hand — still
/// loads.
#[test]
fn preferences_carrying_an_unrecognized_key_load() {
    let mut value = default_preferences_value();
    value.as_mapping_mut().unwrap().insert(
        serde_yaml::Value::String("unrecognized_setting".to_string()),
        serde_yaml::Value::String("value".to_string()),
    );

    let loaded = read_preferences_value(&value).expect("an unknown key is ignored");

    assert!(loaded.prefill_with_file_metadata);
    assert!(loaded.identification.automatic);
}

/// A hand-edited `0` is refused at load rather than reaching coven — the
/// `NonZeroU32` field makes the deadlocking value unrepresentable.
#[test]
fn a_zero_concurrency_fails_to_load() {
    let mut value = default_preferences_value();
    value.as_mapping_mut().unwrap().insert(
        serde_yaml::Value::String("max_concurrent_uploads".to_string()),
        serde_yaml::Value::Number(0.into()),
    );
    assert!(
        read_preferences_value(&value).is_err(),
        "a zero concurrency must not load"
    );
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

/// Every key the file writes is serialized unconditionally, so a missing key
/// fails rather than taking an implicit default.
#[test]
fn preferences_require_every_field() {
    for key in [
        "discogs",
        "replay_gain_mode",
        "save_presets",
        "default_track_save_preset",
        "default_release_save_preset",
        "pause_between_sides",
        "side_pause_countdown",
        "max_concurrent_uploads",
        "max_concurrent_downloads",
        "show_remaining_time",
        "library_full_width",
        "verify_decode_on_import",
        "import_storage",
        "identification",
        "prefill_with_file_metadata",
        "cast_enabled",
        "mcp",
        "subsonic",
    ] {
        assert!(
            read_preferences_without(key).is_err(),
            "preferences should fail without {key}"
        );
    }
}

/// The file is the contract. This pins the whole `preferences.yaml` a
/// library writes at its defaults: every key, its order, its nesting, and its
/// value. A rename, a dropped key, or a value that stopped being emitted shows
/// up here.
#[test]
fn preferences_yaml_pins_the_on_disk_file() {
    let tmp = TempDir::new().unwrap();
    let config = make_test_config("abc-123", tmp.path().to_path_buf());
    config.save_preferences().unwrap();
    let written = std::fs::read_to_string(tmp.path().join(PREFERENCES_FILENAME)).unwrap();

    assert_eq!(
        written,
        r#"discogs: null
replay_gain_mode: Off
save_presets:
- id: flac
  name: FLAC
  codec: !Flac
    bit_depth: Source
  filename_tokens:
  - TrackNumber
  - Title
  pregap_placement: AppendToPreviousExceptHtoa
  applies_to_track: true
  applies_to_release: true
  embed_cover: true
- id: mp3
  name: MP3
  codec: !Mp3
    bitrate_kbps: 320
  filename_tokens:
  - TrackNumber
  - Title
  pregap_placement: AppendToPreviousExceptHtoa
  applies_to_track: true
  applies_to_release: true
  embed_cover: true
default_track_save_preset: flac
default_release_save_preset: flac
pause_between_sides: true
side_pause_countdown: Off
max_concurrent_uploads: 3
max_concurrent_downloads: 3
show_remaining_time: false
library_full_width: false
verify_decode_on_import: true
import_storage:
  cloud: true
  pinned: true
identification:
  automatic: true
  steps:
    read_cover_art: true
    look_up_disc_ids: true
    look_up_barcodes: true
    search_by_title: true
    follow_catalog_links: true
  catalogs:
    musicbrainz: true
    discogs: true
prefill_with_file_metadata: true
cast_enabled: false
mcp:
  enabled: false
  port: 47777
subsonic:
  enabled: false
  port: 4533
  username: ''
  bind_address: 127.0.0.1
"#
    );
    // And it reads back, save presets' codec tags included.
    let read_back = read_preferences(tmp.path()).unwrap();
    assert_eq!(read_back.save_presets, config.prefs.save_presets);
}

/// Casting reaches the local network, so it is opt-in: a fresh library has
/// it off, and the choice survives a write/read of preferences.yaml.
#[test]
fn cast_is_off_by_default_and_survives_yaml_roundtrip() {
    let tmp = TempDir::new().unwrap();
    let mut config = make_test_config("lib-cast", tmp.path().to_path_buf());
    assert!(!config.prefs.cast_enabled, "casting is opt-in");

    config.prefs.cast_enabled = true;
    config.save_preferences().unwrap();

    assert!(read_preferences(tmp.path()).unwrap().cast_enabled);
}

/// A config that is genuinely unreadable is SHOWN as broken, not skipped. The
/// user must be able to see that the library is there and in trouble.
#[test]
fn a_broken_library_is_listed_as_broken() {
    let tmp = TempDir::new().unwrap();
    let app_dir = AppDir::at(tmp.path());
    let library_dir = app_dir.libraries().join("lib-broken");
    std::fs::create_dir_all(&library_dir).unwrap();
    std::fs::write(library_dir.join("config.yaml"), "{ this is not: [valid").unwrap();

    let libraries = Config::discover_libraries(&app_dir).unwrap();

    assert_eq!(libraries.len(), 1, "a broken library must not disappear");
    let broken = &libraries[0];
    assert!(broken.error.is_some(), "it must be marked broken");
    // Its name is unreadable — that is the failure — so the directory stands in.
    assert_eq!(broken.id, "lib-broken");
    assert_eq!(broken.name, "lib-broken");
}

/// A working library and a broken one coexist: the broken one sorts last but
/// is still there.
#[test]
fn a_broken_library_does_not_hide_a_working_one() {
    let tmp = TempDir::new().unwrap();
    let app_dir = AppDir::at(tmp.path());
    let libraries_dir = app_dir.libraries();

    let good_dir = libraries_dir.join("lib-good");
    std::fs::create_dir_all(&good_dir).unwrap();
    let mut good = make_test_config("lib-good", good_dir.clone());
    good.store_name = "Good".to_string();
    good.save_store_config().unwrap();

    let broken_dir = libraries_dir.join("lib-broken");
    std::fs::create_dir_all(&broken_dir).unwrap();
    std::fs::write(broken_dir.join("config.yaml"), "{ nope: [").unwrap();

    let libraries = Config::discover_libraries(&app_dir).unwrap();

    assert_eq!(libraries.len(), 2);
    assert_eq!(libraries[0].name, "Good");
    assert!(libraries[0].error.is_none());
    assert_eq!(libraries[1].id, "lib-broken");
    assert!(libraries[1].error.is_some());
}

/// The YAML mapping names its fields, but nothing reads them by name: the
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

    let mut config = make_test_config("abc-123", PathBuf::from("unused"));
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

    let mut config = make_test_config("abc-123", PathBuf::from("unused"));
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
    let mut config = make_test_config("lib-discogs", tmp.path().to_path_buf());

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

/// A library bae created and changed a preference in opens with both: coven's
/// config from `config.yaml` and bae's preferences from `preferences.yaml`.
#[test]
fn a_saved_library_loads_back() {
    let tmp = TempDir::new().unwrap();
    let app_dir = AppDir::at(tmp.path());
    let mut config = make_test_config("my-library-id", app_dir.registered_library("my-library-id"));
    config.prefs.cast_enabled = true;
    config.save_store_config().unwrap();
    config.save_preferences().unwrap();

    let loaded = Config::load_registered_library(&app_dir, "my-library-id").unwrap();

    assert_eq!(loaded.to_coven(), config.to_coven());
    assert!(loaded.prefs.cast_enabled);
    assert_eq!(loaded.prefs.mcp, McpConfig::disabled_default());
}

#[test]
fn load_registered_library_rejects_mismatched_config_id() {
    let tmp = TempDir::new().unwrap();
    let app_dir = AppDir::at(tmp.path());
    make_test_config(
        "wrong-lib-id",
        app_dir.registered_library("expected-lib-id"),
    )
    .save_store_config()
    .unwrap();

    let result = Config::load_registered_library(&app_dir, "expected-lib-id");

    assert!(matches!(result, Err(ConfigError::Config(_))));
}

#[test]
fn active_library_id_errors_when_pointer_is_empty() {
    let tmp = TempDir::new().unwrap();
    let app_dir = AppDir::at(tmp.path());
    std::fs::write(app_dir.active_library_pointer(), " \n").unwrap();

    let err = Config::active_library_id(&app_dir).unwrap_err();

    assert!(matches!(err, ConfigError::Config(_)));
    assert!(err.to_string().contains("active-library pointer"));
}

#[test]
fn discover_libraries_returns_active_pointer_read_error() {
    let tmp = TempDir::new().unwrap();
    let app_dir = AppDir::at(tmp.path());
    let library_path = app_dir.registered_library("auto-lib");

    make_test_config("auto-lib", library_path)
        .save_store_config()
        .unwrap();
    std::fs::create_dir(app_dir.active_library_pointer()).unwrap();

    assert!(Config::discover_libraries(&app_dir).is_err());
}

/// A library dir whose name isn't valid UTF-8 can't round-trip through the
/// `String` paths the app addresses files by, so discovery skips it (rather
/// than panicking or lossily mangling the path) and still finds the valid
/// siblings.
///
/// Unix-only, and even there only on a filesystem that accepts non-UTF-8
/// names: APFS/HFS+ reject the raw byte at the syscall (EILSEQ), so the
/// directory can't exist and the skip branch is unreachable — in that case
/// the test has nothing to exercise and returns after confirming the
/// filesystem refused the name.
#[cfg(unix)]
#[test]
fn discovery_skips_non_utf8_library_dir() {
    use std::os::unix::ffi::OsStrExt;

    let tmp = TempDir::new().unwrap();
    let app_dir = AppDir::at(tmp.path());
    let libraries_dir = app_dir.libraries();
    std::fs::create_dir_all(&libraries_dir).unwrap();

    // A valid library: UTF-8 dir name + config.yaml.
    let library_path = libraries_dir.join("valid-lib");
    make_test_config("valid-lib", library_path.clone())
        .save_store_config()
        .unwrap();

    // A sibling dir whose name is not valid UTF-8 (a lone 0xFF byte). On a
    // filesystem that rejects such names there's nothing to skip — the
    // discovery is then trivially correct and the rest of the test moot.
    let bad_name = std::ffi::OsStr::from_bytes(b"bad-\xff-name");
    if std::fs::create_dir(libraries_dir.join(bad_name)).is_err() {
        return;
    }

    let discovered = discover_all_library_paths(&app_dir);
    assert_eq!(discovered.len(), 1, "non-UTF-8 dir should be skipped");
    assert_eq!(discovered[0].1.as_ref().unwrap().store_id, "valid-lib");
}

#[test]
fn library_name_roundtrip() {
    let tmp = TempDir::new().unwrap();
    let library_path = tmp.path().to_path_buf();
    let mut config = make_test_config("lib-1", library_path.clone());
    config.store_name = "My Music".to_string();
    config.save_store_config().unwrap();

    let store = coven::Config::load_from_config_yaml(&StoreDir::new(&library_path)).unwrap();
    assert_eq!(store.store_name, "My Music");
}

#[test]
fn discover_libraries_finds_dirs_with_config() {
    let tmp = TempDir::new().unwrap();
    let app_dir = AppDir::at(tmp.path());
    let libraries_dir = app_dir.libraries();

    // Create two libraries
    let lib1_path = libraries_dir.join("lib-1");
    make_test_config("lib-1", lib1_path.clone())
        .save_store_config()
        .unwrap();

    let lib2_path = libraries_dir.join("lib-2");
    let mut lib2 = make_test_config("lib-2", lib2_path.clone());
    lib2.store_name = "Second Library".to_string();
    lib2.save_store_config().unwrap();

    // Create an invalid dir (no config.yaml)
    std::fs::create_dir_all(libraries_dir.join("invalid")).unwrap();

    let discovered = discover_all_library_paths(&app_dir);
    assert_eq!(discovered.len(), 2);

    let ids: Vec<&str> = discovered
        .iter()
        .map(|(_, y)| y.as_ref().unwrap().store_id.as_str())
        .collect();
    assert!(ids.contains(&"lib-1"));
    assert!(ids.contains(&"lib-2"));

    let lib2_entry = discovered
        .iter()
        .find(|(_, y)| y.as_ref().unwrap().store_id == "lib-2")
        .unwrap();
    assert_eq!(lib2_entry.1.as_ref().unwrap().store_name, "Second Library");
}

#[test]
fn find_library_by_id_scans_libraries_dir() {
    let tmp = TempDir::new().unwrap();
    let app_dir = AppDir::at(tmp.path());
    let libraries_dir = app_dir.libraries();

    let lib1_path = libraries_dir.join("lib-1");
    make_test_config("lib-1", lib1_path.clone())
        .save_store_config()
        .unwrap();

    let lib2_path = libraries_dir.join("lib-2");
    make_test_config("lib-2", lib2_path.clone())
        .save_store_config()
        .unwrap();

    let found = find_library_by_id(&app_dir, "lib-1");
    assert!(found.is_some());
    assert_eq!(&*found.unwrap(), lib1_path.as_path());

    let found = find_library_by_id(&app_dir, "lib-2");
    assert!(found.is_some());
    assert_eq!(&*found.unwrap(), lib2_path.as_path());

    assert!(find_library_by_id(&app_dir, "nonexistent").is_none());
}

#[tokio::test]
async fn rename_library_updates_config_yaml() {
    let tmp = TempDir::new().unwrap();
    let library_path = tmp.path().to_path_buf();
    let config = make_test_config("lib-1", library_path.clone());
    config.save_store_config().unwrap();
    let handle = Arc::new(ConfigHandle::new(config));

    handle
        .rename_library(&crate::library_name::LibraryName::parse("New Name").unwrap())
        .await
        .unwrap();
    assert_eq!(handle.config().store_name, "New Name");

    let store = coven::Config::load_from_config_yaml(&StoreDir::new(&library_path)).unwrap();
    assert_eq!(store.store_name, "New Name");
    assert_eq!(store.store_id, "lib-1"); // unchanged
}

/// An `update` is reflected by the `Config` that `config()` returns — the
/// same `Config` the bridge reads to build the UI's Discogs token status. If
/// a write only reached an on-disk copy or a side cache, the bridge would
/// keep reporting "not configured" until the next load.
#[tokio::test]
async fn update_is_reflected_by_config() {
    let tmp = TempDir::new().unwrap();
    let config = make_test_config("lib-update", tmp.path().to_path_buf());
    config.save_store_config().unwrap();
    let handle = Arc::new(ConfigHandle::new(config));

    assert!(handle.config().prefs.discogs.is_none());
    handle
        .update_preferences(|prefs| prefs.discogs = Some(DiscogsValidation::Valid))
        .await
        .unwrap();
    assert_eq!(
        handle.config().prefs.discogs,
        Some(DiscogsValidation::Valid)
    );
}

/// A store edit and a preference edit racing each other both land, in memory
/// and in their own files.
#[test]
fn update_serializes_concurrent_edits() {
    let tmp = TempDir::new().unwrap();
    let library_path = tmp.path().to_path_buf();
    let config = make_test_config("lib-update-race", library_path.clone());
    config.save_store_config().unwrap();
    let handle = Arc::new(ConfigHandle::new(config));
    let start = Arc::new(Barrier::new(3));

    let rename = {
        let handle = Arc::clone(&handle);
        let start = Arc::clone(&start);
        std::thread::spawn(move || {
            start.wait();
            handle
                .update_store_now(|store| {
                    std::thread::sleep(Duration::from_millis(100));
                    store.store_name = "Renamed Library".to_string();
                })
                .unwrap();
        })
    };
    let playback = {
        let handle = Arc::clone(&handle);
        let start = Arc::clone(&start);
        std::thread::spawn(move || {
            start.wait();
            handle
                .update_preferences_now(|prefs| {
                    std::thread::sleep(Duration::from_millis(100));
                    prefs.pause_between_sides = false;
                })
                .unwrap();
        })
    };

    start.wait();
    rename.join().unwrap();
    playback.join().unwrap();

    let final_config = handle.config().clone();
    assert_eq!(final_config.store_name, "Renamed Library");
    assert!(!final_config.prefs.pause_between_sides);

    let store = coven::Config::load_from_config_yaml(&StoreDir::new(&library_path)).unwrap();
    assert_eq!(store.store_name, "Renamed Library");
    assert!(!read_preferences(&library_path).unwrap().pause_between_sides);
}

/// A store coven just restored or joined keeps every field coven gave it, and
/// starts bae's preferences at their defaults.
#[test]
fn from_coven_keeps_coven_config_and_defaults_preferences() {
    let mut coven_config = coven::Config::with_defaults(
        "restored-lib-abc-123".to_string(),
        "restored-device".to_string(),
        "Test Library".to_string(),
    );
    coven_config.cloud_home.provider = Some(CloudProvider::CloudKit);
    coven_config.snapshot_commit_threshold = NonZeroU64::new(7).unwrap();
    coven_config.cloud_home.cloudkit_owner_name = Some("_owner".to_string());
    coven_config.cloud_home.cloudkit_zone_name = Some("bae-library".to_string());

    let config = Config::from_coven(coven_config.clone(), PathBuf::from("unused"));

    assert_eq!(config.to_coven(), coven_config);
    assert_eq!(config.prefs.mcp, McpConfig::disabled_default());
}

/// coven's restore and join write `config.yaml` themselves and treat it as the
/// finished store. A library that has only that file — nothing bae wrote after
/// coven returned — opens, with bae's preferences at their defaults.
#[test]
fn a_library_coven_wrote_opens_with_default_preferences() {
    let tmp = TempDir::new().unwrap();
    let app_dir = AppDir::at(tmp.path());
    let store_dir = app_dir.store_layout().store_dir("restored-lib");
    let mut coven_config = coven::Config::with_defaults(
        "restored-lib".to_string(),
        "restored-device".to_string(),
        "Restored Library".to_string(),
    );
    coven_config.snapshot_commit_threshold = NonZeroU64::new(7).unwrap();
    coven_config.save_to_config_yaml(&store_dir).unwrap();

    let config = Config::load_registered_library(&app_dir, "restored-lib").unwrap();

    assert_eq!(config.to_coven(), coven_config);
    assert_eq!(config.prefs.mcp, McpConfig::disabled_default());
}

/// `config.yaml` stays in coven's format after bae changes a setting, so coven
/// can still read it back (join reads it as its completion marker).
#[tokio::test]
async fn changing_a_preference_leaves_coven_config_readable_by_coven() {
    let tmp = TempDir::new().unwrap();
    let config = make_test_config("lib-owned", tmp.path().to_path_buf());
    config.save_store_config().unwrap();
    let handle = Arc::new(ConfigHandle::new(config.clone()));

    handle
        .update_preferences(|prefs| prefs.pause_between_sides = false)
        .await
        .unwrap();

    let store_dir = coven::StoreDir::new(tmp.path());
    assert_eq!(
        coven::Config::load_from_config_yaml(&store_dir).unwrap(),
        config.to_coven()
    );
}

/// Every countdown choice a person can pick is written to `preferences.yaml`
/// and read back unchanged.
#[tokio::test]
async fn side_pause_countdown_round_trips_through_preferences_yaml() {
    for countdown in [
        SidePauseCountdown::Off,
        SidePauseCountdown::Seconds5,
        SidePauseCountdown::Seconds15,
        SidePauseCountdown::Seconds30,
        SidePauseCountdown::Seconds45,
        SidePauseCountdown::Seconds60,
    ] {
        let tmp = TempDir::new().unwrap();
        let config = make_test_config("lib-countdown", tmp.path().to_path_buf());
        config.save_store_config().unwrap();
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
