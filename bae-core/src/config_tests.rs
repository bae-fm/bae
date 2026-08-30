use super::*;
use serial_test::serial;
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

/// A full `ConfigYaml` serialized to a `serde_yaml::Value` mapping, for
/// tests that assert a single missing key fails the load.
fn full_config_yaml_value() -> serde_yaml::Value {
    let config = make_test_config("abc-123", PathBuf::from("unused"));
    serde_yaml::to_value(ConfigYaml::from(&config)).unwrap()
}

/// Parse a full config with one top-level key removed.
fn parse_yaml_without(key: &str) -> Result<ConfigYaml, serde_yaml::Error> {
    let mut value = full_config_yaml_value();
    let map = value.as_mapping_mut().unwrap();
    map.remove(serde_yaml::Value::String(key.to_string()))
        .unwrap_or_else(|| panic!("{key} not in serialized config"));
    serde_yaml::from_value(value)
}

#[test]
fn export_settings_survive_yaml_roundtrip() {
    let tmp = TempDir::new().unwrap();
    let mut config = make_test_config("lib", tmp.path().to_path_buf());
    // Filename tokens are per-preset now; a preset's edited pattern survives.
    config.save_presets[0].filename_tokens =
        vec![SaveFilenameToken::Artist, SaveFilenameToken::Title];
    config.save_to_config_yaml().unwrap();

    let yaml: ConfigYaml =
        serde_yaml::from_str(&std::fs::read_to_string(tmp.path().join("config.yaml")).unwrap())
            .unwrap();
    assert_eq!(
        yaml.save_presets[0].filename_tokens,
        vec![SaveFilenameToken::Artist, SaveFilenameToken::Title]
    );
    assert_eq!(yaml.save_presets, config.save_presets);
    assert_eq!(
        yaml.default_track_save_preset,
        config.default_track_save_preset
    );
    assert_eq!(
        yaml.default_release_save_preset,
        config.default_release_save_preset
    );
}

#[test]
fn transfer_concurrency_defaults_to_three() {
    let tmp = TempDir::new().unwrap();
    let config = make_test_config("lib", tmp.path().to_path_buf());
    assert_eq!(config.max_concurrent_uploads.get(), 3);
    assert_eq!(config.max_concurrent_downloads.get(), 3);
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
    config.max_concurrent_uploads = NonZeroU32::new(5).unwrap();
    config.max_concurrent_downloads = NonZeroU32::new(2).unwrap();
    assert_eq!(usize_bound(config.max_concurrent_uploads).get(), 5);
    assert_eq!(usize_bound(config.max_concurrent_downloads).get(), 2);
}

#[test]
fn transfer_concurrency_survives_yaml_roundtrip() {
    let tmp = TempDir::new().unwrap();
    let mut config = make_test_config("lib", tmp.path().to_path_buf());
    config.max_concurrent_uploads = NonZeroU32::new(7).unwrap();
    config.max_concurrent_downloads = NonZeroU32::new(4).unwrap();
    config.save_to_config_yaml().unwrap();

    let yaml: ConfigYaml =
        serde_yaml::from_str(&std::fs::read_to_string(tmp.path().join("config.yaml")).unwrap())
            .unwrap();
    assert_eq!(yaml.max_concurrent_uploads.get(), 7);
    assert_eq!(yaml.max_concurrent_downloads.get(), 4);
}

#[test]
fn import_metadata_settings_default_to_automatic_lookup() {
    let tmp = TempDir::new().unwrap();
    let config = make_test_config("lib", tmp.path().to_path_buf());

    assert_eq!(
        config.default_find_online_mode,
        DefaultFindOnlineMode::Automatic
    );
    assert_eq!(
        config.default_import_metadata_source,
        DefaultImportMetadataSource::FindOnline
    );
}

#[test]
fn import_metadata_source_and_find_online_mode_roundtrip_independently() {
    let tmp = TempDir::new().unwrap();
    let mut config = make_test_config("lib", tmp.path().to_path_buf());
    config.default_find_online_mode = DefaultFindOnlineMode::SearchManually;
    config.default_import_metadata_source = DefaultImportMetadataSource::None;
    config.save_to_config_yaml().unwrap();

    let yaml: ConfigYaml =
        serde_yaml::from_str(&std::fs::read_to_string(tmp.path().join("config.yaml")).unwrap())
            .unwrap();
    let loaded = yaml.into_config("device".to_string(), tmp.path().to_path_buf());

    assert_eq!(
        loaded.default_find_online_mode,
        DefaultFindOnlineMode::SearchManually
    );
    assert_eq!(
        loaded.default_import_metadata_source,
        DefaultImportMetadataSource::None
    );
}

#[test]
fn find_online_mode_is_independent_of_the_discovery_default() {
    let tmp = TempDir::new().unwrap();
    let mut config = make_test_config("lib", tmp.path().to_path_buf());
    for source in [
        DefaultImportMetadataSource::FindOnline,
        DefaultImportMetadataSource::FileTags,
        DefaultImportMetadataSource::None,
    ] {
        config.default_import_metadata_source = source;
        assert_eq!(
            config.default_find_online_mode,
            DefaultFindOnlineMode::Automatic
        );
    }
}

/// A hand-edited `0` is refused at load rather than reaching coven — the
/// `NonZeroU32` field makes the deadlocking value unrepresentable.
#[test]
fn a_zero_concurrency_fails_to_load() {
    let mut value = full_config_yaml_value();
    value.as_mapping_mut().unwrap().insert(
        serde_yaml::Value::String("max_concurrent_uploads".to_string()),
        serde_yaml::Value::Number(0.into()),
    );
    let yaml = serde_yaml::to_string(&value).unwrap();
    assert!(
        parse_config_yaml(&yaml).is_err(),
        "a zero concurrency must not load"
    );
}

#[test]
fn config_yaml_requires_library_id() {
    assert!(
        parse_yaml_without("library_id").is_err(),
        "ConfigYaml should fail without library_id"
    );
}

#[test]
fn config_yaml_requires_mcp() {
    assert!(
        parse_yaml_without("mcp").is_err(),
        "ConfigYaml should fail without mcp"
    );
}

#[test]
fn config_yaml_requires_subsonic() {
    assert!(
        parse_yaml_without("subsonic").is_err(),
        "ConfigYaml should fail without subsonic"
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

/// Every bae-local field except `device_id` is serialized unconditionally, so
/// a missing key fails rather than taking an implicit default.
#[test]
fn config_yaml_requires_every_bae_field() {
    for key in [
        "discogs",
        "replay_gain_mode",
        "save_presets",
        "default_track_save_preset",
        "default_release_save_preset",
        "pause_between_sides",
        "max_concurrent_uploads",
        "max_concurrent_downloads",
        "show_remaining_time",
        "library_full_width",
        "verify_decode_on_import",
        "default_find_online_mode",
        "default_import_metadata_source",
        "cast_enabled",
    ] {
        assert!(
            parse_yaml_without(key).is_err(),
            "ConfigYaml should fail without {key}"
        );
    }
}

/// Casting reaches the local network, so it is opt-in: a fresh library has
/// it off, and the choice survives a write/read of config.yaml.
#[test]
fn cast_is_off_by_default_and_survives_yaml_roundtrip() {
    let tmp = TempDir::new().unwrap();
    let mut config = make_test_config("lib-cast", tmp.path().to_path_buf());
    assert!(!config.cast_enabled, "casting is opt-in");

    config.cast_enabled = true;
    config.save_to_config_yaml().unwrap();

    let yaml: ConfigYaml =
        serde_yaml::from_str(&std::fs::read_to_string(tmp.path().join("config.yaml")).unwrap())
            .unwrap();
    assert!(yaml.cast_enabled);
}

/// A config that is genuinely unreadable is SHOWN as broken, not skipped. The
/// user must be able to see that the library is there and in trouble.
#[test]
#[serial]
fn a_broken_library_is_listed_as_broken() {
    let tmp = TempDir::new().unwrap();
    let bae_dir = tmp.path();
    let library_dir = bae_dir.join("libraries").join("lib-broken");
    std::fs::create_dir_all(&library_dir).unwrap();
    std::fs::write(library_dir.join("config.yaml"), "{ this is not: [valid").unwrap();

    let libraries = discover_libraries_from_bae_dir(bae_dir).unwrap();

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
#[serial]
fn a_broken_library_does_not_hide_a_working_one() {
    let tmp = TempDir::new().unwrap();
    let bae_dir = tmp.path();
    let libraries_dir = bae_dir.join("libraries");

    let good_dir = libraries_dir.join("lib-good");
    std::fs::create_dir_all(&good_dir).unwrap();
    let mut good = make_test_config("lib-good", good_dir.clone());
    good.store_name = "Good".to_string();
    good.save_to_config_yaml().unwrap();

    let broken_dir = libraries_dir.join("lib-broken");
    std::fs::create_dir_all(&broken_dir).unwrap();
    std::fs::write(broken_dir.join("config.yaml"), "{ nope: [").unwrap();

    let libraries = discover_libraries_from_bae_dir(bae_dir).unwrap();

    assert_eq!(libraries.len(), 2);
    assert_eq!(libraries[0].name, "Good");
    assert!(libraries[0].error.is_none());
    assert_eq!(libraries[1].id, "lib-broken");
    assert!(libraries[1].error.is_some());
}

/// `device_id` is the one designed absence: missing on a fresh library, and
/// auto-generated (and written back) on first load rather than failing.
#[test]
fn config_yaml_allows_missing_device_id() {
    let config = parse_yaml_without("device_id").unwrap();
    assert_eq!(config.device_id, None);
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

    assert!(config.discogs.is_none());
    assert!(matches!(
        config.discogs_token_status(),
        DiscogsTokenStatus::NotConfigured
    ));

    config.discogs = Some(DiscogsValidation::Unvalidated);
    assert!(matches!(
        config.discogs_token_status(),
        DiscogsTokenStatus::Unvalidated
    ));

    config.discogs = Some(DiscogsValidation::Rejected);
    assert!(matches!(
        config.discogs_token_status(),
        DiscogsTokenStatus::Rejected
    ));
}

#[test]
fn config_yaml_requires_storage() {
    // `storage` rides the flattened coven CloudHomeConfig and carries no
    // serde default: a config file without it fails to load rather than
    // silently assuming a cipher/path scheme.
    assert!(
        parse_yaml_without("storage").is_err(),
        "ConfigYaml should fail without storage"
    );
}

#[test]
fn save_and_load_config_yaml_roundtrip() {
    let tmp = TempDir::new().unwrap();
    let library_path = tmp.path().to_path_buf();
    let config = make_test_config("my-library-id", library_path.clone());

    config.save_to_config_yaml().unwrap();

    let yaml: ConfigYaml =
        serde_yaml::from_str(&std::fs::read_to_string(library_path.join("config.yaml")).unwrap())
            .unwrap();
    assert_eq!(yaml.library_id, "my-library-id");
    assert_eq!(yaml.mcp, McpConfig::disabled_default());
}

#[test]
fn load_from_registered_library_dir_rejects_mismatched_config_id() {
    let tmp = TempDir::new().unwrap();
    let library_path = tmp.path().join("libraries").join("expected-lib-id");
    make_test_config("wrong-lib-id", library_path.clone())
        .save_to_config_yaml()
        .unwrap();

    let result = Config::load_from_registered_library_dir(
        library_path,
        "expected-lib-id",
        &coven::SequentialIdProvider::new("device"),
    );

    assert!(matches!(result, Err(ConfigError::Config(_))));
}

#[test]
fn read_active_library_id_errors_when_pointer_is_empty() {
    let tmp = TempDir::new().unwrap();
    std::fs::write(tmp.path().join("active-library"), " \n").unwrap();

    let err = read_active_library_id(tmp.path()).unwrap_err();

    assert!(matches!(err, ConfigError::Config(_)));
    assert!(err.to_string().contains("active-library pointer"));
}

#[test]
fn discover_libraries_from_bae_dir_returns_active_pointer_read_error() {
    let tmp = TempDir::new().unwrap();
    let bae_dir = tmp.path();
    let library_path = registered_library_path(bae_dir, "auto-lib");

    make_test_config("auto-lib", library_path)
        .save_to_config_yaml()
        .unwrap();
    std::fs::create_dir(bae_dir.join("active-library")).unwrap();

    assert!(discover_libraries_from_bae_dir(bae_dir).is_err());
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
    let bae_dir = tmp.path();
    let libraries_dir = bae_dir.join("libraries");
    std::fs::create_dir_all(&libraries_dir).unwrap();

    // A valid library: UTF-8 dir name + config.yaml.
    let library_path = libraries_dir.join("valid-lib");
    make_test_config("valid-lib", library_path.clone())
        .save_to_config_yaml()
        .unwrap();

    // A sibling dir whose name is not valid UTF-8 (a lone 0xFF byte). On a
    // filesystem that rejects such names there's nothing to skip — the
    // discovery is then trivially correct and the rest of the test moot.
    let bad_name = std::ffi::OsStr::from_bytes(b"bad-\xff-name");
    if std::fs::create_dir(libraries_dir.join(bad_name)).is_err() {
        return;
    }

    let discovered = discover_all_library_paths(bae_dir);
    assert_eq!(discovered.len(), 1, "non-UTF-8 dir should be skipped");
    assert_eq!(discovered[0].1.as_ref().unwrap().library_id, "valid-lib");
}

#[test]
fn library_name_roundtrip() {
    let tmp = TempDir::new().unwrap();
    let library_path = tmp.path().to_path_buf();
    let mut config = make_test_config("lib-1", library_path.clone());
    config.store_name = "My Music".to_string();
    config.save_to_config_yaml().unwrap();

    let yaml: ConfigYaml =
        serde_yaml::from_str(&std::fs::read_to_string(library_path.join("config.yaml")).unwrap())
            .unwrap();
    assert_eq!(yaml.library_name, "My Music");
}

#[test]
fn discover_libraries_finds_dirs_with_config() {
    let tmp = TempDir::new().unwrap();
    let bae_dir = tmp.path();
    let libraries_dir = bae_dir.join("libraries");

    // Create two libraries
    let lib1_path = libraries_dir.join("lib-1");
    make_test_config("lib-1", lib1_path.clone())
        .save_to_config_yaml()
        .unwrap();

    let lib2_path = libraries_dir.join("lib-2");
    let mut lib2 = make_test_config("lib-2", lib2_path.clone());
    lib2.store_name = "Second Library".to_string();
    lib2.save_to_config_yaml().unwrap();

    // Create an invalid dir (no config.yaml)
    std::fs::create_dir_all(libraries_dir.join("invalid")).unwrap();

    let discovered = discover_all_library_paths(bae_dir);
    assert_eq!(discovered.len(), 2);

    let ids: Vec<&str> = discovered
        .iter()
        .map(|(_, y)| y.as_ref().unwrap().library_id.as_str())
        .collect();
    assert!(ids.contains(&"lib-1"));
    assert!(ids.contains(&"lib-2"));

    let lib2_entry = discovered
        .iter()
        .find(|(_, y)| y.as_ref().unwrap().library_id == "lib-2")
        .unwrap();
    let lib2_yaml = lib2_entry.1.as_ref().unwrap();
    assert_eq!(lib2_yaml.library_name, "Second Library");
}

#[test]
fn find_library_by_id_scans_libraries_dir() {
    let tmp = TempDir::new().unwrap();
    let bae_dir = tmp.path();
    let libraries_dir = bae_dir.join("libraries");

    let lib1_path = libraries_dir.join("lib-1");
    make_test_config("lib-1", lib1_path.clone())
        .save_to_config_yaml()
        .unwrap();

    let lib2_path = libraries_dir.join("lib-2");
    make_test_config("lib-2", lib2_path.clone())
        .save_to_config_yaml()
        .unwrap();

    let found = find_library_by_id(bae_dir, "lib-1");
    assert!(found.is_some());
    assert_eq!(&*found.unwrap(), lib1_path.as_path());

    let found = find_library_by_id(bae_dir, "lib-2");
    assert!(found.is_some());
    assert_eq!(&*found.unwrap(), lib2_path.as_path());

    assert!(find_library_by_id(bae_dir, "nonexistent").is_none());
}

#[test]
fn rename_library_updates_config_yaml() {
    let tmp = TempDir::new().unwrap();
    let library_path = tmp.path().to_path_buf();
    let config = make_test_config("lib-1", library_path.clone());
    config.save_to_config_yaml().unwrap();
    let handle = ConfigHandle::new(config);

    handle
        .rename_library(&crate::library_name::LibraryName::parse("New Name").unwrap())
        .unwrap();
    assert_eq!(handle.config().store_name, "New Name");

    let yaml: ConfigYaml =
        serde_yaml::from_str(&std::fs::read_to_string(library_path.join("config.yaml")).unwrap())
            .unwrap();
    assert_eq!(yaml.library_name, "New Name");
    assert_eq!(yaml.library_id, "lib-1"); // unchanged
}

/// An `update` is reflected by the `Config` that `config()` returns — the
/// same `Config` the bridge reads to build the UI's Discogs token status. If
/// a write only reached an on-disk copy or a side cache, the bridge would
/// keep reporting "not configured" until the next load.
#[test]
fn update_is_reflected_by_config() {
    let tmp = TempDir::new().unwrap();
    let config = make_test_config("lib-update", tmp.path().to_path_buf());
    config.save_to_config_yaml().unwrap();
    let handle = ConfigHandle::new(config);

    assert!(handle.config().discogs.is_none());
    handle
        .update(|c| c.discogs = Some(DiscogsValidation::Valid))
        .unwrap();
    assert_eq!(handle.config().discogs, Some(DiscogsValidation::Valid));
}

#[test]
fn update_serializes_concurrent_edits() {
    let tmp = TempDir::new().unwrap();
    let library_path = tmp.path().to_path_buf();
    let config = make_test_config("lib-update-race", library_path.clone());
    config.save_to_config_yaml().unwrap();
    let handle = Arc::new(ConfigHandle::new(config));
    let start = Arc::new(Barrier::new(3));

    fn spawn_update(
        handle: Arc<ConfigHandle>,
        start: Arc<Barrier>,
        edit: impl FnOnce(&mut Config) + Send + 'static,
    ) -> std::thread::JoinHandle<()> {
        std::thread::spawn(move || {
            start.wait();
            handle
                .update(|config| {
                    std::thread::sleep(Duration::from_millis(100));
                    edit(config);
                })
                .unwrap();
        })
    }

    let rename = spawn_update(Arc::clone(&handle), Arc::clone(&start), |config| {
        config.store_name = "Renamed Library".to_string();
    });
    let playback = spawn_update(Arc::clone(&handle), Arc::clone(&start), |config| {
        config.pause_between_sides = true;
    });

    start.wait();
    rename.join().unwrap();
    playback.join().unwrap();

    let final_config = handle.config().clone();
    assert_eq!(final_config.store_name, "Renamed Library");
    assert!(final_config.pause_between_sides);

    let yaml: ConfigYaml =
        serde_yaml::from_str(&std::fs::read_to_string(library_path.join("config.yaml")).unwrap())
            .unwrap();
    assert_eq!(yaml.library_name, "Renamed Library");
    assert!(yaml.pause_between_sides);
}

#[test]
fn from_coven_preserves_library_id_and_persists_bae_yaml() {
    let tmp = TempDir::new().unwrap();
    let library_path = tmp.path().join("libraries").join("restored-lib-abc-123");
    let library_id = "restored-lib-abc-123";

    let mut coven_config = coven::Config::with_defaults(
        library_id.to_string(),
        "restored-device".to_string(),
        "Test Library".to_string(),
    );
    coven_config.cloud_home.provider = Some(CloudProvider::CloudKit);
    coven_config.cloud_home.cloudkit_owner_name = Some("_owner".to_string());
    coven_config.cloud_home.cloudkit_zone_name = Some("bae-library".to_string());
    let config = Config::from_coven(coven_config, library_path.clone());

    assert_eq!(config.store_id, library_id);
    assert_eq!(config.store_name, "Test Library");
    assert_eq!(config.mcp, McpConfig::disabled_default());
    assert_eq!(
        config.cloud_home.cloudkit_owner_name.as_deref(),
        Some("_owner")
    );
    assert_eq!(
        config.cloud_home.cloudkit_zone_name.as_deref(),
        Some("bae-library")
    );

    config.save_to_config_yaml().unwrap();

    let yaml: ConfigYaml =
        serde_yaml::from_str(&std::fs::read_to_string(library_path.join("config.yaml")).unwrap())
            .unwrap();
    assert_eq!(yaml.library_id, library_id);
    assert_eq!(yaml.mcp, McpConfig::disabled_default());
    assert_eq!(
        yaml.cloud_home.cloudkit_owner_name.as_deref(),
        Some("_owner")
    );
    assert_eq!(
        yaml.cloud_home.cloudkit_zone_name.as_deref(),
        Some("bae-library")
    );
}
