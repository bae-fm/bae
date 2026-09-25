#[tokio::test]
async fn set_save_presets_rejects_removing_selected_default() {
    let (manager, _temp_dir) = setup_test_manager().await;
    manager
        .set_default_track_save_preset("mp3".to_string())
        .unwrap();

    let presets_without_mp3: Vec<_> = manager
        .save_presets()
        .into_iter()
        .filter(|preset| preset.id != "mp3")
        .collect();
    let err = manager
        .set_save_presets(presets_without_mp3)
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
            bit_depth: crate::config::SaveBitDepth::Source,
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
            .set_default_track_save_preset("no-such-preset".to_string())
            .is_err(),
        "an unknown preset id is rejected"
    );

    let mut presets = manager.save_presets();
    presets.push(release_only_image_preset());
    manager.set_save_presets(presets).unwrap();

    assert!(
        manager
            .set_default_track_save_preset("flac-image".to_string())
            .is_err(),
        "a release-only preset can't be the track-save default"
    );
    manager
        .set_default_release_save_preset("flac-image".to_string())
        .expect("a release-applicable preset is a valid release default");
}

/// `save_track` resolves the preset first, so a release-only preset is refused
/// before any track work — the id need not even exist as a track.
#[tokio::test]
async fn save_track_rejects_release_only_preset() {
    let (manager, _temp_dir) = setup_test_manager().await;
    let mut presets = manager.save_presets();
    presets.push(release_only_image_preset());
    manager.set_save_presets(presets).unwrap();

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
    manager.set_outputs_paused(true);

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
    manager.set_save_presets(edited).unwrap();

    let snap = manager.output_snapshot();
    let crate::library::OutputKind::Save { preset } = &snap.ops[0].payload.kind else {
        panic!("expected a queued save op");
    };
    assert_eq!(
        preset.name, "FLAC",
        "the queued save uses the preset captured at enqueue, not the edited one"
    );
}

/// Break one of bae's own tables so the next read or write against it fails,
/// standing in for a database that has gone bad under a delete. Only bae's tables
/// are renameable: coven's SQL authorizer refuses a host statement that alters one
/// of its reserved tables, so a cleanup step coven owns is failed by handing it
/// input it refuses instead (see the rollback tests below).
async fn rename_table_for_test(manager: &LibraryManager, table: &str) {
    manager
        .database
        .rename_host_table_for_test(table)
        .await
        .unwrap();
}

async fn store_test_cover_image(manager: &LibraryManager, release_id: &str) {
    store_test_cover_image_with_blob(manager, release_id, COVER_BLOB).await;
}

/// Write a release's cover row and its blob. A coven blob id names one immutable
/// byte-string, so replacing a cover means a NEW `blob_id` on the same row — pass a
/// different `blob_suffix` to stand in for what `change_cover` does.
async fn store_test_cover_image_with_blob(
    manager: &LibraryManager,
    release_id: &str,
    blob_suffix: &str,
) {
    manager
        .store_library_image_blob(
            &DbLibraryImage {
                id: release_id.to_string(),
                blob_id: bae_test_support::test_uuid(&format!("{release_id}-{blob_suffix}")),
                image_type: LibraryImageType::Cover,
                content_type: crate::util::content_type::ContentType::Jpeg,
                file_size: 5,
                width: None,
                height: None,
                source: "local".to_string(),
                source_url: None,
                cloud_path: None,
                // The hash must be of the bytes actually stored: coven verifies
                // a blob against its row's signed hash.
                content_hash: crate::util::fs::hash_bytes(b"image"),
                created_at: manager.clock.now(),
            },
            b"image",
        )
        .await
        .unwrap();
}

/// Replacing a cover declares the blob it replaces deleted, and that blob is
/// read before the write opens. A replacement that lands in between would
/// leave its blob undeclared and its bytes in coven's store for good, so the
/// write checks the row still names the blob it planned to replace and
/// refuses otherwise.
#[tokio::test]
async fn replacing_a_cover_refuses_a_replacement_that_landed_first() {
    let (manager, _temp_dir, _album, release) = manager_with_release().await;
    store_test_cover_image_with_blob(&manager, &release.id, "cover-first").await;
    let concurrent = manager.clone();
    let concurrent_release = release.id.clone();
    let late_blob = bae_test_support::test_uuid(&format!("{}-cover-late", release.id));
    let mut late = manager
        .get_library_image(&release.id, &LibraryImageType::Cover)
        .await
        .unwrap()
        .expect("the first cover is stored");
    late.blob_id = late_blob.clone();

    let error = manager
        .database
        .write_library_image_blob_after_planning_for_test(&late, b"image", move || async move {
            store_test_cover_image_with_blob(&concurrent, &concurrent_release, "cover-between")
                .await;
        })
        .await
        .expect_err("a replacement planned against a superseded cover is refused");

    assert!(error.to_string().contains("changed after planning"));
    let stored = manager
        .get_library_image(&release.id, &LibraryImageType::Cover)
        .await
        .unwrap()
        .expect("the cover row remains");
    assert_eq!(
        stored.blob_id,
        bae_test_support::test_uuid(&format!("{}-cover-between", release.id)),
        "the replacement that landed first stands"
    );
}

/// A library opened at its registered directory, connected to a test cloud
/// home (so it holds a device identity, a master key, and cloud credentials),
/// with a host secret stored beside them.
async fn open_library_to_forget(
    app_dir: &crate::config::AppDir,
    library_id: &str,
) -> LibraryManager {
    let library_dir = app_dir.registered_library(library_id);
    let mut config = Config::with_defaults(
        library_id.to_string(),
        "test-device".to_string(),
        StoreDir::new(library_dir.clone()),
        "Test Library".to_string(),
    );
    config.cloud_home.provider = Some(crate::config::CloudProvider::CloudKit);
    crate::config::install_test_keyring();
    let database = Database::open(
        StoreDir::new(library_dir),
        config.inner.clone(),
        Arc::new(coven::SystemClock),
        crate::sync::synced_tables(),
        None,
    )
    .unwrap();
    let manager = LibraryManager::new(
        database,
        app_dir.clone(),
        Arc::new(ConfigHandle::new(config)),
        Arc::new(coven::SystemClock),
        Arc::new(coven::UuidProvider),
        crate::diagnostics::Diagnostics::noop(),
        tokio::runtime::Handle::current(),
        crate::import::cover_art::RemoteImageCache::for_test(crate::util::http::Http::for_test()),
        crate::providers::Providers::offline(),
    );
    manager
        .connect_test_cloud_home(
            Arc::new(coven::InMemoryCloudHome::new()),
            crate::sync::CloudCipher::Encrypted(coven::EncryptionService::from_key([7u8; 32])),
        )
        .await
        .unwrap();
    manager
        .database
        .set_host_secret(crate::keys::MCP_BEARER_TOKEN, "forget-test-secret")
        .unwrap();
    manager
}

/// Forgetting a library on this device, once it is closed, leaves no keyring
/// entry for it — master key, cloud credentials, host secrets — no directory,
/// and no active-library pointer at it.
#[tokio::test]
async fn forgetting_a_closed_library_leaves_no_keyring_entry_or_directory() {
    let home = TempDir::new().unwrap();
    let app_dir = crate::config::AppDir::under_home(home.path());
    let library_id = format!("forget-{}", Uuid::new_v4());
    let manager = open_library_to_forget(&app_dir, &library_id).await;
    std::fs::write(app_dir.active_library_pointer(), &library_id).unwrap();
    let keys = coven::StoreKeys::bind(library_id.clone());
    assert!(keys.get_encryption_key().unwrap().is_some());
    assert!(keys.get_host_secret(crate::keys::MCP_BEARER_TOKEN).unwrap().is_some());

    manager.close().await;
    coven::assert_no_open_files_under(&app_dir.registered_library(&library_id));
    crate::library::remove_local_library(&app_dir, &library_id).unwrap();

    assert_eq!(keys.get_encryption_key().unwrap(), None);
    assert!(keys.get_cloud_home_credentials().unwrap().is_none());
    for name in crate::keys::HOST_SECRET_NAMES {
        assert_eq!(keys.get_host_secret(name).unwrap(), None, "{name}");
    }
    assert!(!app_dir.registered_library(&library_id).exists());
    assert!(!app_dir.active_library_pointer().exists());
}

/// A library still open anywhere is not removed: coven refuses, and every
/// keyring entry, the directory, and the active-library pointer stay.
#[tokio::test]
async fn removing_an_open_library_is_refused_and_removes_nothing() {
    let home = TempDir::new().unwrap();
    let app_dir = crate::config::AppDir::under_home(home.path());
    let library_id = format!("forget-open-{}", Uuid::new_v4());
    let manager = open_library_to_forget(&app_dir, &library_id).await;
    std::fs::write(app_dir.active_library_pointer(), &library_id).unwrap();

    let error = crate::library::remove_local_library(&app_dir, &library_id)
        .expect_err("an open library is not removed");

    assert!(error.to_string().contains("Failed to remove library data"), "{error}");
    let keys = coven::StoreKeys::bind(library_id.clone());
    assert!(keys.get_encryption_key().unwrap().is_some());
    assert!(app_dir.registered_library(&library_id).exists());
    assert_eq!(
        std::fs::read_to_string(app_dir.active_library_pointer()).unwrap(),
        library_id
    );
    drop(manager);
}

#[tokio::test]
async fn mcp_config_rejects_port_zero_and_persists_valid_config() {
    let (manager, _temp_dir) = setup_test_manager().await;
    let invalid = crate::config::McpConfig {
        enabled: true,
        port: 0,
    };
    assert!(manager.set_mcp_config(invalid).is_err());
    assert_eq!(
        manager.get_config().prefs.mcp,
        crate::config::McpConfig::disabled_default()
    );

    let valid = crate::config::McpConfig {
        enabled: true,
        port: crate::config::MCP_DEFAULT_PORT + 1,
    };
    manager.set_mcp_config(valid).unwrap();
    assert_eq!(manager.get_config().prefs.mcp, valid);
}

#[tokio::test]
async fn mcp_token_is_keyring_backed_and_sets_target() {
    let (manager, _temp_dir) = setup_test_manager().await;
    assert!(manager.get_mcp_token().unwrap().is_none());

    let token = manager.ensure_mcp_token().unwrap();
    assert_eq!(token.len(), 64);
    assert!(token.chars().all(|ch| ch.is_ascii_hexdigit()));
    assert_eq!(manager.ensure_mcp_token().unwrap(), token);

    let replacement = "a".repeat(64);
    manager.set_mcp_token(replacement.clone()).unwrap();
    assert_eq!(
        manager.get_mcp_token().unwrap().as_deref(),
        Some(replacement.as_str())
    );
}

#[tokio::test]
async fn subsonic_password_is_keyring_backed() {
    let (manager, _temp_dir) = setup_test_manager().await;
    assert!(manager.get_subsonic_password().unwrap().is_none());

    manager.set_subsonic_password("s3cret".to_string()).unwrap();
    assert_eq!(
        manager.get_subsonic_password().unwrap().as_deref(),
        Some("s3cret")
    );

    manager
        .set_subsonic_password("rotated".to_string())
        .unwrap();
    assert_eq!(
        manager.get_subsonic_password().unwrap().as_deref(),
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
        .set_subsonic_config(enabled_without_username)
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
    manager.set_subsonic_config(valid.clone()).unwrap();
    assert_eq!(manager.get_config().prefs.subsonic, valid);
}

/// Something has to be asked. Core refuses the write that would leave nothing,
/// and refuses it whichever source is last — the rule is about the list, not
/// about MusicBrainz.
#[tokio::test]
async fn the_last_source_being_asked_cannot_be_switched_off() {
    use crate::import::{Catalog, SourceAvailability};

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
        .set_metadata_source_enabled(Catalog::MusicBrainz, false)
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
    use crate::import::{Catalog, SourceAvailability};

    let (manager, _temp_dir) = setup_test_manager().await;

    manager
        .set_metadata_source_enabled(Catalog::Discogs, false)
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
            .metadata_sources
            .enabled(Catalog::Discogs),
        "the switch is kept where the person left it"
    );
}
