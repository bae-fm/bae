#[tokio::test]
async fn set_save_presets_rejects_removing_selected_default() {
    let (manager, _temp_dir) = setup_test_manager().await;
    manager
        .set_default_track_save_preset("mp3".to_string()).await
        .unwrap();

    let presets_without_mp3: Vec<_> = manager
        .save_presets()
        .into_iter()
        .filter(|preset| preset.id != "mp3")
        .collect();
    let err = manager
        .set_save_presets(presets_without_mp3).await
        .expect_err("selected default preset cannot be removed");

    assert!(err.to_string().contains("unknown export preset mp3"));
    assert!(manager
        .save_presets()
        .iter()
        .any(|preset| preset.id == "mp3"));
}

/// A release-only preset (single-file CUE) that a track save must refuse.
#[cfg(test)]
fn release_only_image_preset() -> crate::config::SavePreset {
    crate::config::SavePreset {
        id: "flac-image".to_string(),
        name: "FLAC image".to_string(),
        codec: crate::config::SaveCodec::Flac {
            bit_depth: crate::config::SaveBitDepth::Bits16,
        },
        filename_tokens: vec![crate::config::SaveFilenameToken::Title],
        pregap_placement: crate::config::SavePregapPlacement::SingleFileWithCue,
        applies_to_track: false,
        applies_to_release: true,
        embed_cover: true,
    }
}

/// A save default must name a preset that exists and applies to its level.
#[tokio::test]
async fn set_default_save_preset_rejects_unknown_and_wrong_level() {
    let (manager, _temp_dir) = setup_test_manager().await;

    assert!(
        manager
            .set_default_track_save_preset("no-such-preset".to_string()).await
            .is_err(),
        "an unknown preset id is rejected"
    );

    let mut presets = manager.save_presets();
    presets.push(release_only_image_preset());
    manager.set_save_presets(presets).await.unwrap();

    assert!(
        manager
            .set_default_track_save_preset("flac-image".to_string()).await
            .is_err(),
        "a release-only preset can't be the track-save default"
    );
    manager
        .set_default_release_save_preset("flac-image".to_string()).await
        .expect("a release-applicable preset is a valid release default");
}

/// `save_track` resolves the preset first, so a release-only preset is refused
/// before any track work — the id need not even exist as a track.
#[tokio::test]
async fn save_track_rejects_release_only_preset() {
    let (manager, _temp_dir) = setup_test_manager().await;
    let mut presets = manager.save_presets();
    presets.push(release_only_image_preset());
    manager.set_save_presets(presets).await.unwrap();

    let err = manager
        .save_track(
            "any-track",
            std::path::Path::new("/tmp/out.flac"),
            "flac-image",
        )
        .await
        .expect_err("a release-only preset can't back a track save");
    assert!(
        err.to_string().contains("not available for track save"),
        "unexpected error: {err}"
    );
}

/// The preset is captured whole at enqueue: editing (or deleting) it afterward
/// can't change or break the already-queued save.
#[tokio::test]
async fn enqueue_release_save_captures_the_preset() {
    let (manager, temp_dir) = setup_test_manager().await;
    let release_id = insert_pinnable_release(&manager).await;
    manager.set_outputs_paused(true).await.unwrap();

    let target = temp_dir.path().join("save-out");
    manager
        .enqueue_release_save(&release_id, target, "flac")
        .await
        .unwrap();

    // Rename the "flac" preset after enqueue; the queued save keeps the old one.
    let edited: Vec<_> = manager
        .save_presets()
        .into_iter()
        .map(|mut preset| {
            if preset.id == "flac" {
                preset.name = "FLAC EDITED".to_string();
            }
            preset
        })
        .collect();
    manager.set_save_presets(edited).await.unwrap();

    let snap = output_queue_now(&manager).await;
    let crate::library::OutputKind::Save { preset } = &snap.rows[0].op.payload.kind else {
        panic!("expected a queued save op");
    };
    assert_eq!(
        preset.name, "FLAC",
        "the queued save uses the preset captured at enqueue, not the edited one"
    );
}

/// A cover replacement is planned against the cover it replaces. One that
/// lands in between wins, and the planned replacement is refused rather than
/// overwriting a cover its planner never saw.
#[tokio::test]
async fn replacing_a_cover_refuses_a_replacement_that_landed_first() {
    let (manager, _temp_dir, _album, release) = manager_with_release().await;
    store_test_cover(&manager, &release.id, b"first cover").await;
    let concurrent = manager.clone();
    let concurrent_release = release.id.clone();
    let late = DbLibraryImage::cover(&release.id, "local", None, manager.clock.now().into());

    let error = manager
        .database
        .write_library_image_after_planning_for_test(&late, b"late cover", move || async move {
            store_test_cover(&concurrent, &concurrent_release, b"between cover").await;
        })
        .await
        .expect_err("a replacement planned against a superseded cover is refused");

    assert!(error.to_string().contains("changed after planning"));
    assert_eq!(
        manager.read_cover_image_bytes(&release.id).await.unwrap(),
        Some(b"between cover".to_vec()),
        "the replacement that landed first stands"
    );
}

/// Removing the active library closes its custody and removes both directories.
#[tokio::test]
async fn removal_releases_custody_and_preferences() {
    let (manager, _temp) = setup_test_manager().await;
    let id = manager.database.store_id().to_string();
    manager.ensure_mcp_token().await.unwrap();
    manager.set_mcp_config(crate::config::McpConfig::disabled_default()).await.unwrap();
    manager.get_config().save_active_library(&manager.app_dir).unwrap();
    manager.remove_library().await.unwrap();
    assert!(manager.get_mcp_token().await.is_err());
    assert!(!manager.app_dir.registered_library(&id).exists());
    assert!(manager.app_dir.store_layout().stores().await.unwrap().is_empty());
    assert!(!manager.app_dir.active_library_pointer().exists());
}

/// What removal would lose is read without removing anything; the removal
/// then discards it.
#[cfg(feature = "test-utils")]
#[tokio::test]
async fn removal_discards_the_unsent_work_it_reports() {
    let (manager, _temp) = setup_test_manager().await;
    connect_test_cloud(&manager).await;
    let files = TempDir::new().unwrap();
    insert_release_with_queued_uploads(&manager, files.path(), "Album Title", &[("01.flac", b"audio")])
        .await;
    let id = manager.database.store_id().to_string();

    let work = manager.unsent_before_removal().await.unwrap();
    assert!(work.writes > 0, "{work:?}");
    assert_eq!(work.files, 1);
    assert_eq!(manager.app_dir.store_layout().stores().await.unwrap().len(), 1);

    manager.remove_library().await.unwrap();
    assert!(manager.app_dir.store_layout().stores().await.unwrap().is_empty());
    assert!(!manager.app_dir.registered_library(&id).exists());
}

/// A library with no storage set up has nowhere it uploads to, so nothing
/// counts as unsent; removing it deletes its whole catalog.
#[tokio::test]
async fn a_library_without_storage_reports_nothing_unsent() {
    let (manager, _temp) = setup_test_manager().await;
    let work = manager.unsent_before_removal().await.unwrap();
    assert!(work.is_empty(), "{work:?}");
}

#[tokio::test]
async fn mcp_config_rejects_port_zero_and_persists_valid_config() {
    let (manager, _temp_dir) = setup_test_manager().await;
    let invalid = crate::config::McpConfig {
        enabled: true,
        port: 0,
    };
    assert!(manager.set_mcp_config(invalid).await.is_err());
    assert_eq!(
        manager.get_config().prefs.mcp,
        crate::config::McpConfig::disabled_default()
    );

    let valid = crate::config::McpConfig {
        enabled: true,
        port: crate::config::MCP_DEFAULT_PORT + 1,
    };
    manager.set_mcp_config(valid).await.unwrap();
    assert_eq!(manager.get_config().prefs.mcp, valid);
}

#[tokio::test]
async fn mcp_token_is_keyring_backed_and_sets_target() {
    let (manager, _temp_dir) = setup_test_manager().await;
    assert!(manager.get_mcp_token().await.unwrap().is_none());

    let token = manager.ensure_mcp_token().await.unwrap();
    assert_eq!(token.len(), 64);
    assert!(token.chars().all(|ch| ch.is_ascii_hexdigit()));
    assert_eq!(manager.ensure_mcp_token().await.unwrap(), token);

    let replacement = "a".repeat(64);
    manager.set_mcp_token(replacement.clone()).await.unwrap();
    assert_eq!(
        manager.get_mcp_token().await.unwrap().as_deref(),
        Some(replacement.as_str())
    );
}

#[tokio::test]
async fn subsonic_password_is_keyring_backed() {
    let (manager, _temp_dir) = setup_test_manager().await;
    assert!(manager.get_subsonic_password().await.unwrap().is_none());

    manager.set_subsonic_password("s3cret".to_string()).await.unwrap();
    assert_eq!(
        manager.get_subsonic_password().await.unwrap().as_deref(),
        Some("s3cret")
    );

    manager
        .set_subsonic_password("rotated".to_string()).await
        .unwrap();
    assert_eq!(
        manager.get_subsonic_password().await.unwrap().as_deref(),
        Some("rotated")
    );
}

#[tokio::test]
async fn subsonic_config_rejects_invalid_and_persists_valid() {
    let (manager, _temp_dir) = setup_test_manager().await;
    let enabled_without_username = crate::config::SubsonicConfig {
        enabled: true,
        port: crate::config::SUBSONIC_DEFAULT_PORT,
        username: String::new(),
        bind_address: "127.0.0.1".to_string(),
    };
    assert!(manager
        .set_subsonic_config(enabled_without_username).await
        .is_err());
    assert_eq!(
        manager.get_config().prefs.subsonic,
        crate::config::SubsonicConfig::disabled_default()
    );

    let valid = crate::config::SubsonicConfig {
        enabled: true,
        port: crate::config::SUBSONIC_DEFAULT_PORT + 1,
        username: "listener".to_string(),
        bind_address: "0.0.0.0".to_string(),
    };
    manager.set_subsonic_config(valid.clone()).await.unwrap();
    assert_eq!(manager.get_config().prefs.subsonic, valid);
}

/// Something has to be asked. Core refuses the write that would leave nothing,
/// and refuses it whichever source is last — the rule is about the list, not
/// about MusicBrainz.
#[tokio::test]
async fn the_last_source_being_asked_cannot_be_switched_off() {
    use crate::import::SourceAvailability;
    use bae_catalogs::catalog::Catalog;

    let (manager, _temp_dir) = setup_test_manager().await;

    // No Discogs key in a fresh library, so MusicBrainz is the only one asked.
    let states: Vec<_> = manager
        .metadata_sources()
        .into_iter()
        .map(|entry| (entry.catalog, entry.state))
        .collect();
    assert_eq!(
        states,
        vec![
            (Catalog::MusicBrainz, SourceAvailability::On),
            (Catalog::Discogs, SourceAvailability::NotConfigured),
        ]
    );

    let refused = manager
        .set_metadata_source_enabled(Catalog::MusicBrainz, false).await
        .expect_err("the only source being asked cannot be switched off");
    assert!(
        refused.to_string().contains("MusicBrainz"),
        "the refusal names the source: {refused}"
    );
    assert_eq!(
        manager.metadata_sources()[0].state,
        SourceAvailability::On,
        "the refused write changed nothing"
    );
}

/// A source with no credential reports why it is not asked rather than looking
/// switched off, and the person's switch is kept underneath it: turning it off
/// is allowed (it is not the last one asked) and is what comes back when the
/// credential arrives.
#[tokio::test]
async fn an_unreachable_source_keeps_the_switch_underneath_it() {
    use crate::import::SourceAvailability;
    use bae_catalogs::catalog::Catalog;

    let (manager, _temp_dir) = setup_test_manager().await;

    manager
        .set_metadata_source_enabled(Catalog::Discogs, false).await
        .expect("switching off a source nothing is asking is allowed");
    assert_eq!(
        manager.metadata_sources()[1].state,
        SourceAvailability::NotConfigured,
        "no credential still beats the switch"
    );
    assert!(
        !manager
            .get_config()
            .prefs
            .identification
            .catalogs
            .enabled(Catalog::Discogs),
        "the switch is kept where the person left it"
    );
}
