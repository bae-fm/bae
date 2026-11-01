use super::*;
use std::time::Duration;
use tempfile::TempDir;

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

/// The wire this feature exists for: the stored values are what the open
/// store is handed. `transfer_limits` is the exact conversion opening the
/// library applies; if it dropped or clamped a value, the knob would move
/// nothing.
#[test]
fn stored_concurrency_widens_to_the_store_limits() {
    let prefs = Preferences {
        max_concurrent_uploads: NonZeroU32::new(5).unwrap(),
        max_concurrent_downloads: NonZeroU32::new(2).unwrap(),
        ..Preferences::default()
    };
    let limits = prefs.transfer_limits();
    assert_eq!(limits.uploads.get(), 5);
    assert_eq!(limits.downloads.get(), 2);
}

/// Nothing is imported on its own until the person says so.
#[test]
fn a_new_library_imports_nothing_on_its_own() {
    let prefs = IdentificationPreferences::default();
    assert!(!prefs.import_when_identified);
}

/// An import goes where the stored choice says, pinned as it says, and to the
/// cloud only when there is a home to go to.
#[test]
fn an_import_s_destination_is_the_stored_choice() {
    use crate::import::ImportDestination;
    let mut prefs = Preferences::default();
    assert_eq!(
        prefs.import_destination(false),
        ImportDestination::Local,
        "without storage, imports stay local"
    );
    assert_eq!(
        prefs.import_destination(true),
        ImportDestination::Remote { pin: true }
    );
    prefs.import_storage.pinned = false;
    assert_eq!(
        prefs.import_destination(true),
        ImportDestination::Remote { pin: false }
    );
    prefs.import_storage.cloud = false;
    assert_eq!(prefs.import_destination(true), ImportDestination::Local);
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

/// The row names a column per catalog, but nothing else reads them by name:
/// the accessors are total over the catalogs bae asks, so each has an answer
/// and a new one cannot be silently forgotten.
#[test]
fn lookup_catalog_preferences_are_total_over_the_asked_catalogs() {
    let mut prefs = LookupCatalogPreferences::default();
    for catalog in bae_catalogs::catalog::Catalog::LOOKUP {
        assert!(prefs.enabled(catalog), "every asked catalog starts asked");
    }

    prefs.set(bae_catalogs::catalog::Catalog::Discogs, false);
    assert!(prefs.enabled(bae_catalogs::catalog::Catalog::MusicBrainz));
    assert!(!prefs.enabled(bae_catalogs::catalog::Catalog::Discogs));
}

/// The availability list folds two questions that are not the same — whether
/// this library holds what a source needs, and whether the person wants it
/// asked — into the one answer everything that asks the sources together
/// reads. Unreachable beats switched-off, so a source with no credential says
/// which of the two to fix while the person's switch waits underneath it.
#[test]
fn metadata_sources_fold_the_credential_and_the_switch() {
    use crate::import::SourceAvailability;
    use bae_catalogs::catalog::Catalog;

    let mut prefs = Preferences::default();
    assert_eq!(
        prefs
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

    prefs.discogs = Some(DiscogsValidation::Valid);
    prefs.identification.catalogs.set(Catalog::Discogs, false);
    assert_eq!(
        prefs.metadata_sources()[1].state,
        SourceAvailability::Off,
        "with a key, the switch is what answers"
    );

    prefs.discogs = Some(DiscogsValidation::Rejected);
    assert_eq!(
        prefs.metadata_sources()[1].state,
        SourceAvailability::NotConfigured,
        "a rejected key is no key, whatever the switch says"
    );
}

/// The rule that decides both the refusal and the greyed-out switch: switching
/// off the only source still being asked would leave nothing to ask.
#[test]
fn the_only_asked_source_is_the_one_that_cannot_be_switched_off() {
    use crate::import::is_the_only_asked_source;
    use bae_catalogs::catalog::Catalog;

    let mut prefs = Preferences::default();
    let sources = prefs.metadata_sources();
    assert!(is_the_only_asked_source(&sources, Catalog::MusicBrainz));
    assert!(
        !is_the_only_asked_source(&sources, Catalog::Discogs),
        "a source nothing is asking is not the last one asked"
    );

    prefs.discogs = Some(DiscogsValidation::Valid);
    let sources = prefs.metadata_sources();
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
    let mut prefs = Preferences::default();

    assert!(prefs.discogs.is_none());
    assert!(matches!(
        prefs.discogs_token_status(),
        DiscogsTokenStatus::NotConfigured
    ));

    prefs.discogs = Some(DiscogsValidation::Unvalidated);
    assert!(matches!(
        prefs.discogs_token_status(),
        DiscogsTokenStatus::Unvalidated
    ));

    prefs.discogs = Some(DiscogsValidation::Rejected);
    assert!(matches!(
        prefs.discogs_token_status(),
        DiscogsTokenStatus::Rejected
    ));
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

#[tokio::test]
async fn discovery_uses_coven_names_and_sorts_the_active_store_first() {
    let tmp = TempDir::new().unwrap();
    let app_dir = AppDir::at(tmp.path()).unwrap();
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
    app_dir.save_active_library(second_id).unwrap();
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
    assert!(libraries
        .iter()
        .all(|l| matches!(l.state, LibraryState::Openable(None))));
    assert_eq!(
        libraries[0].path,
        app_dir.registered_library(&second_id.to_string())
    );
}

#[tokio::test]
async fn discovery_reports_a_broken_store_instead_of_omitting_it() {
    let tmp = TempDir::new().unwrap();
    let app_dir = AppDir::at(tmp.path()).unwrap();
    let id = coven::StoreId(uuid::Uuid::new_v4());
    app_dir
        .store_layout()
        .create_store_dir(id, &coven::UuidIds)
        .unwrap();
    let error = Config::discover_libraries(&app_dir).await.unwrap_err();
    assert!(error.to_string().contains(&id.to_string()), "{error}");
}

/// A library whose storage settings can't be read is listed with why, and
/// the others still list as they are.
#[tokio::test]
async fn discovery_lists_a_library_whose_settings_cant_be_read() {
    let tmp = TempDir::new().unwrap();
    let app_dir = AppDir::at(tmp.path()).unwrap();
    let installation = coven::TestCoven::new();
    let mut ids = Vec::new();
    for name in ["Openable", "Unreadable"] {
        let handle = crate::library::configure_store(installation.builder(app_dir.store_layout()))
            .create_store(name)
            .await
            .unwrap();
        ids.push(handle.store_id());
        handle.close().await.unwrap();
    }
    std::fs::write(
        tmp.path()
            .join("stores")
            .join(ids[1].to_string())
            .join("storage.json"),
        "{not settings",
    )
    .unwrap();

    let libraries = Config::discover_libraries(&app_dir).await.unwrap();
    let state = |name: &str| {
        &libraries
            .iter()
            .find(|library| library.name == name)
            .expect("every store is listed")
            .state
    };
    assert!(matches!(state("Openable"), LibraryState::Openable(None)));
    assert!(
        matches!(state("Unreadable"), LibraryState::Unreadable(_)),
        "{:?}",
        state("Unreadable")
    );
}

#[tokio::test]
async fn discovery_reports_invalid_or_unreadable_active_pointers() {
    let tmp = TempDir::new().unwrap();
    let app_dir = AppDir::at(tmp.path()).unwrap();
    std::fs::write(app_dir.active_library_pointer(), " \n").unwrap();
    assert!(Config::discover_libraries(&app_dir).await.is_err());
    std::fs::remove_file(app_dir.active_library_pointer()).unwrap();
    std::fs::create_dir(app_dir.active_library_pointer()).unwrap();
    assert!(Config::discover_libraries(&app_dir).await.is_err());
}

#[test]
fn installation_id_is_shared_by_libraries_and_survives_reopening() {
    let tmp = TempDir::new().unwrap();
    let app_dir = AppDir::at(tmp.path()).unwrap();
    let ids = coven::SequentialIds::new();
    let first = app_dir.install_id(&ids).unwrap();
    assert_eq!(app_dir.install_id(&ids).unwrap(), first);
    assert_eq!(
        AppDir::at(tmp.path()).unwrap().install_id(&ids).unwrap(),
        first
    );
    std::fs::write(tmp.path().join("install-id"), "invalid").unwrap();
    assert!(app_dir.install_id(&ids).is_err());
}

#[tokio::test]
async fn an_app_dir_under_a_fresh_home_exists_and_holds_a_new_store() {
    let home = TempDir::new().unwrap();
    let app_dir = AppDir::under_home(home.path()).unwrap();
    assert!(home.path().join(".bae").is_dir());
    let installation = coven::TestCoven::new();
    let handle = crate::library::configure_store(installation.builder(app_dir.store_layout()))
        .create_store("First")
        .await
        .unwrap();
    handle.close().await.unwrap();
}

#[test]
fn a_library_id_is_a_uuid_never_a_path() {
    let id = coven::StoreId(uuid::Uuid::new_v4());
    assert_eq!(parse_library_id(&id.to_string()).unwrap(), id);
    for id in ["../outside", "invalid"] {
        assert!(matches!(parse_library_id(id), Err(ConfigError::Config(_))));
    }
}
