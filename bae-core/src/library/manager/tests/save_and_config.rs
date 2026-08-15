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

/// Whether coven's durable queue holds a cloud tombstone for this blob. A
/// tombstone outlives the row that named it, so `(namespace, blob_id)` is all
/// there is to identify it by.
async fn has_queued_delete(manager: &LibraryManager, namespace: &str, blob_id: &str) -> bool {
    manager
        .database
        .has_queued_delete_for_test(namespace, blob_id)
        .await
        .unwrap()
}

/// Break one of bae's own tables so the next read or write against it fails,
/// standing in for a database that has gone bad under a delete. Only bae's tables
/// are renameable: coven's SQL authorizer refuses a host statement that alters one
/// of its reserved tables, so a cleanup step coven owns is failed by handing it
/// input it refuses instead (see the rollback tests below).
async fn rename_table_for_test(manager: &LibraryManager, from: &str, to: &str) {
    match (from, to) {
        ("release_files", "release_files_unavailable") => manager
            .database
            .rename_release_files_table_for_test()
            .await
            .unwrap(),
        ("tracks", "tracks_unavailable") => manager
            .database
            .rename_tracks_table_for_test()
            .await
            .unwrap(),
        ("covers", "covers_unavailable") => manager
            .database
            .rename_covers_table_for_test()
            .await
            .unwrap(),
        _ => panic!("unsupported table sabotage: {from} -> {to}"),
    }
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

async fn setup_forget_library_manager(library_id: &str, home: &std::path::Path) -> LibraryManager {
    let bae_dir = home.join(".bae");
    let library_dir = crate::config::registered_library_path(&bae_dir, library_id);
    setup_forget_library_manager_at(library_id, library_dir, home).await
}

async fn setup_forget_library_manager_at(
    library_id: &str,
    library_dir: std::path::PathBuf,
    home: &std::path::Path,
) -> LibraryManager {
    let db_path = home.join("manager.db");
    let database = Database::new_test(
        db_path.to_str().unwrap(),
        Arc::new(coven::SystemClock),
        std::sync::Arc::new(coven::UuidProvider),
    )
    .await
    .unwrap();
    let mut config = Config::with_defaults(
        library_id.to_string(),
        "test-device".to_string(),
        StoreDir::new(library_dir.clone()),
        "Test Library".to_string(),
    );
    config.cloud_home.provider = Some(crate::config::CloudProvider::CloudKit);
    let cloud_home = config.cloud_home.clone();
    let config_handle = Arc::new(ConfigHandle::new(config));
    crate::config::install_test_keyring();
    let manager = LibraryManager::new(
        database,
        config_handle,
        Arc::new(coven::SystemClock),
        Arc::new(coven::UuidProvider),
        crate::diagnostics::Diagnostics::noop(),
        tokio::runtime::Handle::current(),
        crate::import::cover_art::RemoteImageCache::for_test(),
    );
    manager
        .database
        .setup_cloud_home_with_test_home(
            cloud_home,
            Arc::new(coven::InMemoryCloudHome::new()),
        )
        .await
        .unwrap();
    manager
        .database
        .set_host_secret(crate::keys::MCP_BEARER_TOKEN, "forget-test-secret")
        .unwrap();
    manager
}

fn assert_forget_material_available(manager: &LibraryManager) {
    assert_eq!(
        manager.cloud_home_key_state().unwrap(),
        coven::CloudHomeKeyState::Available
    );
    assert_eq!(
        manager
            .database
            .host_secret(crate::keys::MCP_BEARER_TOKEN)
            .unwrap()
            .as_deref(),
        Some("forget-test-secret")
    );
}

fn assert_forget_material_removed(manager: &LibraryManager) {
    assert_eq!(
        manager.cloud_home_key_state().unwrap(),
        coven::CloudHomeKeyState::Locked
    );
    assert_eq!(
        manager
            .database
            .host_secret(crate::keys::MCP_BEARER_TOKEN)
            .unwrap(),
        None
    );
}

fn setup_forget_library_home(library_id: &str) -> (TempDir, std::path::PathBuf) {
    let home = TempDir::new().unwrap();
    let bae_dir = home.path().join(".bae");
    let library_dir = crate::config::registered_library_path(&bae_dir, library_id);
    (home, library_dir)
}

#[tokio::test]
async fn forget_library_returns_error_when_registered_path_cannot_be_removed() {
    let library_id = format!("forget-fails-{}", Uuid::new_v4());
    let (home, library_path) = setup_forget_library_home(&library_id);
    let bae_dir = home.path().join(".bae");
    std::fs::create_dir_all(library_path.parent().unwrap()).unwrap();
    std::fs::write(&library_path, b"not a directory").unwrap();
    std::fs::write(bae_dir.join("active-library"), &library_id).unwrap();
    let manager = setup_forget_library_manager(&library_id, home.path()).await;
    let err = manager
        .forget_library()
        .await
        .expect_err("directory removal failure must surface");

    assert!(
        err.to_string().contains("Failed to remove library data"),
        "error should name the failed library data deletion: {err}"
    );
    assert_eq!(
        std::fs::read_to_string(bae_dir.join("active-library")).unwrap(),
        library_id
    );
    assert_forget_material_available(&manager);
}

#[tokio::test]
async fn forget_library_removes_registered_path_active_pointer_and_key() {
    let library_id = format!("forget-succeeds-{}", Uuid::new_v4());
    let (home, library_path) = setup_forget_library_home(&library_id);
    let bae_dir = home.path().join(".bae");
    std::fs::create_dir_all(&library_path).unwrap();
    std::fs::write(library_path.join("config.yaml"), b"library data").unwrap();
    std::fs::write(bae_dir.join("active-library"), &library_id).unwrap();
    let manager = setup_forget_library_manager(&library_id, home.path()).await;
    manager.forget_library().await.unwrap();

    assert!(!library_path.exists());
    assert!(!bae_dir.join("active-library").exists());
    assert_forget_material_removed(&manager);
}

#[tokio::test]
async fn forget_library_accepts_missing_directory_and_pointer_on_retry() {
    let library_id = format!("forget-retry-{}", Uuid::new_v4());
    let (home, library_path) = setup_forget_library_home(&library_id);
    let bae_dir = home.path().join(".bae");
    std::fs::create_dir_all(&bae_dir).unwrap();
    let manager = setup_forget_library_manager(&library_id, home.path()).await;
    manager.forget_library().await.unwrap();

    assert!(!library_path.exists());
    assert!(!bae_dir.join("active-library").exists());
    assert_forget_material_removed(&manager);
}

#[tokio::test]
async fn forget_library_returns_error_when_active_pointer_cannot_be_read() {
    let library_id = format!("forget-pointer-fails-{}", Uuid::new_v4());
    let (home, library_path) = setup_forget_library_home(&library_id);
    let bae_dir = home.path().join(".bae");
    std::fs::create_dir_all(&library_path).unwrap();
    std::fs::create_dir(bae_dir.join("active-library")).unwrap();
    let manager = setup_forget_library_manager(&library_id, home.path()).await;
    let err = manager
        .forget_library()
        .await
        .expect_err("active pointer read failure must surface");

    assert!(
        err.to_string()
            .contains("Failed to read active-library pointer"),
        "error should name the failed active pointer read: {err}"
    );
    assert!(library_path.exists());
    assert!(bae_dir.join("active-library").is_dir());
    assert_forget_material_available(&manager);
}

#[tokio::test]
async fn forget_library_returns_error_when_active_pointer_names_another_library() {
    let library_id = format!("forget-pointer-mismatch-{}", Uuid::new_v4());
    let (home, library_path) = setup_forget_library_home(&library_id);
    let bae_dir = home.path().join(".bae");
    std::fs::create_dir_all(&library_path).unwrap();
    std::fs::write(bae_dir.join("active-library"), "different-library").unwrap();
    let manager = setup_forget_library_manager(&library_id, home.path()).await;
    let err = manager
        .forget_library()
        .await
        .expect_err("active pointer mismatch must surface");

    assert!(
        err.to_string().contains("points at different-library"),
        "error should name the active pointer mismatch: {err}"
    );
    assert!(library_path.exists());
    assert_eq!(
        std::fs::read_to_string(bae_dir.join("active-library")).unwrap(),
        "different-library"
    );
    assert_forget_material_available(&manager);
}

#[tokio::test]
async fn forget_library_rejects_unregistered_library_dir() {
    let library_id = format!("forget-unregistered-{}", Uuid::new_v4());
    let home = TempDir::new().unwrap();
    let library_path = home.path().join("external-library");
    std::fs::create_dir_all(&library_path).unwrap();
    let manager =
        setup_forget_library_manager_at(&library_id, library_path.clone(), home.path()).await;
    let err = manager
        .forget_library()
        .await
        .expect_err("unregistered library directory must fail loudly");

    assert!(
        err.to_string().contains("does not match library id"),
        "error should name the unregistered library directory: {err}"
    );
    assert!(library_path.exists());
    assert_forget_material_available(&manager);
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
        manager.get_config().mcp,
        crate::config::McpConfig::disabled_default()
    );

    let valid = crate::config::McpConfig {
        enabled: true,
        port: crate::config::MCP_DEFAULT_PORT + 1,
    };
    manager.set_mcp_config(valid).unwrap();
    assert_eq!(manager.get_config().mcp, valid);
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
        manager.get_config().subsonic,
        crate::config::SubsonicConfig::disabled_default()
    );

    let valid = crate::config::SubsonicConfig {
        enabled: true,
        port: crate::config::SUBSONIC_DEFAULT_PORT + 1,
        username: "listener".to_string(),
        bind_address: "0.0.0.0".to_string(),
    };
    manager.set_subsonic_config(valid.clone()).unwrap();
    assert_eq!(manager.get_config().subsonic, valid);
}
