use super::*;
use std::time::Duration;

#[test]
fn validate_concurrency_bounds() {
    assert!(validate_concurrency(0).is_err(), "0 deadlocks coven's drain");
    assert_eq!(validate_concurrency(1).unwrap().get(), 1);
    assert_eq!(
        validate_concurrency(MAX_CONCURRENT_TRANSFERS).unwrap().get(),
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
    assert!(config.validate().is_err(), "an enabled server with no username authenticates no one");
}

#[test]
fn subsonic_config_rejects_non_ip_bind_address() {
    let config = SubsonicConfig {
        enabled: true,
        port: SUBSONIC_DEFAULT_PORT,
        username: "listener".to_string(),
        bind_address: "not-an-ip".to_string(),
    };
    assert!(config.validate().is_err(), "a bind address that isn't an IP must be rejected");
}

#[test]
fn subsonic_config_allows_lan_bind_address() {
    let config = SubsonicConfig {
        enabled: true,
        port: SUBSONIC_DEFAULT_PORT,
        username: "listener".to_string(),
        bind_address: "0.0.0.0".to_string(),
    };
    assert!(config.validate().is_ok(), "0.0.0.0 opens the server to the network and is valid");
}

#[test]
fn subsonic_config_allows_disabled_without_username() {
    let config = SubsonicConfig::disabled_default();
    assert!(config.username.is_empty());
    assert_eq!(config.bind_address, "127.0.0.1");
    assert!(config.validate().is_ok(), "a disabled server needs no username");
}

/// The row names a column per catalog, but nothing else reads them by name:
/// the accessors are total over the catalogs bae asks, so each has an answer
/// and a new one cannot be silently forgotten.
#[test]
fn lookup_catalog_preferences_are_total_over_the_asked_catalogs() {
    let mut prefs = LookupCatalogPreferences::default();
    for catalog in bae_catalogs::catalog::LookupCatalog::ALL {
        assert!(prefs.enabled(catalog), "every asked catalog starts asked");
    }

    prefs.set(bae_catalogs::catalog::LookupCatalog::Discogs, false);
    assert!(prefs.enabled(bae_catalogs::catalog::LookupCatalog::MusicBrainz));
    assert!(!prefs.enabled(bae_catalogs::catalog::LookupCatalog::Discogs));
}

/// The availability list folds two questions that are not the same — whether
/// the person wants a source asked, and whether this library holds what it
/// needs — into the one answer everything that asks the sources together
/// reads. The switch comes first: a source switched off is off whatever it
/// holds, and one switched on without its credential says it needs one.
#[test]
fn metadata_sources_fold_the_credential_and_the_switch() {
    use crate::SourceAvailability;
    use bae_catalogs::catalog::LookupCatalog;

    let mut prefs = Preferences::default();
    assert_eq!(
        prefs
            .metadata_sources()
            .into_iter()
            .map(|entry| (entry.catalog, entry.state))
            .collect::<Vec<_>>(),
        vec![
            (LookupCatalog::MusicBrainz, SourceAvailability::On),
            (LookupCatalog::Discogs, SourceAvailability::NotConfigured),
        ],
        "a fresh library has Discogs switched on and no key, so it needs one"
    );

    prefs.identification.catalogs.set(LookupCatalog::Discogs, false);
    assert_eq!(
        prefs.metadata_sources()[1].state,
        SourceAvailability::Off,
        "switched off, Discogs is off, key or no key"
    );
    prefs.identification.catalogs.set(LookupCatalog::Discogs, true);

    prefs.discogs = Some(DiscogsValidation::Valid);
    prefs.identification.catalogs.set(LookupCatalog::Discogs, false);
    assert_eq!(
        prefs.metadata_sources()[1].state,
        SourceAvailability::Off,
        "with a key, a switch that is off is off"
    );

    prefs.identification.catalogs.set(LookupCatalog::Discogs, true);
    prefs.discogs = Some(DiscogsValidation::Rejected);
    assert_eq!(
        prefs.metadata_sources()[1].state,
        SourceAvailability::NotConfigured,
        "a rejected key is no key: switched on, it needs another"
    );
}

/// Search Discogs is a switch like any other: the person can move it without
/// a key. Only the last source still asked is held, since switching it off
/// would leave nothing to ask.
#[test]
fn a_switch_can_change_unless_it_is_the_last_source_asked() {
    use bae_catalogs::catalog::LookupCatalog;

    let mut prefs = Preferences::default();
    let can_change = |prefs: &Preferences| {
        prefs
            .lookup_catalog_settings()
            .into_iter()
            .map(|setting| (setting.availability.catalog, setting.can_change))
            .collect::<Vec<_>>()
    };
    assert_eq!(
        can_change(&prefs),
        vec![(LookupCatalog::MusicBrainz, false), (LookupCatalog::Discogs, true)],
        "without a key MusicBrainz is the only source asked, and Discogs still moves"
    );

    prefs.discogs = Some(DiscogsValidation::Valid);
    assert_eq!(
        can_change(&prefs),
        vec![(LookupCatalog::MusicBrainz, true), (LookupCatalog::Discogs, true)]
    );
}

/// A library asks no source once every source is switched off or lacks what it
/// needs: MusicBrainz off and Discogs without a key.
#[test]
fn a_library_with_every_source_off_or_unreachable_asks_none() {
    use crate::asks_a_source;
    use bae_catalogs::catalog::LookupCatalog;

    let mut prefs = Preferences::default();
    assert!(asks_a_source(&prefs.metadata_sources()), "a fresh library asks MusicBrainz");

    prefs.identification.catalogs.set(LookupCatalog::MusicBrainz, false);
    assert!(!asks_a_source(&prefs.metadata_sources()), "Discogs has no key to be asked with");

    prefs.discogs = Some(DiscogsValidation::Valid);
    assert!(asks_a_source(&prefs.metadata_sources()), "a key lets Discogs be asked");
}

/// The rule that decides both the refusal and the greyed-out switch: switching
/// off the only source still being asked would leave nothing to ask.
#[test]
fn the_only_asked_source_is_the_one_that_cannot_be_switched_off() {
    use crate::is_the_only_asked_source;
    use bae_catalogs::catalog::LookupCatalog;

    let mut prefs = Preferences::default();
    let sources = prefs.metadata_sources();
    assert!(is_the_only_asked_source(&sources, LookupCatalog::MusicBrainz));
    assert!(
        !is_the_only_asked_source(&sources, LookupCatalog::Discogs),
        "a source nothing is asking is not the last one asked"
    );

    prefs.discogs = Some(DiscogsValidation::Valid);
    let sources = prefs.metadata_sources();
    for catalog in LookupCatalog::ALL {
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
    assert!(matches!(prefs.discogs_token_status(), DiscogsTokenStatus::NotConfigured));

    prefs.discogs = Some(DiscogsValidation::Unvalidated);
    assert!(matches!(prefs.discogs_token_status(), DiscogsTokenStatus::Unvalidated));

    prefs.discogs = Some(DiscogsValidation::Rejected);
    assert!(matches!(prefs.discogs_token_status(), DiscogsTokenStatus::Rejected));
}

#[test]
fn side_pause_countdown_duration_is_the_offered_length() {
    assert_eq!(SidePauseCountdown::Off.duration(), None);
    assert_eq!(SidePauseCountdown::Seconds5.duration(), Some(Duration::from_secs(5)));
    assert_eq!(SidePauseCountdown::Seconds15.duration(), Some(Duration::from_secs(15)));
    assert_eq!(SidePauseCountdown::Seconds30.duration(), Some(Duration::from_secs(30)));
    assert_eq!(SidePauseCountdown::Seconds45.duration(), Some(Duration::from_secs(45)));
    assert_eq!(SidePauseCountdown::Seconds60.duration(), Some(Duration::from_secs(60)));
}

/// A key is stored as Discogs' check of it left it: rejected only on a 401,
/// unvalidated only when Discogs could not be reached.
#[cfg(desktop)]
#[test]
fn validation_folds_validate_token_outcomes() {
    use bae_catalogs::discogs::client::DiscogsError;
    use bae_catalogs::providers::DiscogsCheck;

    let valid = DiscogsCheck::of(Ok(())).unwrap();
    assert_eq!(DiscogsValidation::of(&valid), DiscogsValidation::Valid);
    // A 401 is the one outcome that rejects the stored key.
    let rejected = DiscogsCheck::of(Err(DiscogsError::InvalidApiKey)).unwrap();
    assert_eq!(DiscogsValidation::of(&rejected), DiscogsValidation::Rejected);
    // The fold reads the variant; the cause it keeps is only for logs.
    let offline = DiscogsCheck::Offline(DiscogsError::Provider {
        status: reqwest::StatusCode::SERVICE_UNAVAILABLE,
        told_wait: None,
    });
    assert_eq!(DiscogsValidation::of(&offline), DiscogsValidation::Unvalidated);
}

/// An import goes where the stored choice says, and to the cloud only when
/// there is a home to go to.
#[test]
fn an_import_goes_to_the_cloud_only_where_there_is_storage() {
    assert_eq!(ImportDestination::default(), ImportDestination::CloudPinned);
    for destination in [ImportDestination::Cloud, ImportDestination::CloudPinned] {
        assert_eq!(destination.with_storage(true), destination);
        assert_eq!(
            destination.with_storage(false),
            ImportDestination::ThisDevice,
            "without storage, imports stay on this device"
        );
    }
    for storage in [true, false] {
        assert_eq!(
            ImportDestination::ThisDevice.with_storage(storage),
            ImportDestination::ThisDevice
        );
    }
}

/// Only the pinned choice keeps the files whole here once they are stored.
#[test]
fn only_the_pinned_choice_keeps_files_here() {
    assert!(!ImportDestination::ThisDevice.goes_to_cloud());
    assert!(ImportDestination::Cloud.goes_to_cloud());
    assert!(ImportDestination::CloudPinned.goes_to_cloud());
    assert!(!ImportDestination::Cloud.keeps_files_here());
    assert!(ImportDestination::CloudPinned.keeps_files_here());
    assert!(!ImportDestination::ThisDevice.keeps_files_here());
}
