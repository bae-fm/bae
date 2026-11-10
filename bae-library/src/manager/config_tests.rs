//! The library's settings: save presets and the defaults that name them, the
//! MCP and Subsonic servers' settings and secrets, the metadata sources, a
//! Discogs key's check, and the transfer limits the open store takes.

use crate::manager::tests::*;

/// Revalidation surfaces the config-says-stored/keyring-empty mismatch as an
/// error, not a swallowed warning — the one torn state our writes can't produce
/// but external tampering can.
#[tokio::test]
async fn revalidate_errors_when_config_claims_a_key_the_keyring_lacks() {
    use bae_preferences::DiscogsValidation;

    let (test, _temp_dir) = setup_test_manager().await;

    let manager = test.library.manager();
    // Config claims an Unvalidated key; the keyring has none — the torn state.
    manager
        .update_preferences(|prefs| {
            prefs.discogs = Some(DiscogsValidation::Unvalidated);
            Ok(())
        })
        .await
        .unwrap();

    let handle = test.library.start_import().await.unwrap();

    assert!(
        handle.revalidate_discogs_token().await.is_err(),
        "a stored-but-keyless config must fail revalidation, not warn and continue",
    );
}

/// A key checked while Discogs cannot be reached is stored unvalidated, and
/// the save says it was offline; asking again while still offline leaves the
/// key unvalidated.
#[tokio::test]
async fn a_key_checked_offline_is_stored_unvalidated() {
    use bae_catalogs::discogs::client::DiscogsError;
    use bae_catalogs::providers::DiscogsCheck;
    use bae_preferences::DiscogsValidation;

    let (test, _temp_dir) = setup_test_manager().await;

    let manager = test.library.manager();
    let handle = test.library.start_import().await.unwrap();

    let outcome = handle
        .save_discogs_token("discogs-test-key")
        .await
        .expect("the key is stored though it could not be checked");
    let DiscogsCheck::Offline(reason) = outcome else {
        panic!("an unreachable Discogs leaves the key unchecked: {outcome:?}");
    };
    assert!(matches!(reason, DiscogsError::Transport(_)), "{reason}");
    assert_eq!(manager.preferences().await.unwrap().discogs, Some(DiscogsValidation::Unvalidated));

    handle.revalidate_discogs_token().await.expect("still offline is not a failure");
    assert_eq!(manager.preferences().await.unwrap().discogs, Some(DiscogsValidation::Unvalidated));
}

#[tokio::test]
async fn set_save_presets_rejects_removing_the_last_used_format() {
    let (test, _temp_dir) = setup_test_manager().await;
    let manager = test.library.manager();
    manager.set_last_save_preset("mp3".to_string()).await.unwrap();

    let presets_without_mp3: Vec<_> = manager
        .save_presets()
        .await
        .unwrap()
        .into_iter()
        .filter(|preset| preset.id != "mp3")
        .collect();
    let err = manager
        .set_save_presets(presets_without_mp3)
        .await
        .expect_err("the last-used format's preset cannot be removed");

    assert!(err.to_string().contains("unknown export preset mp3"));
    assert!(manager.save_presets().await.unwrap().iter().any(|preset| preset.id == "mp3"));
}

/// The last-used format must name a preset that exists; any format can be it,
/// whichever kind of save used it.
#[tokio::test]
async fn set_last_save_preset_rejects_unknown_and_takes_any_format() {
    let (test, _temp_dir) = setup_test_manager().await;
    let manager = test.library.manager();

    assert!(
        manager.set_last_save_preset("no-such-preset".to_string()).await.is_err(),
        "an unknown preset id is rejected"
    );

    let mut presets = manager.save_presets().await.unwrap();
    presets.push(bae_preferences::SavePreset {
        id: "wav-16".to_string(),
        name: "WAV 16-bit".to_string(),
        codec: bae_preferences::SaveCodec::Wav { bit_depth: bae_preferences::SaveBitDepth::Bits16 },
        embed_cover: true,
    });
    manager.set_save_presets(presets).await.unwrap();

    manager.set_last_save_preset("wav-16".to_string()).await.unwrap();
    assert_eq!(manager.preferences().await.unwrap().last_save_preset, "wav-16");
}

/// Two formats never share a name, whoever asks to write them.
#[tokio::test]
async fn set_save_presets_refuses_a_name_another_format_has() {
    let (test, _temp_dir) = setup_test_manager().await;
    let manager = test.library.manager();
    let mut presets = manager.save_presets().await.unwrap();
    presets[1].name = presets[0].name.clone();

    let err = manager.set_save_presets(presets).await.expect_err("a taken name is refused");

    assert!(err.to_string().contains("another export preset is named FLAC"), "{err}");
    assert_eq!(manager.save_presets().await.unwrap()[1].name, "MP3");
}

#[tokio::test]
async fn mcp_config_rejects_port_zero_and_persists_valid_config() {
    let (test, _temp_dir) = setup_test_manager().await;
    let manager = test.library.manager();
    let invalid = bae_preferences::McpConfig { enabled: true, port: 0 };
    assert!(manager.set_mcp_config(invalid).await.is_err());
    assert_eq!(
        manager.preferences().await.unwrap().mcp,
        bae_preferences::McpConfig::disabled_default()
    );

    let valid =
        bae_preferences::McpConfig { enabled: true, port: bae_preferences::MCP_DEFAULT_PORT + 1 };
    manager.set_mcp_config(valid).await.unwrap();
    assert_eq!(manager.preferences().await.unwrap().mcp, valid);
}

#[tokio::test]
async fn mcp_token_is_keyring_backed_and_sets_target() {
    let (test, _temp_dir) = setup_test_manager().await;
    let manager = test.library.manager();
    assert!(manager.get_mcp_token().await.unwrap().is_none());

    let token = manager.ensure_mcp_token().await.unwrap();
    assert_eq!(token.len(), 64);
    assert!(token.chars().all(|ch| ch.is_ascii_hexdigit()));
    assert_eq!(manager.ensure_mcp_token().await.unwrap(), token);

    let replacement = "a".repeat(64);
    manager.set_mcp_token(replacement.clone()).await.unwrap();
    assert_eq!(manager.get_mcp_token().await.unwrap().as_deref(), Some(replacement.as_str()));
}

#[tokio::test]
async fn subsonic_password_is_keyring_backed() {
    let (test, _temp_dir) = setup_test_manager().await;
    let manager = test.library.manager();
    assert!(manager.get_subsonic_password().await.unwrap().is_none());

    manager.set_subsonic_password("s3cret".to_string()).await.unwrap();
    assert_eq!(manager.get_subsonic_password().await.unwrap().as_deref(), Some("s3cret"));

    manager.set_subsonic_password("rotated".to_string()).await.unwrap();
    assert_eq!(manager.get_subsonic_password().await.unwrap().as_deref(), Some("rotated"));
}

#[tokio::test]
async fn subsonic_config_rejects_invalid_and_persists_valid() {
    let (test, _temp_dir) = setup_test_manager().await;
    let manager = test.library.manager();
    let enabled_without_username = bae_preferences::SubsonicConfig {
        enabled: true,
        port: bae_preferences::SUBSONIC_DEFAULT_PORT,
        username: String::new(),
        bind_address: "127.0.0.1".to_string(),
    };
    assert!(manager.set_subsonic_config(enabled_without_username).await.is_err());
    assert_eq!(
        manager.preferences().await.unwrap().subsonic,
        bae_preferences::SubsonicConfig::disabled_default()
    );

    let valid = bae_preferences::SubsonicConfig {
        enabled: true,
        port: bae_preferences::SUBSONIC_DEFAULT_PORT + 1,
        username: "listener".to_string(),
        bind_address: "0.0.0.0".to_string(),
    };
    manager.set_subsonic_config(valid.clone()).await.unwrap();
    assert_eq!(manager.preferences().await.unwrap().subsonic, valid);
}

/// Something has to be asked. Core refuses the write that would leave nothing,
/// and refuses it whichever source is last — the rule is about the list, not
/// about MusicBrainz.
#[tokio::test]
async fn the_last_source_being_asked_cannot_be_switched_off() {
    use bae_catalogs::catalog::LookupCatalog;
    use bae_preferences::SourceAvailability;

    let (test, _temp_dir) = setup_test_manager().await;

    let manager = test.library.manager();

    // No Discogs key in a fresh library, so MusicBrainz is the only one asked.
    let states: Vec<_> = manager
        .metadata_sources()
        .await
        .unwrap()
        .into_iter()
        .map(|entry| (entry.catalog, entry.state))
        .collect();
    assert_eq!(
        states,
        vec![
            (LookupCatalog::MusicBrainz, SourceAvailability::On),
            (LookupCatalog::Discogs, SourceAvailability::NotConfigured),
        ]
    );

    let refused = manager
        .set_metadata_source_enabled(LookupCatalog::MusicBrainz, false)
        .await
        .expect_err("the only source being asked cannot be switched off");
    assert!(refused.to_string().contains("MusicBrainz"), "the refusal names the source: {refused}");
    assert_eq!(
        manager.metadata_sources().await.unwrap()[0].state,
        SourceAvailability::On,
        "the refused write changed nothing"
    );
}

/// A source switched on without its credential says it needs one rather than
/// looking on; switched off, it is off. Either way the person's switch is what
/// is stored.
#[tokio::test]
async fn an_unreachable_source_reports_the_switch_before_the_missing_key() {
    use bae_catalogs::catalog::LookupCatalog;
    use bae_preferences::SourceAvailability;

    let (test, _temp_dir) = setup_test_manager().await;

    let manager = test.library.manager();
    assert_eq!(
        manager.metadata_sources().await.unwrap()[1].state,
        SourceAvailability::NotConfigured,
        "switched on, with no key, Discogs needs one"
    );

    manager
        .set_metadata_source_enabled(LookupCatalog::Discogs, false)
        .await
        .expect("switching off a source nothing is asking is allowed");
    assert_eq!(
        manager.metadata_sources().await.unwrap()[1].state,
        SourceAvailability::Off,
        "switched off, it is off"
    );
    assert!(
        !manager
            .preferences()
            .await
            .unwrap()
            .identification
            .catalogs
            .enabled(LookupCatalog::Discogs),
        "the switch is stored where the person left it"
    );
}

/// The concurrency settings reach the open store when they are written, not
/// at the next open: the limit the next upload drain and pin read is the one
/// just set.
#[tokio::test]
async fn transfer_concurrency_settings_apply_to_the_open_store() {
    let (test, _temp) = setup_test_manager().await;
    let manager = test.library.manager();
    let before = manager.transfer_limits();

    manager.set_max_concurrent_uploads(5).await.unwrap();
    manager.set_max_concurrent_downloads(2).await.unwrap();

    let after = manager.transfer_limits();
    assert_eq!(after.uploads.get(), 5);
    assert_eq!(after.downloads.get(), 2);
    assert_ne!((before.uploads, before.downloads), (after.uploads, after.downloads));
    assert_eq!(manager.preferences().await.unwrap().max_concurrent_uploads.get(), 5);
    assert_eq!(manager.preferences().await.unwrap().max_concurrent_downloads.get(), 2);

    assert!(manager.set_max_concurrent_uploads(0).await.is_err());
    assert_eq!(manager.transfer_limits().uploads.get(), 5, "a rejected value changes nothing");
}
