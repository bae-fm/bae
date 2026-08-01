// Fixture row ids. coven validates every synced row's primary key as a
// canonical v4 UUID (`RowIdentity::IndependentUuid`), which is what bae's
// real ids are, so these fixtures carry UUIDs too. Each constant is named
// for the moniker it replaced, so assertions still read by name.
const COVER_1: &str = "2bc5ed84-97ea-4463-83cb-c206a428c802"; // was "cover-1"
const COVER_BLOB: &str = "4c98761d-e446-4871-8800-8ce69ac302ad"; // was "cover-blob"
const REL_1: &str = "cccb6034-5922-40d2-8d0b-d94619230882"; // was "e6cdc1f3-3a7b-473e-86aa-fe093cc5e94e"
const REL_2: &str = "dcdebf05-2d41-4dfc-8823-024993c9d00f"; // was "e6cdc0f3-3a7b-458b-86aa-fd093cc5e79b"
const REL_ABORT: &str = "59c56c5f-eee0-4a61-8ecc-fb828ab4c828"; // was "rel-abort"
const REL_NONE: &str = "cdb5eed8-db50-45f6-814a-dc623215ae04"; // was "rel-none"
const REL_X: &str = "e6b79143-6ab2-4358-8f53-460594936d69"; // was "rel-x"
const TRACK_1: &str = "f2f77437-aa03-4583-8b1c-d12bcf984967"; // was "track-1"
const TRACK_2: &str = "9ebc44e7-5513-4550-8d29-5d8919ec917b"; // was "track-2"
const TRACK_A: &str = "0482872e-d4bf-4080-8426-441a0a3e71fc"; // was "track-a"
const TRACK_BETA: &str = "094f4448-f13c-4284-83ea-e362fb2f38aa"; // was "track-beta"
const TRACK_WORK_A: &str = "d410a973-6a19-4ad3-87d8-b0c8c13d6015"; // was "track-work-a"
const WORK_A: &str = "432c8996-8af0-43dc-868a-822a256f65c4"; // was "work-a"

use super::track::playback_info_from_track_release;
use super::*;
use crate::config::Config;
use crate::db::{
    DbAlbum, DbAlbumArtist, DbFile, DbLibraryImage, DbRelease, DbTrackWork, DbWork,
    LibraryImageType,
};
use crate::import::MetadataSource;
#[cfg(feature = "test-utils")]
use crate::sync::CloudCipher;
use crate::util::content_type::ContentType;
use chrono::Utc;
#[cfg(feature = "test-utils")]
use coven::EncryptionService;
#[cfg(feature = "test-utils")]
use coven::InMemoryCloudHome;
use coven::StoreDir;
use tempfile::TempDir;
use uuid::Uuid;

async fn setup_test_manager() -> (LibraryManager, TempDir) {
    // A unique id per test so keyring entries don't collide in the shared
    // process-global mock store (see `install_test_keyring`).
    setup_test_manager_with_library_id(&format!("test-{}", Uuid::new_v4())).await
}

/// The keyring entries the mock keyring stores are namespaced by library id
/// (`<base>:<library_id>`) in one process-global store. Tests that read or
/// write keyring secrets (the Discogs key) must use a unique library id so
/// parallel tests don't clobber each other's entries.
async fn setup_test_manager_with_library_id(library_id: &str) -> (LibraryManager, TempDir) {
    let temp_dir = TempDir::new().unwrap();
    let db_path = temp_dir.path().join("test.db");
    let database = Database::new_test(
        db_path.to_str().unwrap(),
        Arc::new(coven::SystemClock),
        std::sync::Arc::new(coven::UuidProvider),
    )
    .await
    .unwrap();

    // Insert the test artist that create_test_album() references
    let artist = DbArtist {
        id: bae_test_support::test_uuid("e36744a5-1a36-460f-891c-e7e558034edf"),
        name: "Test Artist".to_string(),
        sort_name: None,
        discogs_artist_id: None,
        musicbrainz_artist_id: None,
        created_at: Utc::now(),
    };
    database.insert_artist(&artist).await.unwrap();

    let library_dir = StoreDir::new(temp_dir.path().to_path_buf());
    let config = Config::with_defaults(
        library_id.to_string(),
        "test-device".to_string(),
        library_dir.clone(),
        "Test Library".to_string(),
    );
    let config_handle = Arc::new(ConfigHandle::new(config));
    crate::config::install_test_keyring();
    let key_service = StoreKeys::bind(library_id.to_string());
    // `database`'s coven handle (opened via `Database::new_test` above)
    // establishes its own per-store identity under the fixed test store id
    // `new_test` always opens — see the note there.
    let manager = LibraryManager::new(
        database,
        config_handle,
        key_service,
        Arc::new(coven::SystemClock),
        Arc::new(coven::UuidProvider),
        crate::diagnostics::Diagnostics::noop(),
        tokio::runtime::Handle::current(),
    );
    (manager, temp_dir)
}

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
        .handle()
        .queued_deletes()
        .await
        .unwrap()
        .iter()
        .any(|delete| delete.namespace == namespace && delete.blob_id == blob_id)
}

/// Break one of bae's own tables so the next read or write against it fails,
/// standing in for a database that has gone bad under a delete. Only bae's tables
/// are renameable: coven's SQL authorizer refuses a host statement that alters one
/// of its reserved tables, so a cleanup step coven owns is failed by handing it
/// input it refuses instead (see the rollback tests below).
async fn rename_table_for_test(manager: &LibraryManager, from: &str, to: &str) {
    let statement = format!("ALTER TABLE {from} RENAME TO {to}");
    manager
        .database
        .handle()
        .sql(move |sql| {
            sql.execute(&statement, [])?;
            Ok::<(), coven::CovenError>(())
        })
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
    let config = Config::with_defaults(
        library_id.to_string(),
        "test-device".to_string(),
        StoreDir::new(library_dir.clone()),
        "Test Library".to_string(),
    );
    let config_handle = Arc::new(ConfigHandle::new(config));
    crate::config::install_test_keyring();
    let key_service = StoreKeys::bind(library_id.to_string());
    LibraryManager::new(
        database,
        config_handle,
        key_service,
        Arc::new(coven::SystemClock),
        Arc::new(coven::UuidProvider),
        crate::diagnostics::Diagnostics::noop(),
        tokio::runtime::Handle::current(),
    )
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
    manager.key_service.set_encryption_key("00").unwrap();

    let err = manager
        .forget_library()
        .expect_err("directory removal failure must surface");

    assert!(
        err.to_string().contains("Failed to remove library data"),
        "error should name the failed library data deletion: {err}"
    );
    assert_eq!(
        std::fs::read_to_string(bae_dir.join("active-library")).unwrap(),
        library_id
    );
    assert_eq!(
        manager.key_service.get_encryption_key().unwrap().as_deref(),
        Some("00")
    );
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
    manager.key_service.set_encryption_key("00").unwrap();

    manager.forget_library().unwrap();

    assert!(!library_path.exists());
    assert!(!bae_dir.join("active-library").exists());
    assert!(manager.key_service.get_encryption_key().unwrap().is_none());
}

#[tokio::test]
async fn forget_library_accepts_missing_directory_and_pointer_on_retry() {
    let library_id = format!("forget-retry-{}", Uuid::new_v4());
    let (home, library_path) = setup_forget_library_home(&library_id);
    let bae_dir = home.path().join(".bae");
    std::fs::create_dir_all(&bae_dir).unwrap();
    let manager = setup_forget_library_manager(&library_id, home.path()).await;
    manager.key_service.set_encryption_key("00").unwrap();

    manager.forget_library().unwrap();

    assert!(!library_path.exists());
    assert!(!bae_dir.join("active-library").exists());
    assert!(manager.key_service.get_encryption_key().unwrap().is_none());
}

#[tokio::test]
async fn forget_library_returns_error_when_active_pointer_cannot_be_read() {
    let library_id = format!("forget-pointer-fails-{}", Uuid::new_v4());
    let (home, library_path) = setup_forget_library_home(&library_id);
    let bae_dir = home.path().join(".bae");
    std::fs::create_dir_all(&library_path).unwrap();
    std::fs::create_dir(bae_dir.join("active-library")).unwrap();
    let manager = setup_forget_library_manager(&library_id, home.path()).await;
    manager.key_service.set_encryption_key("00").unwrap();

    let err = manager
        .forget_library()
        .expect_err("active pointer read failure must surface");

    assert!(
        err.to_string()
            .contains("Failed to read active-library pointer"),
        "error should name the failed active pointer read: {err}"
    );
    assert!(library_path.exists());
    assert!(bae_dir.join("active-library").is_dir());
    assert_eq!(
        manager.key_service.get_encryption_key().unwrap().as_deref(),
        Some("00")
    );
}

#[tokio::test]
async fn forget_library_returns_error_when_active_pointer_names_another_library() {
    let library_id = format!("forget-pointer-mismatch-{}", Uuid::new_v4());
    let (home, library_path) = setup_forget_library_home(&library_id);
    let bae_dir = home.path().join(".bae");
    std::fs::create_dir_all(&library_path).unwrap();
    std::fs::write(bae_dir.join("active-library"), "different-library").unwrap();
    let manager = setup_forget_library_manager(&library_id, home.path()).await;
    manager.key_service.set_encryption_key("00").unwrap();

    let err = manager
        .forget_library()
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
    assert_eq!(
        manager.key_service.get_encryption_key().unwrap().as_deref(),
        Some("00")
    );
}

#[tokio::test]
async fn forget_library_rejects_unregistered_library_dir() {
    let library_id = format!("forget-unregistered-{}", Uuid::new_v4());
    let home = TempDir::new().unwrap();
    let library_path = home.path().join("external-library");
    std::fs::create_dir_all(&library_path).unwrap();
    let manager =
        setup_forget_library_manager_at(&library_id, library_path.clone(), home.path()).await;
    manager.key_service.set_encryption_key("00").unwrap();

    let err = manager
        .forget_library()
        .expect_err("unregistered library directory must fail loudly");

    assert!(
        err.to_string().contains("does not match library id"),
        "error should name the unregistered library directory: {err}"
    );
    assert!(library_path.exists());
    assert_eq!(
        manager.key_service.get_encryption_key().unwrap().as_deref(),
        Some("00")
    );
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

fn create_test_album() -> DbAlbum {
    DbAlbum {
        id: Uuid::new_v4().to_string(),
        title: "Test Album".to_string(),
        artist_id: bae_test_support::test_uuid("e36744a5-1a36-460f-891c-e7e558034edf"),
        year: Some(2024),
        primary_release_id: None,
        is_compilation: false,
        created_at: Utc::now(),
    }
}

fn create_test_release(album_id: &str) -> DbRelease {
    DbRelease {
        id: Uuid::new_v4().to_string(),
        album_id: album_id.to_string(),
        release_name: None,
        pressing: Pressing {
            year: Some(2024),
            format: None,
            label: None,
            catalog_number: None,
            country: None,
            barcode: None,
        },
        disc_id: None,
        metadata_source: crate::db::ReleaseMetadataSource::FileTags,
        metadata_source_release_id: None,
        remote: true,
        source_folder_name: None,
        content_hash: None,
        album_loudness_lufs: None,
        album_peak_linear: None,
        created_at: Utc::now(),
    }
}

#[tokio::test]
async fn work_detail_release_rows_are_display_ready() {
    let (manager, _temp_dir) = setup_test_manager().await;
    let album = create_test_album();
    let mut release = create_test_release(&album.id);
    release.pressing.format = Some("CD".to_string());
    let track = crate::db::DbTrack::new_test(&release.id, TRACK_A, "Track Title", Some(1));
    let now = Utc::now();
    let work = DbWork::new(WORK_A, "Work Title", None, Some("work".to_string()), now);
    let track_work = DbTrackWork::new(
        &track.id,
        &work.id,
        0,
        MetadataSource::MusicBrainz,
        TRACK_WORK_A.to_string(),
        now,
    );
    let cover = DbLibraryImage {
        id: release.id.clone(),
        blob_id: format!("{}-cover-blob", release.id),
        image_type: LibraryImageType::Cover,
        content_type: ContentType::Jpeg,
        file_size: 100,
        width: None,
        height: None,
        source: "local".to_string(),
        source_url: None,
        cloud_path: None,
        content_hash: crate::util::fs::hash_bytes(b"fixture"),
        created_at: now,
    };

    manager.database.insert_album(&album).await.unwrap();
    manager.database.insert_release(&release).await.unwrap();
    manager.database.insert_track(&track).await.unwrap();
    manager
        .database
        .insert_composition_fixture_rows(std::slice::from_ref(&work), &[track_work], &[cover])
        .await
        .unwrap();

    let detail = manager
        .get_work_detail(&work.id)
        .await
        .unwrap()
        .expect("work detail");

    assert_eq!(detail.releases.len(), 1);
    let row = &detail.releases[0];
    assert_eq!(row.release_id, release.id);
    assert_eq!(row.album_id, album.id);
    assert_eq!(row.album_title, album.title);
    assert_eq!(row.display_name, "2024 CD");
    assert_eq!(row.format.as_deref(), Some("CD"));
    let cover = row.cover.as_ref().expect("work release cover");
    assert_eq!(cover.id, release.id);
    assert!(!cover.version.is_empty());
    assert_eq!(cover.image_type, crate::db::LibraryImageType::Cover);
}

#[tokio::test]
async fn test_delete_release_with_single_release_deletes_album() {
    let (manager, _temp_dir) = setup_test_manager().await;
    let album = create_test_album();
    let release = create_test_release(&album.id);

    manager.database.insert_album(&album).await.unwrap();
    manager.database.insert_release(&release).await.unwrap();

    manager.delete_release(&release.id).await.unwrap();

    let album_result = manager.database.find_album_by_id(&album.id).await.unwrap();
    assert!(album_result.is_none());
    let releases = manager
        .database
        .get_releases_for_album(&album.id)
        .await
        .unwrap();
    assert!(releases.is_empty());
}

#[tokio::test]
async fn test_delete_release_with_multiple_releases_preserves_album() {
    let (manager, _temp_dir) = setup_test_manager().await;
    let album = create_test_album();
    let release1 = create_test_release(&album.id);
    let release2 = create_test_release(&album.id);

    manager.database.insert_album(&album).await.unwrap();
    manager.database.insert_release(&release1).await.unwrap();
    manager.database.insert_release(&release2).await.unwrap();

    manager.delete_release(&release1.id).await.unwrap();

    let album_result = manager.database.find_album_by_id(&album.id).await.unwrap();
    assert!(album_result.is_some());
    let releases = manager
        .database
        .get_releases_for_album(&album.id)
        .await
        .unwrap();
    assert_eq!(releases.len(), 1);
    assert_eq!(releases[0].id, release2.id);
}

#[tokio::test]
async fn delete_releases_with_content_hash_removes_only_matching() {
    let (manager, _temp_dir) = setup_test_manager().await;
    let album = create_test_album();
    let mut matching1 = create_test_release(&album.id);
    matching1.content_hash = Some("hash-shared".to_string());
    let mut matching2 = create_test_release(&album.id);
    matching2.content_hash = Some("hash-shared".to_string());
    let mut other = create_test_release(&album.id);
    other.content_hash = Some("hash-other".to_string());

    manager.database.insert_album(&album).await.unwrap();
    manager.database.insert_release(&matching1).await.unwrap();
    manager.database.insert_release(&matching2).await.unwrap();
    manager.database.insert_release(&other).await.unwrap();

    manager
        .delete_releases_with_content_hash("hash-shared")
        .await
        .unwrap();

    // Both releases carrying the re-imported folder's hash are gone; the
    // unrelated release survives. This is the overwrite the import worker
    // performs before inserting a re-import of the same folder tree.
    let remaining = manager
        .database
        .get_releases_for_album(&album.id)
        .await
        .unwrap();
    assert_eq!(remaining.len(), 1);
    assert_eq!(remaining[0].id, other.id);
}

/// Deleting one release of a multi-release album must tombstone its remote
/// cloud blobs — delete_release has to queue the cloud-outbox deletes like
/// delete_album/unmanage, or the remote blobs leak in the cloud (nothing else
/// processes the release once its rows are gone).
#[cfg(feature = "test-utils")]
#[tokio::test]
async fn delete_release_tombstones_remote_cloud_blobs() {
    let (manager, temp_dir) = setup_test_manager().await;
    connect_test_cloud(&manager).await;

    // A release whose one file really did reach the cloud — the tombstone is
    // owed to a cloud object that exists, which is the whole point.
    let release1 =
        make_remote_release(&manager, &temp_dir.path().join("r1"), "Album One", false).await;
    let file_id = manager
        .database
        .get_files_for_release(&release1)
        .await
        .unwrap()[0]
        .id
        .clone();
    // A sibling release in the same album, so delete_release takes the
    // album-survives branch.
    let album_id = manager
        .database
        .find_release_by_id(&release1)
        .await
        .unwrap()
        .expect("the release exists")
        .album_id;
    let mut release2 = create_test_release(&album_id);
    release2.remote = false;
    manager.database.insert_release(&release2).await.unwrap();

    manager.delete_release(&release1).await.unwrap();

    // delete_release awaits the deletion queueing, so by now the cloud object's
    // tombstone is enqueued.
    assert!(
        has_queued_delete(&manager, crate::sync::RELEASE_FILES_NAMESPACE, &file_id).await,
        "deleting a remote release tombstones its cloud blob"
    );
}

#[cfg(feature = "test-utils")]
#[tokio::test]
async fn delete_release_cancels_in_flight_make_remote() {
    let (manager, temp_dir) = setup_test_manager().await;
    let home = connect_test_cloud(&manager).await;
    let release = insert_partially_uploaded_make_remote_release(&manager, temp_dir.path()).await;
    let deleted_before = home.exact_delete_count();

    manager.delete_release(&release.id).await.unwrap();

    assert!(manager
        .database
        .find_release_by_id(&release.id)
        .await
        .unwrap()
        .is_none());
    assert!(
        manager
            .database
            .handle()
            .queued_uploads()
            .await
            .unwrap()
            .is_empty(),
        "deleting the release cancels unresolved make-Remote uploads"
    );
    assert!(manager
        .database
        .make_remote_progress_for_release(&release.id)
        .await
        .unwrap()
        .is_none());
    // The object that already reached the cloud is removed outright, not left
    // as a queued tombstone: the make-Remote never published, so nothing else
    // can reference it and the cancel's own unwind deletes it.
    assert!(
        home.exact_delete_count() > deleted_before,
        "the uploaded object is deleted from the cloud"
    );
    assert!(
        manager
            .database
            .handle()
            .queued_deletes()
            .await
            .unwrap()
            .is_empty(),
        "an unpublished object needs no tombstone"
    );
}

#[cfg(feature = "test-utils")]
#[tokio::test]
async fn delete_album_cancels_in_flight_make_remote() {
    let (manager, temp_dir) = setup_test_manager().await;
    let home = connect_test_cloud(&manager).await;
    let release = insert_partially_uploaded_make_remote_release(&manager, temp_dir.path()).await;
    let deleted_before = home.exact_delete_count();

    manager.delete_album(&release.album_id).await.unwrap();

    assert!(manager
        .database
        .find_release_by_id(&release.id)
        .await
        .unwrap()
        .is_none());
    assert!(
        manager
            .database
            .handle()
            .queued_uploads()
            .await
            .unwrap()
            .is_empty(),
        "deleting the album cancels unresolved make-Remote uploads"
    );
    assert!(manager
        .database
        .make_remote_progress_for_release(&release.id)
        .await
        .unwrap()
        .is_none());
    // The object that already reached the cloud is removed outright, not left
    // as a queued tombstone: the make-Remote never published, so nothing else
    // can reference it and the cancel's own unwind deletes it.
    assert!(
        home.exact_delete_count() > deleted_before,
        "the uploaded object is deleted from the cloud"
    );
    assert!(
        manager
            .database
            .handle()
            .queued_deletes()
            .await
            .unwrap()
            .is_empty(),
        "an unpublished object needs no tombstone"
    );
}

#[tokio::test]
async fn delete_release_fails_before_rows_are_deleted_when_file_cleanup_lookup_fails() {
    let (manager, _temp_dir) = setup_test_manager().await;
    let album = create_test_album();
    let release = create_test_release(&album.id);
    manager.database.insert_album(&album).await.unwrap();
    manager.database.insert_release(&release).await.unwrap();

    let file = DbFile::new(
        &release.id,
        "track1.flac",
        5,
        crate::util::content_type::ContentType::Flac,
        Uuid::new_v4().to_string(),
        Utc::now(),
        crate::util::fs::hash_bytes(b"fixture"),
    );
    manager.add_file(&file).await.unwrap();

    rename_table_for_test(&manager, "release_files", "release_files_unavailable").await;

    let error = manager.delete_release(&release.id).await.unwrap_err();
    assert!(matches!(error, LibraryError::Database(_)));
    assert!(manager
        .database
        .find_release_by_id(&release.id)
        .await
        .unwrap()
        .is_some());
}

/// The row deletes and the blob cleanup share one transaction, so a cleanup step
/// coven refuses takes the row deletes down with it — the release survives rather
/// than leaving the library short a release whose blob bookkeeping still stands.
///
/// Clearing an external registration names the blob table it belongs to, and
/// coven refuses a name that declares no blob. That refusal lands mid-transaction,
/// after the deletes have been staged, which is the point.
#[tokio::test]
async fn delete_release_rolls_back_when_an_external_ref_clear_is_refused() {
    let (manager, _temp_dir) = setup_test_manager().await;
    let album = create_test_album();
    let release = create_test_release(&album.id);
    manager.database.insert_album(&album).await.unwrap();
    manager.database.insert_release(&release).await.unwrap();

    let file = DbFile::new(
        &release.id,
        "track1.flac",
        5,
        crate::util::content_type::ContentType::Flac,
        Uuid::new_v4().to_string(),
        Utc::now(),
        crate::util::fs::hash_bytes(b"fixture"),
    );
    manager.add_file(&file).await.unwrap();

    manager
        .database
        .delete_release_with_cleanup(
            &release.id,
            &album.id,
            DeleteCleanupPlan {
                blobs_to_tombstone: Vec::new(),
                external_refs_to_clear: vec![("no_such_blob_table".to_string(), file.id.clone())],
            },
        )
        .await
        .expect_err("clearing a ref on an undeclared blob table is refused");

    assert!(manager
        .database
        .find_release_by_id(&release.id)
        .await
        .unwrap()
        .is_some());
}

/// The tombstone half of the rollback above. A blob with no committed cloud
/// object has nothing to remove and coven refuses to queue a tombstone for it, so
/// a cover that never reached the cloud is a cleanup step that fails inside the
/// delete transaction.
#[tokio::test]
async fn delete_release_rolls_back_when_a_blob_tombstone_is_refused() {
    let (manager, _temp_dir) = setup_test_manager().await;
    let album = create_test_album();
    let release = create_test_release(&album.id);
    manager.database.insert_album(&album).await.unwrap();
    manager.database.insert_release(&release).await.unwrap();
    store_test_cover_image(&manager, &release.id).await;

    let cover_blob = manager
        .handle()
        .row_blob_ref(crate::sync::COVERS_NAMESPACE, &release.id)
        .await
        .unwrap();
    assert!(
        cover_blob.stored().is_none(),
        "no provider is connected, so the cover reached no cloud object"
    );

    manager
        .database
        .delete_release_with_cleanup(
            &release.id,
            &album.id,
            DeleteCleanupPlan {
                blobs_to_tombstone: vec![cover_blob],
                external_refs_to_clear: Vec::new(),
            },
        )
        .await
        .expect_err("tombstoning a blob with no cloud object is refused");

    assert!(manager
        .database
        .find_release_by_id(&release.id)
        .await
        .unwrap()
        .is_some());
}

#[tokio::test]
async fn delete_album_fails_before_rows_are_deleted_when_track_lookup_fails() {
    let (manager, _temp_dir) = setup_test_manager().await;
    let album = create_test_album();
    let release = create_test_release(&album.id);
    manager.database.insert_album(&album).await.unwrap();
    manager.database.insert_release(&release).await.unwrap();

    rename_table_for_test(&manager, "tracks", "tracks_unavailable").await;

    let error = manager.delete_album(&album.id).await.unwrap_err();
    assert!(matches!(error, LibraryError::Database(_)));
    assert!(manager
        .database
        .find_album_by_id(&album.id)
        .await
        .unwrap()
        .is_some());
}

#[tokio::test]
async fn delete_album_fails_before_rows_are_deleted_when_file_cleanup_lookup_fails() {
    let (manager, _temp_dir) = setup_test_manager().await;
    let album = create_test_album();
    let release = create_test_release(&album.id);
    manager.database.insert_album(&album).await.unwrap();
    manager.database.insert_release(&release).await.unwrap();

    rename_table_for_test(&manager, "release_files", "release_files_unavailable").await;

    let error = manager.delete_album(&album.id).await.unwrap_err();
    assert!(matches!(error, LibraryError::Database(_)));
    assert!(manager
        .database
        .find_album_by_id(&album.id)
        .await
        .unwrap()
        .is_some());
}

/// The playing track's cover reference is what the UI caches its art under, so
/// it has to carry the `covers` row's version, not just the release id — and it
/// has to be absent when the release has no cover at all, rather than naming a
/// row that isn't there.
#[cfg(feature = "test-utils")]
#[tokio::test]
async fn playback_track_info_carries_the_cover_version() {
    let (manager, _temp_dir) = setup_test_manager().await;
    let album = create_test_album();
    let release = create_test_release(&album.id);
    let track = crate::db::DbTrack::new_test(&release.id, TRACK_A, "Track Title", Some(1));
    let track_artist = crate::db::DbTrackArtist::new(
        &track.id,
        &bae_test_support::test_uuid("e36744a5-1a36-460f-891c-e7e558034edf"),
        0,
        Uuid::new_v4().to_string(),
        Utc::now(),
    );
    manager.database.insert_album(&album).await.unwrap();
    manager.database.insert_release(&release).await.unwrap();
    manager.database.insert_track(&track).await.unwrap();
    manager
        .database
        .insert_track_artist(&track_artist)
        .await
        .unwrap();

    // No cover row yet: nothing to reference.
    let info = manager.get_playback_track_info(&track.id).await.unwrap();
    assert_eq!(info.cover_image, None);

    store_test_cover_image(&manager, &release.id).await;
    let version = manager
        .database
        .cover_version(&release.id)
        .await
        .unwrap()
        .expect("the stored cover has a version");
    let info = manager.get_playback_track_info(&track.id).await.unwrap();
    assert_eq!(
        info.cover_image,
        Some(crate::album_detail::ImageRef {
            id: release.id.clone(),
            version,
            image_type: LibraryImageType::Cover,
        })
    );
}

#[tokio::test]
async fn playback_info_from_track_release_rejects_missing_album() {
    let (manager, _temp_dir) = setup_test_manager().await;
    let album = create_test_album();
    let release = create_test_release(&album.id);
    let track = crate::db::DbTrack::new_test(&release.id, TRACK_A, "Track Title", Some(1));
    let track_artist = crate::db::DbTrackArtist::new(
        &track.id,
        &bae_test_support::test_uuid("e36744a5-1a36-460f-891c-e7e558034edf"),
        0,
        Uuid::new_v4().to_string(),
        Utc::now(),
    );
    manager.database.insert_album(&album).await.unwrap();
    manager.database.insert_release(&release).await.unwrap();
    manager.database.insert_track(&track).await.unwrap();
    manager
        .database
        .insert_track_artist(&track_artist)
        .await
        .unwrap();

    let mut broken_release = release.clone();
    broken_release.album_id = "missing-album".to_string();

    let error = playback_info_from_track_release(&manager.database, &track, &broken_release)
        .await
        .unwrap_err();
    assert!(
        matches!(error, LibraryError::TrackMapping(message) if message.contains("missing-album"))
    );
}

#[tokio::test]
async fn delete_release_fails_before_rows_are_deleted_when_cover_lookup_fails() {
    let (manager, _temp_dir) = setup_test_manager().await;
    let album = create_test_album();
    let release = create_test_release(&album.id);
    manager.database.insert_album(&album).await.unwrap();
    manager.database.insert_release(&release).await.unwrap();
    store_test_cover_image(&manager, &release.id).await;

    rename_table_for_test(&manager, "covers", "covers_unavailable").await;

    let error = manager.delete_release(&release.id).await.unwrap_err();
    assert!(matches!(error, LibraryError::Database(_)));
    assert!(manager
        .database
        .find_release_by_id(&release.id)
        .await
        .unwrap()
        .is_some());
}

/// Deleting a release cascade-deletes its `covers` row (the FK on `covers.id`
/// to `releases`), and the delete path cleans up the cover blob: a Remote
/// release's cover is tombstoned in the cloud and dropped from the cache.
#[cfg(feature = "test-utils")]
#[tokio::test]
async fn delete_release_removes_its_cover_image() {
    let (manager, temp_dir) = setup_test_manager().await;
    connect_test_cloud_with_sync_loop(&manager).await;

    // A genuinely Remote release, so storing its cover publishes the blob and
    // there is a real cloud object for the delete to tombstone.
    let release1 = ReleaseRef::of(
        &manager,
        make_remote_release_under_sync_loop(
            &manager,
            &temp_dir.path().join("r1"),
            "Album One",
            false,
        )
        .await,
    )
    .await;
    // A sibling release so the album survives the single-release delete.
    let mut release2 = create_test_release(&release1.album_id);
    release2.remote = false;
    manager.database.insert_release(&release2).await.unwrap();

    // Give release1 a cover: a `covers` row plus its blob in one coven batch.
    manager
        .store_library_image_blob(
            &crate::db::DbLibraryImage {
                id: release1.id.clone(),
                blob_id: bae_test_support::test_uuid(&format!("{}-cover-blob", release1.id)),
                image_type: LibraryImageType::Cover,
                content_type: crate::util::content_type::ContentType::Jpeg,
                file_size: 5,
                width: None,
                height: None,
                source: "local".to_string(),
                source_url: None,
                cloud_path: None,
                content_hash: crate::util::fs::hash_bytes(b"image"),
                created_at: manager.clock.now(),
            },
            b"image",
        )
        .await
        .unwrap();

    wait_for_published_blob(&manager, crate::sync::COVERS_NAMESPACE, &release1.id).await;

    manager.delete_release(&release1.id).await.unwrap();

    // Row removed (the `covers` FK to `releases` cascade-deletes it).
    assert!(manager
        .get_library_image(&release1.id, &LibraryImageType::Cover)
        .await
        .unwrap()
        .is_none());

    // The cover blob's cloud object is tombstoned, named by the namespace and
    // blob id coven queues it under.
    assert!(
        has_queued_delete(
            &manager,
            crate::sync::COVERS_NAMESPACE,
            &bae_test_support::test_uuid(&format!("{}-cover-blob", release1.id)),
        )
        .await,
        "cover blob delete must be enqueued"
    );
}

/// delete_album removes each release's cover too (same helper, second wiring
/// site): the cover row is gone and its blob delete is enqueued.
#[cfg(feature = "test-utils")]
#[tokio::test]
async fn delete_album_removes_release_covers() {
    let (manager, temp_dir) = setup_test_manager().await;
    connect_test_cloud_with_sync_loop(&manager).await;
    let release = ReleaseRef::of(
        &manager,
        make_remote_release_under_sync_loop(
            &manager,
            &temp_dir.path().join("r1"),
            "Album One",
            false,
        )
        .await,
    )
    .await;
    manager
        .store_library_image_blob(
            &crate::db::DbLibraryImage {
                id: release.id.clone(),
                blob_id: bae_test_support::test_uuid(&format!("{}-cover-blob", release.id)),
                image_type: LibraryImageType::Cover,
                content_type: crate::util::content_type::ContentType::Jpeg,
                file_size: 5,
                width: None,
                height: None,
                source: "local".to_string(),
                source_url: None,
                cloud_path: None,
                content_hash: crate::util::fs::hash_bytes(b"image"),
                created_at: manager.clock.now(),
            },
            b"image",
        )
        .await
        .unwrap();

    wait_for_published_blob(&manager, crate::sync::COVERS_NAMESPACE, &release.id).await;

    manager.delete_album(&release.album_id).await.unwrap();

    assert!(manager
        .get_library_image(&release.id, &LibraryImageType::Cover)
        .await
        .unwrap()
        .is_none());
    assert!(
        has_queued_delete(
            &manager,
            crate::sync::COVERS_NAMESPACE,
            &bae_test_support::test_uuid(&format!("{}-cover-blob", release.id)),
        )
        .await
    );
}

/// Drain the manager's library event channel and return the first
/// `ReleaseRemoved` seen, failing if the channel closes first.
async fn next_release_removed(
    rx: &mut broadcast::Receiver<LibraryEvent>,
) -> (String, String, Option<AlbumSummary>) {
    loop {
        match rx.recv().await {
            Ok(LibraryEvent::ReleaseRemoved {
                album_id,
                release_id,
                album,
            }) => return (album_id, release_id, album),
            Ok(_) => continue,
            Err(e) => panic!("library event channel closed before ReleaseRemoved: {e}"),
        }
    }
}

#[tokio::test]
async fn release_removed_carries_post_removal_parent_album() {
    let (manager, _temp_dir) = setup_test_manager().await;
    let album = create_test_album();
    let release1 = create_test_release(&album.id);
    let release2 = create_test_release(&album.id);

    manager.database.insert_album(&album).await.unwrap();
    manager.database.insert_release(&release1).await.unwrap();
    manager.database.insert_release(&release2).await.unwrap();

    let mut rx = manager.subscribe_events();
    manager.delete_release(&release1.id).await.unwrap();

    let (album_id, release_id, summary) = next_release_removed(&mut rx).await;
    assert_eq!(album_id, album.id);
    assert_eq!(release_id, release1.id);
    let summary = summary.expect("album survives, so the event carries its summary");
    assert!(
        !summary.release_ids.contains(&release1.id),
        "deleted release must be gone from the post-removal summary"
    );
    assert!(
        summary.release_ids.contains(&release2.id),
        "surviving release must remain in the post-removal summary"
    );
}

#[tokio::test]
async fn release_removed_carries_none_when_album_no_longer_exists() {
    // The album-cascade case: the sync path calls `emit_release_removed` for a
    // release whose album was already removed with its last release, while
    // `delete_release` takes the album-removed branch instead. So drive
    // `emit_release_removed` against a missing album directly — the same call the
    // sync path makes — and assert it ships `album: None` instead of panicking.
    let (manager, _temp_dir) = setup_test_manager().await;

    let mut rx = manager.subscribe_events();
    manager
        .emit_release_removed("gone-album-id", "gone-release-id")
        .await;

    let (album_id, release_id, summary) = next_release_removed(&mut rx).await;
    assert_eq!(album_id, "gone-album-id");
    assert_eq!(release_id, "gone-release-id");
    assert!(
        summary.is_none(),
        "a removed album yields no summary, not a panic in AlbumSummary::from_raw"
    );
}

#[tokio::test]
async fn test_delete_album_deletes_all_releases() {
    let (manager, _temp_dir) = setup_test_manager().await;
    let album = create_test_album();
    let release1 = create_test_release(&album.id);
    let release2 = create_test_release(&album.id);

    manager.database.insert_album(&album).await.unwrap();
    manager.database.insert_release(&release1).await.unwrap();
    manager.database.insert_release(&release2).await.unwrap();

    manager.delete_album(&album.id).await.unwrap();

    let album_result = manager.database.find_album_by_id(&album.id).await.unwrap();
    assert!(album_result.is_none());
    let releases = manager
        .database
        .get_releases_for_album(&album.id)
        .await
        .unwrap();
    assert!(releases.is_empty());
}

/// Insert (or overwrite) a release's `covers` row. The cover reference reads
/// the row (not the bytes), and each upsert stamps a fresh `_updated_at`, so
/// re-calling this moves the cover's version — what `change_cover` does when it
/// replaces a cover in place.
async fn add_cover_row(manager: &LibraryManager, release_id: &str) {
    manager
        .upsert_library_image(&crate::db::DbLibraryImage {
            id: release_id.to_string(),
            blob_id: format!("{release_id}-cover-blob"),
            image_type: LibraryImageType::Cover,
            content_type: crate::util::content_type::ContentType::Jpeg,
            file_size: 5,
            width: None,
            height: None,
            source: "local".to_string(),
            source_url: None,
            cloud_path: None,
            content_hash: crate::util::fs::hash_bytes(b"fixture"),
            created_at: manager.clock.now(),
        })
        .await
        .unwrap();
}

#[tokio::test]
async fn release_detail_has_no_cover_without_a_cover_row() {
    let (manager, _temp_dir) = setup_test_manager().await;
    let album = create_test_album();
    let release = create_test_release(&album.id);
    manager.database.insert_album(&album).await.unwrap();
    manager.database.insert_release(&release).await.unwrap();
    let detail = manager
        .find_release_detail(&release.id)
        .await
        .unwrap()
        .expect("detail present for known release");
    assert!(detail.summary.cover.is_none());
}

/// A peer can ship a `release_files` row whose `original_filename` is a
/// path-traversal token. Primary keys can no longer carry one — coven validates
/// every synced-table id, on a local write and on an incoming changeset alike —
/// but the path *fragments* on a row are ordinary text it never inspects, and a
/// synced row makes such a value durable. So the display resolver that fires on
/// every sync cycle must treat it as a missing asset rather than panic: a panic
/// here crash-loops every device, every cycle.
#[cfg(feature = "test-utils")]
#[tokio::test]
async fn find_release_detail_does_not_panic_on_traversal_filenames_from_a_peer() {
    let (manager, _temp_dir) = setup_test_manager().await;

    let album = create_test_album();
    let mut release = create_test_release(&album.id);
    release.remote = false;
    manager.database.insert_album(&album).await.unwrap();
    manager.database.insert_release(&release).await.unwrap();

    // An image file whose stored filename is a traversal token. bae's own write
    // path validates the fragment and refuses it, which is why the value has to
    // be written straight onto the row the way coven applies a peer's changeset
    // — the seam `set_original_filename_for_test` exists for. It drives the
    // gallery's blob resolution and, as the release's representative blob, the
    // pinned-cache check; both must reject the bad fragment, not panic.
    let file = DbFile::new(
        &release.id,
        "cover.jpg",
        5,
        crate::util::content_type::ContentType::Jpeg,
        Uuid::new_v4().to_string(),
        Utc::now(),
        crate::util::fs::hash_bytes(b"fixture"),
    );
    manager.add_file(&file).await.unwrap();
    manager
        .database
        .set_original_filename_for_test(&file.id, "../../etc/y")
        .await
        .unwrap();

    // The resolver that fires when a synced release surfaces in the UI. The
    // bad fragment must resolve to "no cover" / "no local gallery path", not a
    // panic.
    let detail = manager
        .find_release_detail(&release.id)
        .await
        .expect("resolving a release with a traversal filename must not error")
        .expect("the inserted release must resolve to a detail");
    assert!(
        detail.summary.cover.is_none(),
        "there is no cover row, so no cover"
    );
    assert!(
        detail.gallery_items.iter().all(|item| matches!(
            item.source,
            crate::album_detail::GallerySource::ReleaseFile { .. }
        )),
        "with no cover row there is no cover slot; the traversal image file is a \
             release-file item, read by id"
    );
}

/// A release's cover reference carries the `covers` row's `_updated_at` as its
/// version, and overwriting the cover (re-upserting the row) moves that
/// version — the changed field that fires the UI's per-field re-render and
/// reloads the cover.
#[tokio::test]
async fn release_cover_version_moves_when_the_cover_row_is_reupserted() {
    let (manager, _temp_dir) = setup_test_manager().await;
    let album = create_test_album();
    let release = create_test_release(&album.id);
    manager.database.insert_album(&album).await.unwrap();
    manager.database.insert_release(&release).await.unwrap();

    add_cover_row(&manager, &release.id).await;
    let before = manager
        .find_release_detail(&release.id)
        .await
        .unwrap()
        .unwrap()
        .summary
        .cover
        .expect("cover reference present once the row exists");
    assert_eq!(before.id, release.id, "the cover id is the release id");

    // Overwrite (what change_cover does): same row, fresh `_updated_at`.
    add_cover_row(&manager, &release.id).await;
    let after = manager
        .find_release_detail(&release.id)
        .await
        .unwrap()
        .unwrap()
        .summary
        .cover
        .expect("cover reference present after overwrite");
    assert_ne!(
        before.version, after.version,
        "overwriting the cover must move its version"
    );
}

/// Each storage-page row carries its own release's cover reference, not the
/// album's primary-release cover. Two releases of one album, each with its own
/// `covers` row, resolve to their own ids; a non-primary release's row resolves
/// to its own cover rather than the album's primary.
#[tokio::test]
async fn storage_page_rows_carry_each_releases_own_cover() {
    let (manager, _temp_dir) = setup_test_manager().await;
    let album = create_test_album();
    let release1 = create_test_release(&album.id);
    let release2 = create_test_release(&album.id);
    manager.database.insert_album(&album).await.unwrap();
    manager.database.insert_release(&release1).await.unwrap();
    manager.database.insert_release(&release2).await.unwrap();
    // release1 is the album's primary, so its cover is the album-level cover.
    manager
        .set_album_primary_release(&album.id, &release1.id)
        .await
        .unwrap();

    add_cover_row(&manager, &release1.id).await;
    add_cover_row(&manager, &release2.id).await;

    let page = manager
        .get_storage_page(
            &crate::db::StorageSortCriterion {
                field: crate::db::StorageSortField::AlbumTitle,
                direction: crate::db::SortDirection::Ascending,
            },
            crate::db::StorageFilter::All,
            0,
            100,
        )
        .await
        .unwrap();

    let row1 = page
        .rows
        .iter()
        .find(|r| r.release.id == release1.id)
        .expect("release1 row present");
    let row2 = page
        .rows
        .iter()
        .find(|r| r.release.id == release2.id)
        .expect("release2 row present");

    // Each release row resolves to that release's own cover.
    assert_eq!(row1.release.cover.as_ref().unwrap().id, release1.id);
    assert_eq!(row2.release.cover.as_ref().unwrap().id, release2.id);

    // The album carries the primary release's cover; the non-primary release's
    // row carries its own, distinct from the album's.
    assert_eq!(row2.album.cover.as_ref().unwrap().id, release1.id);
    assert_ne!(
        row2.release.cover.as_ref().unwrap().id,
        row2.album.cover.as_ref().unwrap().id
    );
}

#[tokio::test]
async fn album_detail_cover_is_versioned_and_moves_on_overwrite() {
    let (manager, _temp_dir) = setup_test_manager().await;
    let album = create_test_album();
    let release = create_test_release(&album.id);
    manager.database.insert_album(&album).await.unwrap();
    manager.database.insert_release(&release).await.unwrap();

    // No cover row yet: the detail carries no cover reference.
    let detail = manager
        .find_album_detail(&album.id)
        .await
        .unwrap()
        .expect("detail present for known album");
    assert!(detail.cover.is_none());

    add_cover_row(&manager, &release.id).await;
    let before = manager
        .find_album_detail(&album.id)
        .await
        .unwrap()
        .unwrap()
        .cover
        .expect("cover reference present once the row exists");
    assert_eq!(before.id, release.id);

    // Overwrite the cover (what change_cover does): fresh `_updated_at`.
    add_cover_row(&manager, &release.id).await;
    let after = manager
        .find_album_detail(&album.id)
        .await
        .unwrap()
        .unwrap()
        .cover
        .expect("cover reference present after overwrite");

    // The summary the UI re-renders against carries a changed version, which
    // fires the per-field re-render and reloads the cover.
    assert_ne!(before.version, after.version);
}

#[tokio::test]
async fn find_album_detail_returns_none_when_releases_vanish() {
    let (manager, _temp_dir) = setup_test_manager().await;
    let album = create_test_album();
    manager.database.insert_album(&album).await.unwrap();

    let detail = manager
        .find_album_detail(&album.id)
        .await
        .expect("empty album aggregate must resolve without an error");

    assert!(detail.is_none());
}

#[tokio::test]
async fn resolve_album_detail_errors_when_releases_vanish() {
    let (manager, _temp_dir) = setup_test_manager().await;
    let album = create_test_album();

    let err = manager
        .resolve_album_detail(crate::db::DbAlbumDetail {
            album,
            artists: Vec::new(),
            releases: Vec::new(),
        })
        .await
        .expect_err("empty album detail must return an error");

    assert!(
        matches!(&err, LibraryError::TrackMapping(message) if message.contains("has no releases")),
        "empty album detail error should name the missing releases: {err}"
    );
}

#[tokio::test]
async fn find_release_detail_returns_some_for_known_id() {
    let (manager, _temp_dir) = setup_test_manager().await;
    let album = create_test_album();
    let mut release = create_test_release(&album.id);
    release.pressing.year = None;
    release.pressing.format = None;
    release.release_name = None;
    manager.database.insert_album(&album).await.unwrap();
    manager.database.insert_release(&release).await.unwrap();

    let detail = manager
        .find_release_detail(&release.id)
        .await
        .unwrap()
        .expect("detail present for known release");
    assert_eq!(detail.summary.id, release.id);
    assert_eq!(detail.summary.album_id, album.id);
    // Only release in album, no year/format/release_name → "Release 1".
    assert_eq!(detail.display_name, "Release 1");
    assert!(detail.tracks.is_empty());
    assert!(detail.files.is_empty());
}

#[tokio::test]
async fn find_release_detail_surfaces_seeded_tracks() {
    let (manager, _temp_dir) = setup_test_manager().await;
    let album = create_test_album();
    let release = create_test_release(&album.id);
    manager.database.insert_album(&album).await.unwrap();
    manager.database.insert_release(&release).await.unwrap();

    // Seed two tracks; the detail resolver must surface both with their
    // titles and track numbers (not just report emptiness).
    let t1 = crate::db::DbTrack::new_test(&release.id, TRACK_1, "Opening", Some(1));
    let t2 = crate::db::DbTrack::new_test(&release.id, TRACK_2, "Closing", Some(2));
    manager.database.insert_track(&t1).await.unwrap();
    manager.database.insert_track(&t2).await.unwrap();

    let detail = manager
        .find_release_detail(&release.id)
        .await
        .unwrap()
        .expect("detail present");
    assert_eq!(detail.tracks.len(), 2);
    let opening = detail
        .tracks
        .iter()
        .find(|t| t.title == "Opening")
        .expect("opening track surfaced");
    assert_eq!(opening.track_number, Some(1));
    assert!(
        detail.tracks.iter().any(|t| t.title == "Closing"),
        "closing track surfaced"
    );
}

/// A compilation's rows each carry their own artist (the header names none); a
/// single-artist album's rows carry no display artist (they would only repeat
/// the header). Core decides this, so the four front-ends stop rendering it four
/// ways.
#[tokio::test]
async fn display_artist_is_set_only_for_a_compilation() {
    async fn resolve_display_artist(is_compilation: bool) -> Option<String> {
        let (manager, _temp_dir) = setup_test_manager().await;
        let mut album = create_test_album();
        album.is_compilation = is_compilation;
        let release = create_test_release(&album.id);
        let track = crate::db::DbTrack::new_test(&release.id, TRACK_A, "Track Title", Some(1));
        let guest = DbArtist {
            id: "75c512c4-41b6-438d-89a6-d5929fa0697d".to_string(),
            name: "Guest Performer".to_string(),
            sort_name: None,
            discogs_artist_id: None,
            musicbrainz_artist_id: None,
            created_at: Utc::now(),
        };
        let track_artist = crate::db::DbTrackArtist::new(
            &track.id,
            &guest.id,
            0,
            Uuid::new_v4().to_string(),
            Utc::now(),
        );
        manager.database.insert_album(&album).await.unwrap();
        manager.database.insert_release(&release).await.unwrap();
        manager.database.insert_artist(&guest).await.unwrap();
        manager.database.insert_track(&track).await.unwrap();
        manager
            .database
            .insert_track_artist(&track_artist)
            .await
            .unwrap();

        let detail = manager
            .find_release_detail(&release.id)
            .await
            .unwrap()
            .expect("detail present");
        detail.tracks[0].display_artist.clone()
    }

    assert_eq!(
        resolve_display_artist(true).await.as_deref(),
        Some("Guest Performer"),
        "a compilation row shows its own artist"
    );
    assert_eq!(
        resolve_display_artist(false).await,
        None,
        "a single-artist album row shows no artist"
    );
}

#[tokio::test]
async fn gallery_includes_cloud_only_image_files_with_no_local_path() {
    let (manager, _temp_dir) = setup_test_manager().await;
    let album = create_test_album();
    let release = create_test_release(&album.id);
    manager.database.insert_album(&album).await.unwrap();
    manager.database.insert_release(&release).await.unwrap();

    // An image file for the release with no local copy on this device — the
    // release's images live only in the cloud here.
    let image = crate::db::DbFile::new(
        &release.id,
        "back.jpg",
        1234,
        crate::util::content_type::ContentType::Jpeg,
        Uuid::new_v4().to_string(),
        Utc::now(),
        crate::util::fs::hash_bytes(b"fixture"),
    );
    manager.add_file(&image).await.unwrap();

    let detail = manager
        .find_release_detail(&release.id)
        .await
        .unwrap()
        .expect("detail present");

    // The lightbox shows every image the release has: the image file is
    // surfaced as a gallery item read by file id (fetched on demand), a
    // release-file source, not the cover slot.
    let item = detail
        .gallery_items
        .iter()
        .find(|g| g.id == image.id)
        .expect("image file surfaced in gallery");
    assert_eq!(item.label, "back.jpg");
    assert!(
        matches!(
            item.source,
            crate::album_detail::GallerySource::ReleaseFile { .. }
        ),
        "a release-file image is read by file id, not as the cover"
    );
}

/// `change_cover` resizes whatever the user picks to a ≤600 JPEG thumbnail
/// before storing it: a 900×300 PNG release image lands as a 600×200 JPEG blob
/// (downscaled to fit 600, aspect kept), and the `covers` row records JPEG.
#[tokio::test]
async fn change_cover_stores_a_resized_jpeg_thumbnail() {
    let (manager, _temp_dir) = setup_test_manager().await;
    let album = create_test_album();
    let mut release = create_test_release(&album.id);
    release.remote = false;
    manager.database.insert_album(&album).await.unwrap();
    manager.database.insert_release(&release).await.unwrap();

    // An oversized non-JPEG release image on disk, registered as the release's
    // user-provided file so `change_cover` reads it back through coven.
    let source_dir = TempDir::new().unwrap();
    let cover_bytes = {
        let img = ::image::RgbImage::from_pixel(900, 300, ::image::Rgb([20, 160, 90]));
        let mut buf = std::io::Cursor::new(Vec::new());
        ::image::DynamicImage::ImageRgb8(img)
            .write_to(&mut buf, ::image::ImageFormat::Png)
            .unwrap();
        buf.into_inner()
    };
    std::fs::write(source_dir.path().join("art.png"), &cover_bytes).unwrap();
    let file = DbFile::new(
        &release.id,
        "art.png",
        cover_bytes.len() as i64,
        ContentType::Png,
        Uuid::new_v4().to_string(),
        Utc::now(),
        crate::util::fs::hash_bytes(&cover_bytes),
    );
    manager.add_file(&file).await.unwrap();
    manager
        .database
        .register_release_external_refs_for_test(&release.id, &source_dir.path().to_string_lossy())
        .await
        .unwrap();

    manager
        .change_cover(
            &album.id,
            &release.id,
            CoverSelection::ReleaseImage {
                file_id: file.id.clone(),
            },
        )
        .await
        .unwrap();

    // The stored blob decodes as a ≤600 JPEG, not the 900×300 PNG source.
    let stored = manager
        .read_cover_image_blob(&release.id)
        .await
        .unwrap()
        .expect("cover blob stored");
    assert_eq!(
        ::image::guess_format(&stored).unwrap(),
        ::image::ImageFormat::Jpeg
    );
    let decoded = ::image::load_from_memory(&stored).unwrap();
    assert_eq!((decoded.width(), decoded.height()), (600, 200));

    // The row describes the stored thumbnail: JPEG, and its size matches.
    let row = manager
        .get_library_image(&release.id, &LibraryImageType::Cover)
        .await
        .unwrap()
        .expect("cover row stored");
    assert_eq!(row.content_type, ContentType::Jpeg);
    assert_eq!(row.file_size, stored.len() as i64);
}

/// A cover can be changed again and again. coven's `(namespace, blob id)` names one
/// immutable byte-string — a blob's bytes are never rewritten under a live id — so
/// each change mints a NEW `blob_id`, repoints the `covers` row at it, and deletes
/// the blob it replaced. The row's hash and size describe the newly stored bytes,
/// and the old blob's cloud object is queued for deletion.
#[tokio::test]
async fn change_cover_twice_replaces_the_cover_blob() {
    let (manager, _temp_dir) = setup_test_manager().await;
    let album = create_test_album();
    let mut release = create_test_release(&album.id);
    release.remote = false;
    manager.database.insert_album(&album).await.unwrap();
    manager.database.insert_release(&release).await.unwrap();

    // Two visibly different release images, so the two stored thumbnails differ.
    let source_dir = TempDir::new().unwrap();
    let png = |rgb: [u8; 3]| {
        let img = ::image::RgbImage::from_pixel(400, 400, ::image::Rgb(rgb));
        let mut buf = std::io::Cursor::new(Vec::new());
        ::image::DynamicImage::ImageRgb8(img)
            .write_to(&mut buf, ::image::ImageFormat::Png)
            .unwrap();
        buf.into_inner()
    };
    let add_source = |name: &str, bytes: &[u8]| {
        std::fs::write(source_dir.path().join(name), bytes).unwrap();
        DbFile::new(
            &release.id,
            name,
            bytes.len() as i64,
            ContentType::Png,
            Uuid::new_v4().to_string(),
            Utc::now(),
            crate::util::fs::hash_bytes(bytes),
        )
    };
    let green = add_source("green.png", &png([20, 160, 90]));
    let red = add_source("red.png", &png([200, 40, 40]));
    manager.add_file(&green).await.unwrap();
    manager.add_file(&red).await.unwrap();
    manager
        .database
        .register_release_external_refs_for_test(&release.id, &source_dir.path().to_string_lossy())
        .await
        .unwrap();

    let change_to = async |file: &DbFile| {
        manager
            .change_cover(
                &album.id,
                &release.id,
                CoverSelection::ReleaseImage {
                    file_id: file.id.clone(),
                },
            )
            .await
    };
    let cover_row = async || {
        manager
            .get_library_image(&release.id, &LibraryImageType::Cover)
            .await
            .unwrap()
            .expect("cover row stored")
    };

    change_to(&green).await.unwrap();
    let first = cover_row().await;

    // The second change is the one that used to fail: it re-put a blob under an id
    // the `covers` row already referenced.
    change_to(&red).await.unwrap();
    let second = cover_row().await;

    // The row moved to a new blob, and describes the bytes that blob holds.
    assert_ne!(
        second.blob_id, first.blob_id,
        "a replaced cover is a new blob, not new bytes under the old id"
    );
    let stored = manager
        .read_cover_image_blob(&release.id)
        .await
        .unwrap()
        .expect("cover blob stored");
    assert_eq!(
        second.content_hash.as_str(),
        crate::util::fs::hash_bytes(&stored).as_str()
    );
    assert_eq!(second.file_size, stored.len() as i64);

    // The bytes really are the second image's, not the first's.
    let first_stored_len = first.file_size;
    assert_ne!(
        stored.len() as i64,
        first_stored_len,
        "the two source images must produce different thumbnails for this test to mean anything"
    );

    // The row now points at the new blob, so the old one has no row reference to
    // address it by; what must hold is that its bytes are gone from this device —
    // the replace declared its deletion, so coven reclaimed them. This release is
    // Local, so there is no cloud object behind the old blob and nothing to
    // tombstone; the Remote case is `delete_release_removes_its_cover_image`.
    assert!(
        !manager
            .library_dir()
            .local_blob_path(crate::sync::COVERS_NAMESPACE, &first.blob_id)
            .expect("a valid blob path")
            .exists(),
        "the replaced cover blob's bytes must be reclaimed"
    );
}

/// On a browsable home a cover's cloud key is the row's readable `cloud_path`, and
/// that path carries the blob id — so replacing a cover writes a NEW object rather
/// than overwriting the one it replaces. A reused key cannot be made to converge:
/// two devices replacing the same cover would race for one object, and a device
/// applying a changeset written before a replacement could never satisfy that
/// changeset's content hash. Distinct keys leave the superseded object readable
/// until its tombstone is collected.
#[tokio::test]
async fn change_cover_twice_on_a_browsable_home_writes_two_distinct_cloud_keys() {
    let (manager, _temp_dir) = setup_test_manager().await;
    manager.set_home_storage(crate::config::HomeStorage::Browsable);
    let album = create_test_album();
    let mut release = create_test_release(&album.id);
    release.remote = false;
    manager.database.insert_album(&album).await.unwrap();
    manager.database.insert_release(&release).await.unwrap();

    let source_dir = TempDir::new().unwrap();
    let png = |rgb: [u8; 3]| {
        let img = ::image::RgbImage::from_pixel(400, 400, ::image::Rgb(rgb));
        let mut buf = std::io::Cursor::new(Vec::new());
        ::image::DynamicImage::ImageRgb8(img)
            .write_to(&mut buf, ::image::ImageFormat::Png)
            .unwrap();
        buf.into_inner()
    };
    let add_source = |name: &str, bytes: &[u8]| {
        std::fs::write(source_dir.path().join(name), bytes).unwrap();
        DbFile::new(
            &release.id,
            name,
            bytes.len() as i64,
            ContentType::Png,
            Uuid::new_v4().to_string(),
            Utc::now(),
            crate::util::fs::hash_bytes(bytes),
        )
    };
    let green = add_source("green.png", &png([20, 160, 90]));
    let red = add_source("red.png", &png([200, 40, 40]));
    manager.add_file(&green).await.unwrap();
    manager.add_file(&red).await.unwrap();
    manager
        .database
        .register_release_external_refs_for_test(&release.id, &source_dir.path().to_string_lossy())
        .await
        .unwrap();

    let change_to = async |file: &DbFile| {
        manager
            .change_cover(
                &album.id,
                &release.id,
                CoverSelection::ReleaseImage {
                    file_id: file.id.clone(),
                },
            )
            .await
            .unwrap();
        manager
            .get_library_image(&release.id, &LibraryImageType::Cover)
            .await
            .unwrap()
            .expect("cover row stored")
    };

    let first = change_to(&green).await;
    let second = change_to(&red).await;

    // Each row's readable path names its own blob, so the two never collide.
    assert_eq!(
        first.cloud_path.as_deref(),
        Some(format!("{}/{}/cover-{}.jpg", album.id, release.id, first.blob_id).as_str())
    );
    assert_ne!(
        first.cloud_path, second.cloud_path,
        "a replaced cover must not reuse the object its predecessor occupies"
    );

    // The two keys really are distinct objects, so writing the second never
    // overwrites the first.
    let old_blob = crate::sync::image_blob_ref(
        crate::sync::COVERS_NAMESPACE,
        &first.blob_id,
        first.cloud_path.clone(),
    );
    let new_blob = crate::sync::image_blob_ref(
        crate::sync::COVERS_NAMESPACE,
        &second.blob_id,
        second.cloud_path.clone(),
    );
    let old_key = manager.handle().blob_cloud_key(&old_blob).unwrap();
    let new_key = manager.handle().blob_cloud_key(&new_blob).unwrap();
    assert_ne!(old_key, new_key);
}

/// Queueing an album expands to its PRIMARY release's tracks, not the
/// earliest-imported one. When the user picks a non-default primary (e.g. a
/// later remaster over the original vinyl rip), enqueueing the album must
/// play the chosen release.
#[tokio::test]
async fn resolve_to_track_ids_expands_album_to_primary_release() {
    let (manager, _temp_dir) = setup_test_manager().await;
    let album = create_test_album();
    // release1 is imported first (older created_at, so it sorts first);
    // release2 is the user's chosen primary.
    let mut release1 = create_test_release(&album.id);
    release1.created_at = Utc::now() - chrono::Duration::days(1);
    let release2 = create_test_release(&album.id);

    manager.database.insert_album(&album).await.unwrap();
    manager.database.insert_release(&release1).await.unwrap();
    manager.database.insert_release(&release2).await.unwrap();

    let old = crate::db::DbTrack::new_test(
        &release1.id,
        "48ae00a1-d7a5-443c-8240-f999fc4ddfcc",
        "Old A",
        Some(1),
    );
    let p1 = crate::db::DbTrack::new_test(
        &release2.id,
        "cc4180bc-58f5-456f-8116-f9b2099f5b7f",
        "New A",
        Some(1),
    );
    let p2 = crate::db::DbTrack::new_test(
        &release2.id,
        "cc4181bc-58f5-4722-8116-fab2099f5d32",
        "New B",
        Some(2),
    );
    manager.database.insert_track(&old).await.unwrap();
    manager.database.insert_track(&p1).await.unwrap();
    manager.database.insert_track(&p2).await.unwrap();

    manager
        .set_album_primary_release(&album.id, &release2.id)
        .await
        .unwrap();

    let resolved = manager
        .resolve_to_track_ids(std::slice::from_ref(&album.id))
        .await
        .unwrap();
    assert!(resolved.contains(&"cc4180bc-58f5-456f-8116-f9b2099f5b7f".to_string()));
    assert!(resolved.contains(&"cc4181bc-58f5-4722-8116-fab2099f5d32".to_string()));
    assert!(
        !resolved.contains(&"48ae00a1-d7a5-443c-8240-f999fc4ddfcc".to_string()),
        "must not expand to the non-primary release's tracks"
    );
    assert_eq!(resolved.len(), 2);
}

#[tokio::test]
async fn resolve_to_track_ids_rejects_unknown_id() {
    let (manager, _temp_dir) = setup_test_manager().await;
    let err = manager
        .resolve_to_track_ids(&["missing-id".to_string()])
        .await
        .unwrap_err();
    assert!(
        matches!(err, LibraryError::TrackMapping(message) if message.contains("missing-id")),
        "unknown ids must fail instead of being treated as track ids"
    );
}

#[tokio::test]
async fn find_release_detail_display_name_uses_year_format_fallback() {
    let (manager, _temp_dir) = setup_test_manager().await;
    let album = create_test_album();
    let mut release = create_test_release(&album.id);
    release.pressing.year = Some(2024);
    release.pressing.format = Some("CD".to_string());
    manager.database.insert_album(&album).await.unwrap();
    manager.database.insert_release(&release).await.unwrap();

    let detail = manager
        .find_release_detail(&release.id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(detail.display_name, "2024 CD");
}

#[tokio::test]
async fn find_release_detail_display_name_prefers_release_name() {
    let (manager, _temp_dir) = setup_test_manager().await;
    let album = create_test_album();
    let mut release = create_test_release(&album.id);
    release.release_name = Some("Deluxe Edition".to_string());
    release.pressing.year = Some(2024);
    release.pressing.format = Some("CD".to_string());
    manager.database.insert_album(&album).await.unwrap();
    manager.database.insert_release(&release).await.unwrap();

    let detail = manager
        .find_release_detail(&release.id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(detail.display_name, "Deluxe Edition");
}

#[tokio::test]
async fn find_release_detail_uses_position_for_second_release() {
    let (manager, _temp_dir) = setup_test_manager().await;
    let album = create_test_album();
    let mut release1 = create_test_release(&album.id);
    release1.pressing.year = None;
    release1.pressing.format = None;
    let mut release2 = create_test_release(&album.id);
    release2.pressing.year = None;
    release2.pressing.format = None;
    manager.database.insert_album(&album).await.unwrap();
    manager.database.insert_release(&release1).await.unwrap();
    manager.database.insert_release(&release2).await.unwrap();

    let detail2 = manager
        .find_release_detail(&release2.id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(detail2.display_name, "Release 2");
}

#[tokio::test]
async fn find_release_detail_returns_none_for_unknown_id() {
    let (manager, _temp_dir) = setup_test_manager().await;
    let detail = manager.find_release_detail("nonexistent-id").await.unwrap();
    assert!(detail.is_none());
}

#[tokio::test]
async fn find_release_detail_with_returns_none_for_deleted_release() {
    let (manager, _temp_dir) = setup_test_manager().await;
    let album = create_test_album();
    let release = create_test_release(&album.id);
    manager.database.insert_album(&album).await.unwrap();
    manager.database.insert_release(&release).await.unwrap();

    manager.delete_release(&release.id).await.unwrap();

    let detail = crate::library::manager::find_release_detail_with(
        &manager.database,
        manager.handle(),
        true,
        true,
        &release.id,
    )
    .await
    .expect("deleted release lookup must not error");

    assert!(detail.is_none());
}

#[tokio::test]
async fn upload_observer_processes_transition_after_deleted_release() {
    use coven::BlobTransitionObserver;

    let (manager, _temp_dir) = setup_test_manager().await;
    let album = create_test_album();
    let deleted_release = create_test_release(&album.id);
    let remaining_release = create_test_release(&album.id);
    manager.database.insert_album(&album).await.unwrap();
    manager
        .database
        .insert_release(&deleted_release)
        .await
        .unwrap();
    manager
        .database
        .insert_release(&remaining_release)
        .await
        .unwrap();

    manager.delete_release(&deleted_release.id).await.unwrap();
    let mut events = manager.subscribe_events();

    let observer = crate::sync::upload_observer::ReleaseUploadObserver::new(
        manager.sync.outbox_in_flight(),
        manager.sync.upload_sessions(),
        manager.sync.upload_throughput(),
        manager.sync.sync_paused(),
        manager.event_tx.clone(),
    );
    observer.set_database(Arc::new(manager.database.clone()));
    observer.set_handle(manager.handle().clone());

    observer
        .on_root_made_local("releases", &deleted_release.id)
        .await;
    observer
        .on_root_made_local("releases", &remaining_release.id)
        .await;

    let mut saw_remaining_update = false;
    for _ in 0..4 {
        let event = tokio::time::timeout(std::time::Duration::from_secs(2), events.recv())
            .await
            .expect("observer event")
            .expect("event channel stays open");
        if matches!(
            &event,
            LibraryEvent::ReleaseUpdated { release, .. } if release.summary.id == remaining_release.id
        ) {
            saw_remaining_update = true;
            break;
        }
    }

    assert!(
        saw_remaining_update,
        "observer must emit the later release transition"
    );
}

/// A non-release root (covers, artist images) completing its make-remote must
/// still push a fresh outbox snapshot: such a root often commits last in a
/// burst, and without this emission the queue pane freezes on the previous
/// snapshot instead of clearing.
#[tokio::test]
async fn non_release_root_completion_emits_outbox_changed() {
    use coven::BlobTransitionObserver;

    let (manager, _temp_dir) = setup_test_manager().await;
    let mut events = manager.subscribe_events();

    let observer = crate::sync::upload_observer::ReleaseUploadObserver::new(
        manager.sync.outbox_in_flight(),
        manager.sync.upload_sessions(),
        manager.sync.upload_throughput(),
        manager.sync.sync_paused(),
        manager.event_tx.clone(),
    );
    observer.set_database(Arc::new(manager.database.clone()));

    observer.on_root_made_remote("covers", COVER_1).await;

    let event = tokio::time::timeout(std::time::Duration::from_secs(2), events.recv())
        .await
        .expect("covers-root completion must emit an outbox snapshot")
        .expect("event channel stays open");
    assert!(
        matches!(event, LibraryEvent::OutboxChanged { .. }),
        "expected OutboxChanged, got {event:?}",
    );
}

// ── Storage page tests ───────────────────────────────────────────

/// Insert N albums each with one release; return `(albums, releases)`.
/// Each album's title is `"Album {i}"` so ordering is deterministic.
async fn seed_albums(manager: &LibraryManager, count: usize) -> (Vec<DbAlbum>, Vec<DbRelease>) {
    let mut albums = Vec::new();
    let mut releases = Vec::new();
    for i in 0..count {
        let mut album = create_test_album();
        album.title = format!("Album {i}");
        let release = create_test_release(&album.id);
        manager.database.insert_album(&album).await.unwrap();
        manager.database.insert_release(&release).await.unwrap();
        albums.push(album);
        releases.push(release);
    }
    (albums, releases)
}

fn sort_by_album_title_asc() -> crate::db::StorageSortCriterion {
    crate::db::StorageSortCriterion {
        field: crate::db::StorageSortField::AlbumTitle,
        direction: crate::db::SortDirection::Ascending,
    }
}

#[tokio::test]
async fn storage_page_returns_all_rows_for_all_filter() {
    let (manager, _temp_dir) = setup_test_manager().await;
    seed_albums(&manager, 3).await;

    let page = manager
        .get_storage_page(
            &sort_by_album_title_asc(),
            crate::db::StorageFilter::All,
            0,
            10,
        )
        .await
        .unwrap();
    assert_eq!(page.rows.len(), 3);
    assert_eq!(page.total_count, 3);
}

#[tokio::test]
async fn storage_page_paginates() {
    let (manager, _temp_dir) = setup_test_manager().await;
    seed_albums(&manager, 5).await;

    let page1 = manager
        .get_storage_page(
            &sort_by_album_title_asc(),
            crate::db::StorageFilter::All,
            0,
            2,
        )
        .await
        .unwrap();
    let page2 = manager
        .get_storage_page(
            &sort_by_album_title_asc(),
            crate::db::StorageFilter::All,
            2,
            2,
        )
        .await
        .unwrap();
    let page3 = manager
        .get_storage_page(
            &sort_by_album_title_asc(),
            crate::db::StorageFilter::All,
            4,
            2,
        )
        .await
        .unwrap();

    assert_eq!(page1.rows.len(), 2);
    assert_eq!(page2.rows.len(), 2);
    assert_eq!(page3.rows.len(), 1);
    // total_count is the full filtered universe, not the page.
    assert_eq!(page1.total_count, 5);
    assert_eq!(page2.total_count, 5);
    assert_eq!(page3.total_count, 5);
}

#[tokio::test]
async fn storage_page_sorts_album_title_ascending() {
    let (manager, _temp_dir) = setup_test_manager().await;
    seed_albums(&manager, 3).await;

    let page = manager
        .get_storage_page(
            &sort_by_album_title_asc(),
            crate::db::StorageFilter::All,
            0,
            10,
        )
        .await
        .unwrap();
    let titles: Vec<_> = page.rows.iter().map(|r| r.album.title.clone()).collect();
    assert_eq!(titles, vec!["Album 0", "Album 1", "Album 2"]);
}

#[tokio::test]
async fn storage_page_sorts_album_title_descending() {
    let (manager, _temp_dir) = setup_test_manager().await;
    seed_albums(&manager, 3).await;

    let sort = crate::db::StorageSortCriterion {
        field: crate::db::StorageSortField::AlbumTitle,
        direction: crate::db::SortDirection::Descending,
    };
    let page = manager
        .get_storage_page(&sort, crate::db::StorageFilter::All, 0, 10)
        .await
        .unwrap();
    let titles: Vec<_> = page.rows.iter().map(|r| r.album.title.clone()).collect();
    assert_eq!(titles, vec!["Album 2", "Album 1", "Album 0"]);
}

/// Each storage-page row carries the state-appropriate `storage_actions`
/// the Storage Manager row context menu renders — pinned offers unpin +
/// unmanage, cloud-only offers pin + unmanage, local offers manage.
/// With a cloud home present every remote/local transition is open.
#[cfg(feature = "test-utils")]
#[tokio::test]
async fn storage_page_rows_carry_state_appropriate_actions() {
    use crate::album_detail::ReleaseStorageAction::{MakeLocal, MakeRemote, Pin, Unpin};

    let (manager, temp_dir) = setup_test_manager().await;
    connect_test_cloud_with_sync_loop(&manager).await;

    // Pinned: made Remote with pin, so its blob lands in coven's offline cache.
    let pinned = make_remote_release_under_sync_loop(
        &manager,
        &temp_dir.path().join("pinned"),
        "Pinned Album",
        true,
    )
    .await;
    // Cloud-only: made Remote without pin, so its blob is evictable, not pinned.
    let cloud_only = make_remote_release_under_sync_loop(
        &manager,
        &temp_dir.path().join("cloud"),
        "Cloud Album",
        false,
    )
    .await;

    // Local: not remote, files at a local path.
    let mut local_album = create_test_album();
    local_album.title = "Local Album".to_string();
    let mut local = create_test_release(&local_album.id);
    local.remote = false;
    manager.database.insert_album(&local_album).await.unwrap();
    manager.database.insert_release(&local).await.unwrap();
    manager
        .database
        .register_release_external_refs_for_test(&local.id, "/tmp/local")
        .await
        .unwrap();

    let page = manager
        .get_storage_page(
            &sort_by_album_title_asc(),
            crate::db::StorageFilter::All,
            0,
            10,
        )
        .await
        .unwrap();
    let actions: std::collections::HashMap<_, _> = page
        .rows
        .iter()
        .map(|r| (r.release.id.clone(), r.release.storage_actions.clone()))
        .collect();

    assert_eq!(actions[&pinned], vec![Unpin, MakeLocal]);
    assert_eq!(actions[&cloud_only], vec![Pin, MakeLocal]);
    assert_eq!(actions[&local.id], vec![MakeRemote]);
}

/// With no cloud home, no remote storage exists, so the rows offer no
/// transitions — the context menu is empty everywhere.
#[tokio::test]
async fn storage_page_rows_have_no_actions_without_cloud_home() {
    let (manager, _temp_dir) = setup_test_manager().await;
    let album = create_test_album();
    let mut release = create_test_release(&album.id);
    release.remote = false;
    manager.database.insert_album(&album).await.unwrap();
    manager.database.insert_release(&release).await.unwrap();
    manager
        .database
        .register_release_external_refs_for_test(&release.id, "/tmp/local")
        .await
        .unwrap();

    let page = manager
        .get_storage_page(
            &sort_by_album_title_asc(),
            crate::db::StorageFilter::All,
            0,
            10,
        )
        .await
        .unwrap();
    assert_eq!(page.rows.len(), 1);
    assert!(page.rows[0].release.storage_actions.is_empty());
}

#[tokio::test]
async fn storage_page_local_filter_matches_local_path() {
    let (manager, _temp_dir) = setup_test_manager().await;
    let album_remote = create_test_album();
    let mut album_local = create_test_album();
    album_local.title = "Local Album".to_string();
    let remote_release = create_test_release(&album_remote.id);
    let mut local_release = create_test_release(&album_local.id);
    local_release.remote = false;

    manager.database.insert_album(&album_remote).await.unwrap();
    manager.database.insert_album(&album_local).await.unwrap();
    manager
        .database
        .insert_release(&remote_release)
        .await
        .unwrap();
    manager
        .database
        .insert_release(&local_release)
        .await
        .unwrap();
    manager
        .database
        .register_release_external_refs_for_test(&local_release.id, "/tmp/local")
        .await
        .unwrap();

    let local = manager
        .get_storage_page(
            &sort_by_album_title_asc(),
            crate::db::StorageFilter::Local,
            0,
            10,
        )
        .await
        .unwrap();
    assert_eq!(local.rows.len(), 1);
    assert_eq!(local.total_count, 1);
    assert_eq!(local.rows[0].release.id, local_release.id);
    assert_eq!(
        local.rows[0].release.storage_state,
        crate::album_detail::ReleaseStorageState::Local
    );

    let remote = manager
        .get_storage_page(
            &sort_by_album_title_asc(),
            crate::db::StorageFilter::Remote,
            0,
            10,
        )
        .await
        .unwrap();
    assert_eq!(remote.rows.len(), 1);
    assert_eq!(remote.rows[0].release.id, remote_release.id);
    assert_ne!(
        remote.rows[0].release.storage_state,
        crate::album_detail::ReleaseStorageState::Local
    );
}

#[tokio::test]
async fn storage_count_matches_filtered_page_total() {
    let (manager, _temp_dir) = setup_test_manager().await;
    // Three albums, one release each. Mark the second release
    // local at insert time so filters produce distinct counts.
    let mut inserted_local = None;
    for i in 0..3 {
        let mut album = create_test_album();
        album.title = format!("Album {i}");
        let mut release = create_test_release(&album.id);
        let local = i == 1;
        if local {
            release.remote = false;
            inserted_local = Some(release.id.clone());
        }
        manager.database.insert_album(&album).await.unwrap();
        manager.database.insert_release(&release).await.unwrap();
        if local {
            manager
                .database
                .register_release_external_refs_for_test(&release.id, "/tmp/local")
                .await
                .unwrap();
        }
    }

    assert_eq!(
        manager
            .get_storage_count(crate::db::StorageFilter::All)
            .await
            .unwrap(),
        3
    );
    assert_eq!(
        manager
            .get_storage_count(crate::db::StorageFilter::Local)
            .await
            .unwrap(),
        1
    );
    assert_eq!(
        manager
            .get_storage_count(crate::db::StorageFilter::Remote)
            .await
            .unwrap(),
        2
    );
    assert_eq!(
        manager
            .get_storage_count(crate::db::StorageFilter::Uploading)
            .await
            .unwrap(),
        0
    );

    let all_page = manager
        .get_storage_page(
            &sort_by_album_title_asc(),
            crate::db::StorageFilter::All,
            0,
            10,
        )
        .await
        .unwrap();
    assert_eq!(all_page.total_count, 3);

    let local_page = manager
        .get_storage_page(
            &sort_by_album_title_asc(),
            crate::db::StorageFilter::Local,
            0,
            10,
        )
        .await
        .unwrap();
    assert_eq!(local_page.rows.len(), 1);
    assert_eq!(local_page.rows[0].release.id, inserted_local.unwrap());
}

/// `get_storage_total_size` sums `total_size` over every storage row matching
/// `filter` — the same universe `get_storage_page` pages over — independent of
/// how many pages have loaded. For each filter, the aggregate must equal the
/// sum of `total_size` over that filter's full (unpaginated) storage page.
#[cfg(feature = "test-utils")]
#[tokio::test]
async fn storage_total_size_matches_page_total_size_sum() {
    let (manager, temp_dir) = setup_test_manager().await;
    connect_test_cloud(&manager).await;

    // Three releases: one local, one remote-and-quiet, and one mid-make-Remote
    // — its uploads are queued but undrained, so its gate has not flipped and it
    // is still Local as well as Uploading.
    let album_local = create_test_album();
    let album_remote = create_test_album();
    let mut release_local = create_test_release(&album_local.id);
    release_local.remote = false;
    let release_remote = create_test_release(&album_remote.id);

    manager.database.insert_album(&album_local).await.unwrap();
    manager.database.insert_album(&album_remote).await.unwrap();
    manager
        .database
        .insert_release(&release_local)
        .await
        .unwrap();
    manager
        .database
        .insert_release(&release_remote)
        .await
        .unwrap();

    for (release_id, file_size) in [(&release_local.id, 1_000i64), (&release_remote.id, 100)] {
        let file = DbFile {
            id: bae_test_support::test_uuid(&format!("{release_id}-file")),
            release_id: release_id.clone(),
            original_filename: "a.flac".to_string(),
            file_size,
            content_type: crate::util::content_type::ContentType::Flac,
            cloud_path: None,
            content_hash: crate::util::fs::hash_bytes(b"fixture"),
            created_at: Utc::now(),
        };
        manager.database.insert_file(&file).await.unwrap();
    }

    // The uploading release goes through the real transition, so its 10_000
    // bytes are what coven actually has queued.
    insert_release_with_queued_uploads(
        &manager,
        &temp_dir.path().join("uploading"),
        "Uploading Album",
        &[("a.flac", &vec![0u8; 10_000])],
    )
    .await;

    for filter in [
        crate::db::StorageFilter::All,
        crate::db::StorageFilter::Local,
        crate::db::StorageFilter::Remote,
        crate::db::StorageFilter::Uploading,
    ] {
        let page = manager
            .get_storage_page(&sort_by_album_title_asc(), filter, 0, 10)
            .await
            .unwrap();
        let page_sum: i64 = page.rows.iter().map(|row| row.release.total_size).sum();

        let aggregate = manager.get_storage_total_size(filter).await.unwrap();
        assert_eq!(
            aggregate, page_sum as u64,
            "{filter:?}: aggregate must equal the page's own total_size sum"
        );
    }

    // Concrete expectations, so a bug that moves the *same* wrong figure on
    // both sides doesn't slip through. The uploading release counts as Local
    // too — its gate flips only once every upload lands.
    assert_eq!(
        manager
            .get_storage_total_size(crate::db::StorageFilter::All)
            .await
            .unwrap(),
        11_100
    );
    assert_eq!(
        manager
            .get_storage_total_size(crate::db::StorageFilter::Local)
            .await
            .unwrap(),
        11_000
    );
    assert_eq!(
        manager
            .get_storage_total_size(crate::db::StorageFilter::Remote)
            .await
            .unwrap(),
        100
    );
    assert_eq!(
        manager
            .get_storage_total_size(crate::db::StorageFilter::Uploading)
            .await
            .unwrap(),
        10_000
    );
}

/// Connect a real `SyncManager` over an in-memory cloud home (opaque,
/// encrypted) so the manager's cloud read/write/transition paths run against
/// it — the in-module counterpart of the integration tests' `setup_with_cloud`.
/// After this, `has_cloud_home()` holds.
///
/// No sync loop runs behind it: the tests here drive the upload queue with
/// `drain_uploads_expecting_work` and assert what that pass moved, which is only
/// a fact if nothing else drains. A test that needs the loop's own work — the
/// Store write that publishes a transition, or `is_sync_ready()` — takes
/// [`connect_test_cloud_with_sync_loop`] instead.
#[cfg(feature = "test-utils")]
async fn connect_test_cloud(manager: &LibraryManager) -> Arc<InMemoryCloudHome> {
    let home = Arc::new(InMemoryCloudHome::new());
    manager
        .connect_test_cloud_home_caller_driven(
            home.clone(),
            CloudCipher::Encrypted(EncryptionService::from_key([7u8; 32])),
        )
        .await
        .expect("connect in-memory cloud home");
    home
}

/// [`connect_test_cloud`] with the production sync loop running behind it, for
/// the tests that wait on a cycle to publish a transition's Store write. Their
/// drains are the loop's as well as their own, so they assert on published
/// state rather than on a drain's count.
#[cfg(feature = "test-utils")]
async fn connect_test_cloud_with_sync_loop(manager: &LibraryManager) -> Arc<InMemoryCloudHome> {
    let home = Arc::new(InMemoryCloudHome::new());
    manager
        .connect_test_cloud_home(
            home.clone(),
            CloudCipher::Encrypted(EncryptionService::from_key([7u8; 32])),
        )
        .await
        .expect("connect in-memory cloud home");
    home
}

/// A release mid-make-Remote: its uploads are enqueued in coven's durable queue
/// but not drained, so the gate has not flipped and the release still reads
/// Local. This is the state the Uploading filter and the outbox snapshot render.
/// The manager must already be connected via [`connect_test_cloud`].
#[cfg(feature = "test-utils")]
async fn insert_release_with_queued_uploads(
    manager: &LibraryManager,
    dir: &std::path::Path,
    album_title: &str,
    files: &[(&str, &[u8])],
) -> DbRelease {
    let release = insert_local_release_with_files(manager, dir, album_title, files).await;
    manager.coven_make_remote(&release.id, false).await.unwrap();
    assert_eq!(
        manager
            .database
            .handle()
            .queued_uploads_for_root("releases", &release.id)
            .await
            .unwrap()
            .len(),
        files.len(),
        "every file must be queued before the drain runs"
    );
    release
}

/// A release id paired with its album id, for the fixtures that make a release
/// Remote through the real transition (which mints its own album) and then need
/// to name that album.
#[cfg(feature = "test-utils")]
struct ReleaseRef {
    id: String,
    album_id: String,
}

#[cfg(feature = "test-utils")]
impl ReleaseRef {
    async fn of(manager: &LibraryManager, id: String) -> Self {
        let album_id = manager
            .database
            .find_release_by_id(&id)
            .await
            .unwrap()
            .expect("the release exists")
            .album_id;
        Self { id, album_id }
    }
}

/// Wait until a host-provided blob on a Remote release has a committed cloud
/// object.
///
/// Publication is the sync loop's Store write, not the row write: storing a
/// cover leaves its blob `PendingRemote` with no locator, and only the next
/// cycle gives it one — which is what a cloud tombstone needs to name. A test
/// that asserts on the tombstone has to be past that point.
#[cfg(feature = "test-utils")]
async fn wait_for_published_blob(manager: &LibraryManager, namespace: &str, row_id: &str) {
    for tick in 0..2_000 {
        // Re-kick periodically: a cycle already in flight ignores the nudge, and
        // the write only activates on a cycle that starts after it was queued.
        if tick % 50 == 0 {
            manager.handle().sync_now();
        }
        let blob = manager
            .handle()
            .row_blob_ref(namespace, row_id)
            .await
            .expect("the blob-bearing row exists");
        if blob.stored().is_some() {
            return;
        }
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
    }
    panic!(
        "blob {namespace}/{row_id} never reached the cloud; pending={:?} blocked={:?}",
        manager.handle().pending_writes().await.unwrap(),
        manager.handle().blocked_writes().await.unwrap(),
    );
}

/// Create a Remote release the real way: a Local release with one source file
/// on disk (coven's external ref), made Remote (`pin` keeps its blob in the
/// offline cache) and drained so the gate flips. Returns its id. The manager
/// must already be connected via [`connect_test_cloud`].
#[cfg(feature = "test-utils")]
async fn make_remote_release(
    manager: &LibraryManager,
    dir: &std::path::Path,
    album_title: &str,
    pin: bool,
) -> String {
    make_remote_release_with_files(
        manager,
        dir,
        album_title,
        &[("track.flac", b"track-bytes")],
        pin,
    )
    .await
}

#[cfg(feature = "test-utils")]
async fn make_remote_release_with_files(
    manager: &LibraryManager,
    dir: &std::path::Path,
    album_title: &str,
    files: &[(&str, &[u8])],
    pin: bool,
) -> String {
    let release = insert_local_release_with_files(manager, dir, album_title, files).await;
    manager.coven_make_remote(&release.id, pin).await.unwrap();
    let uploaded = manager.drain_uploads_expecting_work().await.unwrap();
    assert_eq!(uploaded, files.len(), "each release blob uploaded");
    release.id
}

/// [`make_remote_release`] for a test connected with
/// [`connect_test_cloud_with_sync_loop`]: the loop drains the queue, so this
/// waits for the make-Remote to finish rather than counting a drain pass this
/// test does not own.
#[cfg(feature = "test-utils")]
async fn make_remote_release_under_sync_loop(
    manager: &LibraryManager,
    dir: &std::path::Path,
    album_title: &str,
    pin: bool,
) -> String {
    let release = insert_local_release_with_files(
        manager,
        dir,
        album_title,
        &[("track.flac", b"track-bytes")],
    )
    .await;
    manager.coven_make_remote(&release.id, pin).await.unwrap();
    wait_for_landed_make_remote(manager, &release.id).await;
    release.id
}

/// Wait for a release's make-Remote to finish under a running sync loop, and
/// assert it landed: no upload work outstanding and the gate flipped.
#[cfg(feature = "test-utils")]
async fn wait_for_landed_make_remote(manager: &LibraryManager, release_id: &str) {
    wait_for_settled_uploads(manager, release_id).await;
    assert!(
        manager
            .database
            .find_release_by_id(release_id)
            .await
            .unwrap()
            .unwrap()
            .remote,
        "every upload landed, so the release is Remote"
    );
}

#[cfg(feature = "test-utils")]
async fn insert_partially_uploaded_make_remote_release(
    manager: &LibraryManager,
    temp_dir: &std::path::Path,
) -> DbRelease {
    let source_dir = temp_dir.join(Uuid::new_v4().to_string());
    let release = insert_local_release_with_files(
        manager,
        &source_dir,
        "Partially Uploaded",
        &[("a.flac", b"uploaded"), ("b.flac", b"missing")],
    )
    .await;

    manager.coven_make_remote(&release.id, true).await.unwrap();
    assert!(manager
        .database
        .make_remote_progress_for_release(&release.id)
        .await
        .unwrap()
        .is_some());
    assert_eq!(
        manager
            .database
            .handle()
            .queued_uploads()
            .await
            .unwrap()
            .len(),
        2
    );

    std::fs::remove_file(source_dir.join("b.flac")).unwrap();
    let uploaded = manager.drain_uploads_expecting_work().await.unwrap();
    assert_eq!(uploaded, 1);
    assert!(
        !manager
            .database
            .find_release_by_id(&release.id)
            .await
            .unwrap()
            .unwrap()
            .remote,
        "the release must still be Local while one upload is unresolved"
    );
    assert_eq!(
        manager
            .database
            .handle()
            .queued_deletes()
            .await
            .unwrap()
            .len(),
        0
    );
    release
}

#[cfg(feature = "test-utils")]
async fn insert_local_release_with_files(
    manager: &LibraryManager,
    dir: &std::path::Path,
    album_title: &str,
    files: &[(&str, &[u8])],
) -> DbRelease {
    let mut album = create_test_album();
    album.title = album_title.to_string();
    let mut release = create_test_release(&album.id);
    release.remote = false;
    manager.database.insert_album(&album).await.unwrap();
    manager.database.insert_release(&release).await.unwrap();
    std::fs::create_dir_all(dir).unwrap();
    let created_at = Utc::now();
    for (index, (name, bytes)) in files.iter().enumerate() {
        std::fs::write(dir.join(name), bytes).unwrap();
        let file = DbFile::new(
            &release.id,
            name,
            bytes.len() as i64,
            crate::util::content_type::ContentType::Flac,
            bae_test_support::test_uuid(&format!("{}-test-file-{index}", release.id)),
            created_at,
            crate::util::fs::hash_bytes(bytes),
        );
        manager.add_file(&file).await.unwrap();
    }
    manager
        .database
        .register_release_external_refs_for_test(&release.id, &dir.to_string_lossy())
        .await
        .unwrap();
    release
}

/// Insert a local release rooted at a nonexistent directory, so no local copy
/// resolves on this device. Seeds a `DbFile` row so the release is otherwise
/// complete.
async fn insert_local_release_without_local_files(
    manager: &LibraryManager,
    album_id: &str,
) -> DbRelease {
    let mut release = create_test_release(album_id);
    release.remote = false;
    manager.database.insert_release(&release).await.unwrap();
    manager
        .database
        .register_release_external_refs_for_test(
            &release.id,
            &format!("/nonexistent/origin-device/{}", Uuid::new_v4()),
        )
        .await
        .unwrap();

    let file = DbFile::new(
        &release.id,
        "track1.flac",
        5,
        crate::util::content_type::ContentType::Flac,
        Uuid::new_v4().to_string(),
        Utc::now(),
        // No real file backs this fixture (the whole point is "no local copy
        // resolves"), so there is no plaintext to hash.
        crate::util::fs::hash_bytes(b"fixture"),
    );
    manager.add_file(&file).await.unwrap();
    release
}

/// Read one byte through the production audio reader and return the playback
/// error reason its fill-error handler reports.
async fn playback_error_reason_for_file(
    manager: &LibraryManager,
    file: &DbFile,
) -> crate::ui::PlaybackErrorReason {
    use crate::playback::data_source::{create_audio_reader, FetchArbiter};
    use crate::playback::sparse_buffer::create_sparse_buffer;

    let buffer = create_sparse_buffer(file.file_size as u64);
    let reader = create_audio_reader(manager, &file.id, FetchArbiter::new(), None, false);
    let (error_tx, mut error_rx) = tokio::sync::mpsc::unbounded_channel();
    reader.start_reading(
        buffer.clone(),
        Box::new(move |error| {
            let _ = error_tx.send(error);
        }),
    );
    // Register demand so the fill fetches; the failed fetch cancels the
    // buffer, which unblocks this read with `None`.
    let demand = tokio::task::spawn_blocking(move || {
        let mut r = buffer.new_reader();
        let mut b = [0u8; 1];
        r.read(&mut b)
    });
    let reason = tokio::time::timeout(std::time::Duration::from_secs(10), async {
        error_rx
            .recv()
            .await
            .expect("error channel open")
            .into_ui_reason()
    })
    .await
    .expect("a playback error must be reported");
    demand.await.expect("demand read task");
    reason
}

/// A Remote track whose bytes must come from the cloud, read with no provider
/// connected, reports `SyncDisconnected` — the reconnect-sync state — not a
/// generic diagnostic. coven raises `NoCloudHome` for the cloud miss; the
/// classifier keys it.
#[cfg(feature = "test-utils")]
#[tokio::test]
async fn remote_read_with_sync_disconnected_reports_sync_disconnected() {
    use crate::ui::PlaybackErrorReason;
    let (manager, dir) = setup_test_manager().await;
    connect_test_cloud(&manager).await;
    let release_id = make_remote_release_with_files(
        &manager,
        dir.path(),
        "Album Title",
        &[("track.flac", b"track-bytes")],
        false,
    )
    .await;
    manager.disconnect_cloud_provider().unwrap();

    let files = manager
        .database
        .get_files_for_release(&release_id)
        .await
        .unwrap();
    let file = files.first().expect("the release has a file");
    let reason = playback_error_reason_for_file(&manager, file).await;
    assert!(
        matches!(reason, PlaybackErrorReason::SyncDisconnected),
        "got {reason:?}"
    );
}

/// A Local track whose source file was removed while its cloud upload is still
/// queued reports `UploadPending` — wait for the upload — because a
/// queued upload for the file explains the missing source. coven
/// raises `ExternalMissing`; the outbox check keys it.
#[cfg(feature = "test-utils")]
#[tokio::test]
async fn pending_upload_with_missing_source_reports_upload_pending() {
    use crate::ui::PlaybackErrorReason;
    let (manager, temp_dir) = setup_test_manager().await;
    connect_test_cloud(&manager).await;
    let release = insert_partially_uploaded_make_remote_release(&manager, temp_dir.path()).await;

    let files = manager
        .database
        .get_files_for_release(&release.id)
        .await
        .unwrap();
    let file = files
        .iter()
        .find(|f| f.original_filename == "b.flac")
        .expect("the un-uploaded file whose source was removed");
    let reason = playback_error_reason_for_file(&manager, file).await;
    assert!(
        matches!(reason, PlaybackErrorReason::UploadPending),
        "got {reason:?}"
    );
}

/// A Local track whose source file is gone with no queued upload stays a
/// diagnostic — the "files missing / moved" state, not `UploadPending`. This
/// pins the discriminator: `ExternalMissing` alone is not upload-pending.
#[tokio::test]
async fn missing_source_without_pending_upload_stays_diagnostic() {
    use crate::ui::PlaybackErrorReason;
    let (manager, _temp_dir) = setup_test_manager().await;
    let album = create_test_album();
    manager.database.insert_album(&album).await.unwrap();
    let release = insert_local_release_without_local_files(&manager, &album.id).await;

    let files = manager
        .database
        .get_files_for_release(&release.id)
        .await
        .unwrap();
    let file = files.first().expect("the release has a file");
    let reason = playback_error_reason_for_file(&manager, file).await;
    assert!(
        matches!(reason, PlaybackErrorReason::Diagnostic { .. }),
        "got {reason:?}"
    );
}

/// The read layer surfaces a local release even when no local copy
/// resolves on this device — there is no availability filter to hide one.
/// The substrate gate (coven's `gated_by_descendants`) prunes such a
/// release's album from a *peer's* sync entirely, so a receiver never
/// materializes an orphan album; nothing is hidden on read here.
#[tokio::test]
async fn surfaces_local_release_with_no_local_files() {
    let (manager, _temp_dir) = setup_test_manager().await;

    let album = create_test_album();
    manager.database.insert_album(&album).await.unwrap();
    let release = insert_local_release_without_local_files(&manager, &album.id).await;

    // Grid and count include the album.
    let page = manager.get_album_page(&[], 0, 10).await.unwrap();
    assert_eq!(page.len(), 1);
    assert_eq!(page[0].release_ids, vec![release.id.clone()]);
    assert_eq!(manager.get_album_count().await.unwrap(), 1);

    // Album detail carries the release.
    let detail = manager
        .find_album_detail(&album.id)
        .await
        .unwrap()
        .expect("album surfaces");
    let detail_ids: Vec<_> = detail
        .releases
        .iter()
        .map(|r| r.summary.id.clone())
        .collect();
    assert_eq!(detail_ids, vec![release.id.clone()]);

    // The release-level resolver returns it.
    assert!(manager
        .find_release_detail(&release.id)
        .await
        .unwrap()
        .is_some());
}

#[tokio::test]
async fn storage_page_sort_by_artist_names() {
    let (manager, _temp_dir) = setup_test_manager().await;

    // Two artists with distinct sort-orderings; ArtistNames sort triggers
    // the `needs_artist_sort_join` branch.
    for (artist_id, artist_name) in &[
        ("ba5b6a6c-bc8c-4015-8b3c-03e78dfe28e5", "Zulu"),
        ("f2ad46f1-3a5e-4bb5-807f-5a314ae94f25", "Alpha"),
    ] {
        let artist = DbArtist {
            id: artist_id.to_string(),
            name: artist_name.to_string(),
            sort_name: None,
            discogs_artist_id: None,
            musicbrainz_artist_id: None,
            created_at: Utc::now(),
        };
        manager.database.insert_artist(&artist).await.unwrap();
        let mut album = create_test_album();
        album.title = format!("Album by {artist_name}");
        album.artist_id = artist_id.to_string();
        let release = create_test_release(&album.id);
        manager.database.insert_album(&album).await.unwrap();
        manager.database.insert_release(&release).await.unwrap();
    }

    let asc = crate::db::StorageSortCriterion {
        field: crate::db::StorageSortField::ArtistNames,
        direction: crate::db::SortDirection::Ascending,
    };
    let page = manager
        .get_storage_page(&asc, crate::db::StorageFilter::All, 0, 10)
        .await
        .unwrap();
    let names: Vec<_> = page.rows.iter().map(|r| r.album.title.clone()).collect();
    assert_eq!(names, vec!["Album by Alpha", "Album by Zulu"]);

    let desc = crate::db::StorageSortCriterion {
        field: crate::db::StorageSortField::ArtistNames,
        direction: crate::db::SortDirection::Descending,
    };
    let page = manager
        .get_storage_page(&desc, crate::db::StorageFilter::All, 0, 10)
        .await
        .unwrap();
    let names: Vec<_> = page.rows.iter().map(|r| r.album.title.clone()).collect();
    assert_eq!(names, vec!["Album by Zulu", "Album by Alpha"]);
}

#[tokio::test]
async fn storage_page_sort_by_format_nulls_last() {
    let (manager, _temp_dir) = setup_test_manager().await;

    for (title, format) in &[
        ("Album No Format", None),
        ("Album CD", Some("CD")),
        ("Album Vinyl", Some("Vinyl")),
    ] {
        let mut album = create_test_album();
        album.title = title.to_string();
        let mut release = create_test_release(&album.id);
        release.pressing.format = format.map(str::to_string);
        manager.database.insert_album(&album).await.unwrap();
        manager.database.insert_release(&release).await.unwrap();
    }

    let asc = crate::db::StorageSortCriterion {
        field: crate::db::StorageSortField::Format,
        direction: crate::db::SortDirection::Ascending,
    };
    let page = manager
        .get_storage_page(&asc, crate::db::StorageFilter::All, 0, 10)
        .await
        .unwrap();
    let titles: Vec<_> = page.rows.iter().map(|r| r.album.title.clone()).collect();
    // NULL format sorts last in both directions.
    assert_eq!(titles, vec!["Album CD", "Album Vinyl", "Album No Format"]);
}

#[tokio::test]
async fn storage_page_sort_by_file_count() {
    let (manager, _temp_dir) = setup_test_manager().await;

    // Three releases, each with a distinct number of files.
    for (title, file_count) in &[("Album A", 1usize), ("Album B", 3), ("Album C", 2)] {
        let mut album = create_test_album();
        album.title = title.to_string();
        let release = create_test_release(&album.id);
        manager.database.insert_album(&album).await.unwrap();
        manager.database.insert_release(&release).await.unwrap();
        for i in 0..*file_count {
            let file = DbFile {
                id: bae_test_support::test_uuid(&format!("{}-file-{i}", release.id)),
                release_id: release.id.clone(),
                original_filename: format!("{i}.flac"),
                file_size: 1000,
                content_type: crate::util::content_type::ContentType::Flac,
                cloud_path: None,
                content_hash: crate::util::fs::hash_bytes(b"fixture"),
                created_at: Utc::now(),
            };
            manager.database.insert_file(&file).await.unwrap();
        }
    }

    let desc = crate::db::StorageSortCriterion {
        field: crate::db::StorageSortField::FileCount,
        direction: crate::db::SortDirection::Descending,
    };
    let page = manager
        .get_storage_page(&desc, crate::db::StorageFilter::All, 0, 10)
        .await
        .unwrap();
    let titles: Vec<_> = page.rows.iter().map(|r| r.album.title.clone()).collect();
    assert_eq!(titles, vec!["Album B", "Album C", "Album A"]);
}

#[tokio::test]
async fn storage_page_sort_by_total_size() {
    let (manager, _temp_dir) = setup_test_manager().await;

    for (title, file_size) in &[("Small", 100i64), ("Big", 10_000), ("Medium", 1_000)] {
        let mut album = create_test_album();
        album.title = title.to_string();
        let release = create_test_release(&album.id);
        manager.database.insert_album(&album).await.unwrap();
        manager.database.insert_release(&release).await.unwrap();
        let file = DbFile {
            id: bae_test_support::test_uuid(&format!("{}-file", release.id)),
            release_id: release.id.clone(),
            original_filename: "a.flac".to_string(),
            file_size: *file_size,
            content_type: crate::util::content_type::ContentType::Flac,
            cloud_path: None,
            content_hash: crate::util::fs::hash_bytes(b"fixture"),
            created_at: Utc::now(),
        };
        manager.database.insert_file(&file).await.unwrap();
    }

    let asc = crate::db::StorageSortCriterion {
        field: crate::db::StorageSortField::TotalSize,
        direction: crate::db::SortDirection::Ascending,
    };
    let page = manager
        .get_storage_page(&asc, crate::db::StorageFilter::All, 0, 10)
        .await
        .unwrap();
    let titles: Vec<_> = page.rows.iter().map(|r| r.album.title.clone()).collect();
    assert_eq!(titles, vec!["Small", "Medium", "Big"]);
}

/// The Uploading filter is coven's upload queue, not a bae column: only the
/// release whose make-Remote is still enqueued appears under it.
#[cfg(feature = "test-utils")]
#[tokio::test]
async fn storage_page_uploading_filter_matches_the_upload_queue() {
    let (manager, temp_dir) = setup_test_manager().await;
    connect_test_cloud(&manager).await;

    let album_quiet = create_test_album();
    let release_quiet = create_test_release(&album_quiet.id);
    manager.database.insert_album(&album_quiet).await.unwrap();
    manager
        .database
        .insert_release(&release_quiet)
        .await
        .unwrap();

    let uploading = insert_release_with_queued_uploads(
        &manager,
        &temp_dir.path().join("uploading"),
        "Uploading Album",
        &[("a.flac", b"a-bytes")],
    )
    .await;

    let sort = crate::db::StorageSortCriterion {
        field: crate::db::StorageSortField::AlbumTitle,
        direction: crate::db::SortDirection::Ascending,
    };
    let page = manager
        .get_storage_page(&sort, crate::db::StorageFilter::Uploading, 0, 10)
        .await
        .unwrap();
    assert_eq!(page.total_count, 1);
    assert_eq!(page.rows.len(), 1);
    assert_eq!(page.rows[0].release.id, uploading.id);
    assert_eq!(
        manager
            .get_storage_count(crate::db::StorageFilter::Uploading)
            .await
            .unwrap(),
        1
    );
}

#[tokio::test]
async fn cancel_release_transition_fires_a_registered_transfer_token() {
    let (manager, _temp_dir) = setup_test_manager().await;

    // A registered unmanage token is fired by the unified cancel.
    let token = crate::library::CancellationToken::new();
    manager
        .transfer_cancels
        .lock()
        .unwrap()
        .insert(REL_X.to_string(), token.clone());
    manager.cancel_release_transition(REL_X).await.unwrap();
    assert!(token.is_cancelled(), "transfer token fired");

    // Nothing in progress for an unknown release → no-op, no error.
    manager.cancel_release_transition(REL_NONE).await.unwrap();
}

// Needs the test-utils mock cloud home: a Remote release implies a connected
// home, which the make-Local read storage is built over (the cancel fires
// before any blob is read, so the home is never actually called).
#[cfg(feature = "test-utils")]
#[tokio::test]
async fn unmanage_cancelled_before_copy_leaves_release_remote() {
    let (manager, temp_dir) = setup_test_manager().await;
    connect_test_cloud(&manager).await;
    // Really Remote, through the real transition: make-Local resolves the
    // release's current locality from coven, which a fabricated `remote` column
    // with no cloud objects behind it cannot answer.
    let release_id =
        make_remote_release(&manager, &temp_dir.path().join("r1"), "Album One", false).await;

    // A token cancelled before the materialize loop runs: coven aborts at the
    // first check, before reading/writing any blob, and never flips state. A
    // cancelled make-Local is a clean stop (Ok), not a failure.
    let token = crate::library::CancellationToken::new();
    token.cancel();
    let dest = temp_dir.path().join("out");
    manager
        .coven_make_local(&release_id, dest.to_str().unwrap(), &token)
        .await
        .expect("a cancelled make-Local ends cleanly");

    let after = manager
        .get_release_by_id(&release_id)
        .await
        .unwrap()
        .unwrap();
    assert!(
        after.remote,
        "cancelled make-Local leaves the release remote"
    );
}

/// The snapshot over a real make-Remote: queued, in flight, mid-file progress,
/// a genuine upload failure, and the cancel that empties the queue.
#[cfg(feature = "test-utils")]
#[tokio::test]
async fn outbox_snapshot_tracks_queued_active_failed_and_cancel() {
    let (manager, temp_dir) = setup_test_manager().await;
    connect_test_cloud(&manager).await;
    let source_dir = temp_dir.path().join("queued");
    let release = insert_release_with_queued_uploads(
        &manager,
        &source_dir,
        "Test Album",
        &[("a.flac", &vec![b'a'; 1000])],
    )
    .await;
    let file_id = manager
        .database
        .get_files_for_release(&release.id)
        .await
        .unwrap()[0]
        .id
        .clone();

    // Freshly queued: per-release count is 1 queued, joined to the album title.
    let snap = manager.outbox_snapshot().await.unwrap();
    assert_eq!(snap.total.queued, 1);
    assert_eq!(snap.total.active, 0);
    assert_eq!(snap.total.failed, 0);
    assert_eq!(snap.total.bytes_total, 1000);
    assert_eq!(snap.total.bytes_done, 0);
    assert_eq!(snap.upload_groups.len(), 1);
    let group = &snap.upload_groups[0];
    assert_eq!(group.display_title, "Test Album");
    assert_eq!(group.release_id.as_deref(), Some(release.id.as_str()));
    assert_eq!(group.files.len(), 1);
    assert_eq!(group.files[0].display_name, "a.flac");
    assert_eq!(group.progress.queued, 1);
    assert_eq!(group.progress.bytes_total, 1000);

    // In flight now: the in-memory map flips it to active, starting at zero
    // bytes done.
    manager
        .sync
        .outbox_in_flight()
        .lock()
        .unwrap()
        .insert(file_id.clone(), 0);
    let snap = manager.outbox_snapshot().await.unwrap();
    assert_eq!(snap.total.active, 1);
    assert_eq!(snap.total.queued, 0);
    assert_eq!(snap.upload_groups[0].progress.active, 1);
    assert_eq!(snap.total.bytes_done, 0);

    // Mid-upload progress advances the live byte count: the snapshot's
    // per-release and aggregate bytes_done climb without the file completing.
    manager
        .sync
        .outbox_in_flight()
        .lock()
        .unwrap()
        .insert(file_id.clone(), 400);
    let snap = manager.outbox_snapshot().await.unwrap();
    assert_eq!(snap.upload_groups[0].progress.active, 1);
    assert_eq!(snap.upload_groups[0].progress.bytes_done, 400);
    assert_eq!(snap.total.bytes_done, 400);
    assert_eq!(snap.total.bytes_total, 1000);
    manager
        .sync
        .outbox_in_flight()
        .lock()
        .unwrap()
        .remove(&file_id);

    // A real failure: the user's file is gone, so the drain cannot seal it. The
    // entry stays queued with coven's own attempt count and error on it.
    std::fs::remove_file(source_dir.join("a.flac")).unwrap();
    assert_eq!(
        manager.drain_uploads_expecting_work().await.unwrap(),
        0,
        "the entry was attempted and sealed nothing"
    );
    let snap = manager.outbox_snapshot().await.unwrap();
    assert_eq!(snap.total.failed, 1);
    assert_eq!(snap.total.queued, 0);
    assert_eq!(snap.upload_groups[0].progress.failed, 1);
    let queued = manager.database.handle().queued_uploads().await.unwrap();
    assert_eq!(queued[0].attempt_count, 1);
    assert!(
        queued[0].last_error.is_some(),
        "coven records why the attempt failed"
    );

    // Cancelling the release's make-Remote clears the queue; the snapshot empties.
    manager.cancel_release_upload(&release.id).await.unwrap();
    let snap = manager.outbox_snapshot().await.unwrap();
    assert!(snap.upload_groups.is_empty());
    assert_eq!(snap.total.failed, 0);
}

/// The real `ReleaseUploadObserver` drives the snapshot's live byte count:
/// `on_blob_upload_progress` advances an in-flight `Active` file's
/// `bytes_done` so the aggregate and per-release bars move mid-file.
#[cfg(feature = "test-utils")]
#[tokio::test]
async fn observer_progress_advances_snapshot_bytes_done() {
    use coven::BlobTransitionObserver;

    let (manager, temp_dir) = setup_test_manager().await;
    connect_test_cloud(&manager).await;
    let release = insert_release_with_queued_uploads(
        &manager,
        &temp_dir.path().join("queued"),
        "Test Album",
        &[("a.flac", &vec![b'a'; 1000])],
    )
    .await;
    let file_id = manager
        .database
        .get_files_for_release(&release.id)
        .await
        .unwrap()[0]
        .id
        .clone();

    // The observer shares the manager's in-flight map and throughput tracker,
    // exactly as production wires it in `build_sync_manager`.
    let observer = crate::sync::upload_observer::ReleaseUploadObserver::new(
        manager.sync.outbox_in_flight(),
        manager.sync.upload_sessions(),
        manager.sync.upload_throughput(),
        manager.sync.sync_paused(),
        manager.event_tx.clone(),
    );
    observer.set_database(Arc::new(manager.database.clone()));

    observer.on_blob_upload_started(&file_id).await;
    let snap = manager.outbox_snapshot().await.unwrap();
    assert_eq!(snap.upload_groups[0].progress.active, 1);
    assert_eq!(snap.upload_groups[0].progress.bytes_done, 0);
    assert_eq!(snap.total.bytes_done, 0);

    // A mid-upload progress report advances the live count without the file
    // completing.
    observer.on_blob_upload_progress(&file_id, 600, 1000).await;
    let snap = manager.outbox_snapshot().await.unwrap();
    assert_eq!(snap.upload_groups[0].progress.active, 1);
    assert_eq!(snap.upload_groups[0].progress.bytes_done, 600);
    assert_eq!(snap.total.bytes_done, 600);
    // The rolling-window tracker saw the 600-byte delta, so the rate is
    // non-zero before the file even finishes.
    assert!(manager.sync.upload_throughput().bytes_per_sec() > 0);

    // Completion clears the in-flight entry and tallies the file as done; the
    // queue entry is still there (this test drives only the observer, not
    // coven's drain), but with its only file shipped the release has nothing
    // left to render — the group leaves the snapshot.
    observer.on_blob_uploaded(&file_id).await;
    let snap = manager.outbox_snapshot().await.unwrap();
    assert_eq!(snap.total.active, 0);
    assert_eq!(snap.total.queued, 0);
    assert!(snap.upload_groups.is_empty());
}

/// A file that finished uploading but whose queue entry hasn't been consumed
/// yet (coven reports completion first, then clears the entry inside the
/// post-upload commit) must read as done work — never as freshly queued. The
/// Storage Manager renders whatever the last emitted snapshot says, so a
/// completed upload re-deriving as "Queued" is a lie the UI can freeze on.
#[cfg(feature = "test-utils")]
#[tokio::test]
async fn completed_upload_with_lingering_entry_is_not_queued() {
    use coven::BlobTransitionObserver;

    let (manager, temp_dir) = setup_test_manager().await;
    connect_test_cloud(&manager).await;
    let release = insert_release_with_queued_uploads(
        &manager,
        &temp_dir.path().join("queued"),
        "Test Album",
        &[("a.flac", &vec![b'a'; 1000])],
    )
    .await;
    let file_id = manager
        .database
        .get_files_for_release(&release.id)
        .await
        .unwrap()[0]
        .id
        .clone();

    let observer = crate::sync::upload_observer::ReleaseUploadObserver::new(
        manager.sync.outbox_in_flight(),
        manager.sync.upload_sessions(),
        manager.sync.upload_throughput(),
        manager.sync.sync_paused(),
        manager.event_tx.clone(),
    );
    observer.set_database(Arc::new(manager.database.clone()));

    observer.on_blob_upload_started(&file_id).await;
    observer.on_blob_upload_progress(&file_id, 1000, 1000).await;
    observer.on_blob_uploaded(&file_id).await;

    // The queue entry is still present — only coven's commit consumes it — but
    // the upload finished: nothing pending anywhere, and the release (its only
    // file shipped) is no longer rendered at all.
    let snap = manager.outbox_snapshot().await.unwrap();
    assert_eq!(
        snap.total.queued, 0,
        "a completed upload must not re-derive as queued"
    );
    assert_eq!(snap.total.active, 0);
    assert_eq!(snap.total.failed, 0);
    assert!(snap.upload_groups.is_empty());
}

/// Insert a remote, not-pinned release with one file and return its id.
/// `remote: true` + no pinned cache copy makes it eligible for pinning.
async fn insert_pinnable_release(manager: &LibraryManager) -> String {
    let album = create_test_album();
    let release = create_test_release(&album.id);
    manager.database.insert_album(&album).await.unwrap();
    manager.database.insert_release(&release).await.unwrap();
    let file = DbFile {
        id: bae_test_support::test_uuid(&format!("{}-file", release.id)),
        release_id: release.id.clone(),
        original_filename: "a.flac".to_string(),
        file_size: 1000,
        content_type: crate::util::content_type::ContentType::Flac,
        cloud_path: None,
        content_hash: crate::util::fs::hash_bytes(b"fixture"),
        created_at: Utc::now(),
    };
    manager.database.insert_file(&file).await.unwrap();
    release.id
}

/// Pausing before the first enqueue parks the worker, so the queue's
/// in-memory state (enqueue, dedup, snapshot counts, cancel) is observable
/// deterministically without the download path racing the assertions.
#[tokio::test]
async fn download_queue_enqueue_dedups_and_cancels_while_paused() {
    let (manager, _temp_dir) = setup_test_manager().await;
    let release_id = insert_pinnable_release(&manager).await;

    // Park the worker up front so nothing drains while we inspect state.
    manager.set_downloads_paused(true);

    manager.enqueue_pins(vec![release_id.clone()]).await;
    let snap = manager.download_snapshot();
    assert_eq!(snap.total.queued, 1);
    assert_eq!(snap.ops.len(), 1);
    assert_eq!(snap.ops[0].title, "Test Album");
    assert_eq!(snap.ops[0].file_count, 1);
    assert_eq!(snap.ops[0].state, crate::library::DownloadState::Queued);
    assert!(snap.paused);

    // Re-enqueuing the same release is a no-op: still one entry.
    manager.enqueue_pins(vec![release_id.clone()]).await;
    assert_eq!(manager.download_snapshot().ops.len(), 1);

    // Cancel drops the entry.
    manager.cancel_download(&release_id);
    let snap = manager.download_snapshot();
    assert!(snap.ops.is_empty());
}

/// An already-pinned release is skipped at enqueue rather than re-downloaded.
#[cfg(feature = "test-utils")]
#[tokio::test]
async fn download_queue_skips_already_pinned() {
    let (manager, temp_dir) = setup_test_manager().await;
    manager.set_downloads_paused(true);

    // A genuinely pinned release: made Remote with pin, so its blob lands in
    // coven's offline cache.
    connect_test_cloud(&manager).await;
    let release_id = make_remote_release(
        &manager,
        &temp_dir.path().join("pinned"),
        "Test Album",
        true,
    )
    .await;
    let summary = manager
        .find_release_storage_summary(&release_id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(
        summary.storage_state,
        crate::album_detail::ReleaseStorageState::Remote
    );
    assert!(
        summary.pinned,
        "the offline-cached blob makes it read as pinned"
    );

    manager.enqueue_pins(vec![release_id.clone()]).await;
    assert!(manager.download_snapshot().ops.is_empty());
}

/// A local (unmanaged) release has nothing to pin — it is already fully on disk —
/// so `enqueue_pins` skips it rather than queueing a download that would fail. The
/// album grid's bulk pin reaches this path with a mixed local/remote selection.
#[cfg(feature = "test-utils")]
#[tokio::test]
async fn download_queue_skips_local_release() {
    let (manager, temp_dir) = setup_test_manager().await;
    manager.set_downloads_paused(true);

    let release = insert_local_release_with_files(
        &manager,
        &temp_dir.path().join("local-source"),
        "Test Album",
        &[("a.flac", b"aaa")],
    )
    .await;
    let summary = manager
        .find_release_storage_summary(&release.id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(
        summary.storage_state,
        crate::album_detail::ReleaseStorageState::Local
    );

    manager.enqueue_pins(vec![release.id.clone()]).await;
    assert!(
        manager.download_snapshot().ops.is_empty(),
        "a local release is not enqueued for pinning"
    );
}

/// A pin that fails (no cloud home for a cloud-only release) lands `Failed`
/// and stays in the queue; `retry_downloads` flips it back to `Queued`.
#[tokio::test]
async fn download_queue_failed_pin_retries() {
    let (manager, _temp_dir) = setup_test_manager().await;
    let release_id = insert_pinnable_release(&manager).await;

    // No cloud home + no local copy ⇒ the pin can't read the file and fails.
    manager.enqueue_pins(vec![release_id.clone()]).await;

    // Let the worker pick it up, fail, and mark it Failed. Poll the snapshot
    // rather than sleeping a fixed interval.
    let failed = wait_for(|| {
        matches!(
            manager.download_snapshot().ops.first().map(|op| &op.state),
            Some(crate::library::DownloadState::Failed { .. })
        )
    })
    .await;
    assert!(failed, "the pin should land Failed without a cloud home");
    assert_eq!(manager.download_snapshot().total.failed, 1);

    // Retry flips it back to Queued; with no cloud home it'll fail again,
    // but the immediate post-retry state is Queued (or already re-failed).
    manager.retry_downloads();
    let snap = manager.download_snapshot();
    assert!(
        snap.ops.first().is_some_and(|op| matches!(
            op.state,
            crate::library::DownloadState::Queued
                | crate::library::DownloadState::Active { .. }
                | crate::library::DownloadState::Failed { .. }
        )),
        "after retry the release is still tracked"
    );

    // Cancelling clears it regardless of the in-flight retry.
    manager.cancel_download(&release_id);
    let cleared = wait_for(|| manager.download_snapshot().ops.is_empty()).await;
    assert!(cleared, "cancel removes the entry");
}

#[cfg(feature = "test-utils")]
#[tokio::test]
async fn download_queue_active_pin_reports_file_progress() {
    let (manager, temp_dir) = setup_test_manager().await;
    connect_test_cloud(&manager).await;
    let release_id = make_remote_release_with_files(
        &manager,
        &temp_dir.path().join("download-source"),
        "Test Album",
        &[("a.flac", b"aaa"), ("b.flac", b"bbbb")],
        false,
    )
    .await;
    let mut events = manager.subscribe_events();

    manager.enqueue_pins(vec![release_id.clone()]).await;

    // The release pins one blob at a time, so the pane sees the byte total climb:
    // 0, then the first file's 3 bytes, then both files' 7.
    let mut seen: Vec<u64> = Vec::new();
    for _ in 0..20 {
        let event = tokio::time::timeout(std::time::Duration::from_secs(2), events.recv())
            .await
            .expect("download queue event")
            .expect("event channel stays open");
        let LibraryEvent::DownloadQueueChanged { snapshot } = event else {
            continue;
        };
        for op in snapshot.ops {
            if op.release_id != release_id {
                continue;
            }
            if let crate::library::DownloadState::Active { progress } = op.state {
                assert_eq!(progress.bytes_total, 7, "the release's known byte total");
                assert_eq!(
                    progress.fraction,
                    progress.bytes_done as f64 / 7.0,
                    "the fraction tracks the bytes"
                );
                if seen.last() != Some(&progress.bytes_done) {
                    seen.push(progress.bytes_done);
                }
            }
        }
        if seen.contains(&7) {
            break;
        }
    }

    assert_eq!(
        seen,
        vec![0, 3, 7],
        "an active download reports each file's bytes as it lands, not just 0 and done",
    );
}

// ── Export queue ─────────────────────────────────────────────────

/// Create a Remote, pinned, exportable release: a Local release with
/// known-byte source files on disk (coven external refs) and a
/// `source_folder_name`, made Remote with pin so its blobs stay readable from
/// the offline cache. Returns its id. The manager must already be connected via
/// [`connect_test_cloud`].
#[cfg(feature = "test-utils")]
async fn make_exportable_release(
    manager: &LibraryManager,
    source_dir: &std::path::Path,
    folder_name: &str,
    files: &[(&str, &[u8])],
) -> String {
    let (release, db_files) = insert_export_release_rows(manager, folder_name, files).await;
    std::fs::create_dir_all(source_dir).unwrap();
    for (file, (_, bytes)) in db_files.iter().zip(files) {
        let path = source_dir.join(&file.original_filename);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, bytes).unwrap();
    }
    manager
        .database
        .register_release_external_refs_for_test(&release.id, &source_dir.to_string_lossy())
        .await
        .unwrap();
    manager.coven_make_remote(&release.id, true).await.unwrap();
    // The sync loop this fixture's tests connect drains the queue itself, so
    // wait for the make-Remote to finish rather than counting a drain pass this
    // test does not own.
    wait_for_landed_make_remote(manager, &release.id).await;
    release.id
}

/// Wait until a release's make-Remote is fully finished — not just uploaded.
///
/// The drain flips the gate, but coven holds each queue entry until the Store
/// write that publishes the transition activates, a cycle later. A test that
/// asserts "no upload work outstanding" has to be past that, or it reads the
/// transition's own leftovers as new work.
#[cfg(feature = "test-utils")]
async fn wait_for_settled_uploads(manager: &LibraryManager, release_id: &str) {
    for tick in 0..2_000 {
        if tick % 50 == 0 {
            manager.handle().sync_now();
        }
        if !manager
            .database
            .has_pending_uploads_for_release(release_id)
            .await
            .unwrap()
        {
            return;
        }
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
    }
    panic!("release {release_id} never finished its make-Remote");
}

#[cfg(feature = "test-utils")]
async fn insert_export_release_rows(
    manager: &LibraryManager,
    folder_name: &str,
    files: &[(&str, &[u8])],
) -> (DbRelease, Vec<DbFile>) {
    let album = create_test_album();
    let mut release = create_test_release(&album.id);
    release.remote = false;
    release.source_folder_name = Some(folder_name.to_string());
    manager.database.insert_album(&album).await.unwrap();
    manager.database.insert_release(&release).await.unwrap();
    let created_at = Utc::now();
    let mut inserted_files = Vec::with_capacity(files.len());
    for (index, (name, bytes)) in files.iter().enumerate() {
        let file = DbFile::new(
            &release.id,
            name,
            bytes.len() as i64,
            crate::util::content_type::ContentType::Flac,
            bae_test_support::test_uuid(&format!("{}-export-file-{index}", release.id)),
            created_at,
            crate::util::fs::hash_bytes(bytes),
        );
        manager.add_file(&file).await.unwrap();
        inserted_files.push(file);
    }
    (release, inserted_files)
}

/// A Local release with one readable source file, whose stored path fragments are
/// then overwritten with `poison` — the shape a row pulled from another device can
/// have. bae's own row-write refuses these values, but coven applies a pulled
/// changeset straight into SQLite, so the guard that matters is the one at the
/// join: the export copy-out and make-Local both validate the fragment before they
/// join it onto the user's folder.
#[cfg(feature = "test-utils")]
async fn insert_local_export_release_with_poisoned_fragment(
    manager: &LibraryManager,
    source_dir: &std::path::Path,
    bytes: &[u8],
    poison: PoisonedFragment<'_>,
) -> String {
    let (release, db_files) =
        insert_export_release_rows(manager, "Album Title", &[("track.flac", bytes)]).await;
    match poison {
        PoisonedFragment::OriginalFilename(value) => manager
            .database
            .set_original_filename_for_test(&db_files[0].id, value)
            .await
            .unwrap(),
        PoisonedFragment::SourceFolderName(value) => manager
            .database
            .set_source_folder_name_for_test(&release.id, value)
            .await
            .unwrap(),
    }
    std::fs::create_dir_all(source_dir).unwrap();
    let source_path = source_dir.join("source.flac");
    std::fs::write(&source_path, bytes).unwrap();
    manager
        .database
        .register_external_blob(
            crate::sync::RELEASE_FILES_NAMESPACE,
            &db_files[0].id,
            &source_path,
        )
        .await
        .unwrap();
    release.id
}

#[cfg(feature = "test-utils")]
enum PoisonedFragment<'a> {
    OriginalFilename(&'a str),
    SourceFolderName(&'a str),
}

/// Pausing before the first enqueue parks the worker, so the queue's in-memory
/// state (enqueue, dedup, target_dir, cancel) is observable deterministically
/// without the export path racing the assertions.
#[tokio::test]
async fn output_queue_enqueue_dedups_and_cancels_while_paused() {
    let (manager, temp_dir) = setup_test_manager().await;
    let release_id = insert_pinnable_release(&manager).await;

    manager.set_outputs_paused(true);

    let target = temp_dir.path().join("export-out");
    manager
        .enqueue_export(&release_id, target.clone())
        .await
        .unwrap();
    let snap = manager.output_snapshot();
    assert_eq!(snap.total.queued, 1);
    assert_eq!(snap.ops.len(), 1);
    assert_eq!(snap.ops[0].title, "Test Album");
    assert_eq!(snap.ops[0].file_count, 1);
    assert_eq!(snap.ops[0].payload.target_dir, target);
    assert_eq!(snap.ops[0].state, crate::library::OutputState::Queued);
    assert!(snap.paused);

    // Re-enqueuing the same release is a no-op: still one entry.
    manager
        .enqueue_export(&release_id, target.clone())
        .await
        .unwrap();
    assert_eq!(manager.output_snapshot().ops.len(), 1);

    // Cancel drops the entry.
    manager.cancel_output(&release_id);
    assert!(manager.output_snapshot().ops.is_empty());
}

/// The verbatim copy-out: exported bytes equal the source bytes, laid out at
/// `<target>/<source_folder_name>/<original_filename>` (including nested
/// subfolders), and the export changes no release state — it stays Remote with
/// no new cloud-outbox rows.
#[cfg(feature = "test-utils")]
#[tokio::test]
async fn export_writes_exact_bytes_in_source_folder_and_leaves_release_remote() {
    let (manager, temp_dir) = setup_test_manager().await;
    connect_test_cloud_with_sync_loop(&manager).await;

    let source_dir = temp_dir.path().join("source");
    let files: &[(&str, &[u8])] = &[
        ("cover.jpg", b"cover-bytes-abc"),
        ("CD1/track.flac", b"flac-bytes-0123456789"),
    ];
    let release_id =
        make_exportable_release(&manager, &source_dir, "Album Title (2020)", files).await;

    // Precondition: Remote, no pending uploads after the drain.
    let before = manager
        .get_release_by_id(&release_id)
        .await
        .unwrap()
        .unwrap();
    assert!(before.remote);
    assert!(!manager
        .database
        .has_pending_uploads_for_release(&release_id)
        .await
        .unwrap());

    let target = temp_dir.path().join("export-out");
    manager
        .enqueue_export(&release_id, target.clone())
        .await
        .unwrap();

    // Success removes the entry from the queue.
    let done = wait_for(|| manager.output_snapshot().ops.is_empty()).await;
    assert!(done, "the export should complete and clear the queue");

    // Byte-accuracy + folder layout.
    for (name, bytes) in files {
        let written = target.join("Album Title (2020)").join(name);
        let got = std::fs::read(&written).unwrap_or_else(|e| panic!("read exported {name}: {e}"));
        assert_eq!(&got, bytes, "exported bytes for {name} match the source");
    }

    // The staging directory was renamed into place, leaving nothing behind it.
    let leftover = std::fs::read_dir(&target)
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
        .collect::<Vec<_>>();
    assert_eq!(
        leftover,
        vec!["Album Title (2020)".to_string()],
        "only the final export folder remains under the target; no staging dir"
    );

    // Export changed no release state: still Remote, no new outbox rows.
    let after = manager
        .get_release_by_id(&release_id)
        .await
        .unwrap()
        .unwrap();
    assert!(after.remote, "export leaves the release Remote");
    assert!(
        !manager
            .database
            .has_pending_uploads_for_release(&release_id)
            .await
            .unwrap(),
        "export enqueues no cloud uploads"
    );
}

#[cfg(feature = "test-utils")]
#[tokio::test]
async fn export_rejects_parent_component_original_filename() {
    let (manager, temp_dir) = setup_test_manager().await;
    let release_id = insert_local_export_release_with_poisoned_fragment(
        &manager,
        &temp_dir.path().join("source"),
        b"escape-bytes",
        PoisonedFragment::OriginalFilename("../escape.flac"),
    )
    .await;

    assert_export_rejects_invalid_path(
        &manager,
        &release_id,
        temp_dir.path(),
        "parent component in original_filename is rejected",
        &["export-out/escape.flac", "export-out/Album Title"],
    )
    .await;
}

#[cfg(feature = "test-utils")]
#[tokio::test]
async fn export_rejects_absolute_original_filename() {
    let (manager, temp_dir) = setup_test_manager().await;
    let absolute_escape = temp_dir.path().join("absolute-escape.flac");
    let release_id = insert_local_export_release_with_poisoned_fragment(
        &manager,
        &temp_dir.path().join("source"),
        b"escape-bytes",
        PoisonedFragment::OriginalFilename(absolute_escape.to_str().unwrap()),
    )
    .await;

    assert_export_rejects_invalid_path(
        &manager,
        &release_id,
        temp_dir.path(),
        "absolute original_filename is rejected",
        &["absolute-escape.flac", "export-out/Album Title"],
    )
    .await;
}

#[cfg(feature = "test-utils")]
#[tokio::test]
async fn export_rejects_parent_component_source_folder_name() {
    let (manager, temp_dir) = setup_test_manager().await;
    let release_id = insert_local_export_release_with_poisoned_fragment(
        &manager,
        &temp_dir.path().join("source"),
        b"track-bytes",
        PoisonedFragment::SourceFolderName("../escape-folder"),
    )
    .await;

    assert_export_rejects_invalid_path(
        &manager,
        &release_id,
        temp_dir.path(),
        "parent component in source_folder_name is rejected",
        &["escape-folder", "export-out"],
    )
    .await;
}

/// make-Local hands coven a map of blob id → local destination, built by joining
/// each file's stored `original_filename` onto the folder the user picked. coven
/// writes wherever that map points, so a `../` in a row another device wrote would
/// materialize the release's bytes outside the chosen folder. The join refuses it,
/// and refuses the whole release: no destination in the map may escape the target.
#[cfg(feature = "test-utils")]
#[tokio::test]
async fn make_local_dest_rejects_a_traversing_original_filename() {
    let (manager, temp_dir) = setup_test_manager().await;
    let target = temp_dir.path().join("make-local-out");
    let release_id = insert_local_export_release_with_poisoned_fragment(
        &manager,
        &temp_dir.path().join("source"),
        b"escape-bytes",
        PoisonedFragment::OriginalFilename("../escape.flac"),
    )
    .await;

    let error = manager
        .make_local_dest(&release_id, target.to_str().unwrap())
        .await
        .expect_err("a traversing original_filename must not produce a destination");
    assert!(
        error.to_string().contains("invalid path fragment"),
        "unexpected error: {error}",
    );
    assert!(
        !temp_dir.path().join("escape.flac").exists(),
        "nothing is written outside the target folder",
    );
}

#[cfg(feature = "test-utils")]
async fn assert_export_rejects_invalid_path(
    manager: &LibraryManager,
    release_id: &str,
    temp_dir: &std::path::Path,
    message: &str,
    absent_paths: &[&str],
) {
    let target = temp_dir.join("export-out");
    let error = manager
        .export_release(release_id, &target, crate::library::OutputKind::Export)
        .await
        .expect_err(message);

    assert!(
        error.to_string().contains("invalid path fragment"),
        "unexpected error: {error}"
    );
    for path in absent_paths {
        assert!(
            !temp_dir.join(path).exists(),
            "invalid export wrote {}",
            temp_dir.join(path).display()
        );
    }
}

/// A write error (an unwritable target) marks the export `Failed` with a message
/// and keeps it in the queue; `retry_outputs` flips it back to `Queued`.
#[cfg(feature = "test-utils")]
#[tokio::test]
async fn export_write_error_marks_failed_and_retries() {
    let (manager, temp_dir) = setup_test_manager().await;
    connect_test_cloud_with_sync_loop(&manager).await;

    let source_dir = temp_dir.path().join("source");
    let release_id = make_exportable_release(
        &manager,
        &source_dir,
        "Album Title",
        &[("track.flac", b"track-bytes")],
    )
    .await;

    // Target a path that is actually a file, so creating the release subfolder
    // under it fails with an I/O error (the read succeeds; the write doesn't).
    let blocker = temp_dir.path().join("blocker");
    std::fs::write(&blocker, b"a file, not a directory").unwrap();

    manager
        .enqueue_export(&release_id, blocker.clone())
        .await
        .unwrap();

    let failed = wait_for(|| {
        matches!(
            manager.output_snapshot().ops.first().map(|op| &op.state),
            Some(crate::library::OutputState::Failed { .. })
        )
    })
    .await;
    assert!(failed, "an unwritable target marks the export Failed");
    assert_eq!(manager.output_snapshot().total.failed, 1);

    // Retry flips it back to Queued (it'll fail again, but stays tracked).
    manager.retry_outputs();
    assert!(manager
        .output_snapshot()
        .ops
        .first()
        .is_some_and(|op| matches!(
            op.state,
            crate::library::OutputState::Queued
                | crate::library::OutputState::Active { .. }
                | crate::library::OutputState::Failed { .. }
        )));

    manager.cancel_output(&release_id);
    let cleared = wait_for(|| manager.output_snapshot().ops.is_empty()).await;
    assert!(cleared, "cancel removes the entry");
}

/// A failure partway through the export (a read error on a later file, after an
/// earlier file has already been written to staging) leaves NO output at the
/// final `<target>/<source_folder_name>/` path — the staging directory is
/// removed, so the export is all-or-nothing.
#[cfg(feature = "test-utils")]
#[tokio::test]
async fn export_mid_failure_leaves_no_partial_output_at_final_path() {
    let (manager, temp_dir) = setup_test_manager().await;

    // A Local release whose two source files live on disk as coven external refs
    // (UserProvided reads straight from the user's own file). Keeping it Local —
    // never made Remote — means the export reads these files directly, so removing
    // one forces a read error on a later file mid-export.
    let source_dir = temp_dir.path().join("source");
    std::fs::create_dir_all(&source_dir).unwrap();
    let album = create_test_album();
    let mut release = create_test_release(&album.id);
    release.remote = false;
    release.source_folder_name = Some("Album Title".to_string());
    manager.database.insert_album(&album).await.unwrap();
    manager.database.insert_release(&release).await.unwrap();

    let files: &[(&str, &[u8])] = &[("01.flac", b"first-ok"), ("02.flac", b"second-fails")];
    for (name, bytes) in files {
        std::fs::write(source_dir.join(name), bytes).unwrap();
        let file = DbFile::new(
            &release.id,
            name,
            bytes.len() as i64,
            crate::util::content_type::ContentType::Flac,
            Uuid::new_v4().to_string(),
            Utc::now(),
            crate::util::fs::hash_bytes(bytes),
        );
        manager.add_file(&file).await.unwrap();
    }
    manager
        .database
        .register_release_external_refs_for_test(&release.id, &source_dir.to_string_lossy())
        .await
        .unwrap();

    // Pause so the source file can be deleted before the worker runs, making the
    // later read fail deterministically rather than racing the copy.
    manager.set_outputs_paused(true);
    let target = temp_dir.path().join("export-out");
    std::fs::create_dir_all(&target).unwrap();
    manager
        .enqueue_export(&release.id, target.clone())
        .await
        .unwrap();
    std::fs::remove_file(source_dir.join("02.flac")).unwrap();
    manager.set_outputs_paused(false);

    let failed = wait_for(|| {
        matches!(
            manager.output_snapshot().ops.first().map(|op| &op.state),
            Some(crate::library::OutputState::Failed { .. })
        )
    })
    .await;
    assert!(
        failed,
        "a read error on a later file marks the export Failed"
    );

    // The all-or-nothing guarantee: nothing at the final path, and the staging
    // directory was removed — the target holds no export output at all.
    assert!(
        !target.join("Album Title").exists(),
        "no partial output at the final export path"
    );
    assert_eq!(
        std::fs::read_dir(&target).unwrap().count(),
        0,
        "staging directory cleaned up; target left empty"
    );
}

/// Poll `predicate` up to ~2s (40 × 50ms), returning whether it became true.
/// Used by the async download-worker tests instead of a fixed sleep.
async fn wait_for(predicate: impl Fn() -> bool) -> bool {
    for _ in 0..40 {
        if predicate() {
            return true;
        }
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    }
    predicate()
}

/// Two release ids whose lexical order is fixed, for the `r.id` tiebreaker.
const TIEBREAK_LO: &str = "066483e0-f9fc-4636-865d-08c069510b2e";
const TIEBREAK_HI: &str = "2153cb27-8335-4523-ae52-be2d6f577ba3";

#[tokio::test]
async fn storage_page_id_tiebreaker_stable_across_pages() {
    let (manager, _temp_dir) = setup_test_manager().await;

    // Two releases sharing album title + created_at — the ORDER BY clause
    // falls through to the `r.id` tiebreaker. The ids are canonical UUIDs
    // (coven takes no other shape on a synced row) chosen so LO sorts first.
    let now = Utc::now();
    let mut album = create_test_album();
    album.title = "Same Title".to_string();
    manager.database.insert_album(&album).await.unwrap();
    let mut release_a = create_test_release(&album.id);
    release_a.id = TIEBREAK_LO.to_string();
    release_a.created_at = now;
    let mut release_b = create_test_release(&album.id);
    release_b.id = TIEBREAK_HI.to_string();
    release_b.created_at = now;
    manager.database.insert_release(&release_a).await.unwrap();
    manager.database.insert_release(&release_b).await.unwrap();

    let sort = crate::db::StorageSortCriterion {
        field: crate::db::StorageSortField::AlbumTitle,
        direction: crate::db::SortDirection::Ascending,
    };
    let first_page = manager
        .get_storage_page(&sort, crate::db::StorageFilter::All, 0, 1)
        .await
        .unwrap();
    let second_page = manager
        .get_storage_page(&sort, crate::db::StorageFilter::All, 1, 1)
        .await
        .unwrap();

    assert_eq!(first_page.rows.len(), 1);
    assert_eq!(second_page.rows.len(), 1);
    assert_eq!(first_page.rows[0].release.id, TIEBREAK_LO);
    assert_eq!(second_page.rows[0].release.id, TIEBREAK_HI);
}

// ── set_identity ───────────────────────────────────────────────────

fn mb_identity(group: &str, release: Option<&str>) -> crate::import::ReleaseIdentity {
    crate::import::ReleaseIdentity {
        source: crate::import::MetadataSource::MusicBrainz,
        source_group_id: group.to_string(),
        source_release_id: release.map(|s| s.to_string()),
    }
}

fn discogs_identity(group: &str, release: Option<&str>) -> crate::import::ReleaseIdentity {
    crate::import::ReleaseIdentity {
        source: crate::import::MetadataSource::Discogs,
        source_group_id: group.to_string(),
        source_release_id: release.map(|s| s.to_string()),
    }
}

#[tokio::test]
async fn set_identity_to_unknown_moves_release_to_fresh_album() {
    let (manager, _temp_dir) = setup_test_manager().await;

    let album = create_test_album();
    let mut release = create_test_release(&album.id);
    release.metadata_source = crate::db::ReleaseMetadataSource::MusicBrainz;
    release.metadata_source_release_id = Some("mb-rel-1".to_string());

    manager.database.insert_album(&album).await.unwrap();
    manager.database.insert_release(&release).await.unwrap();
    manager
        .database
        .insert_release_identities(&release.id, &[mb_identity("g1", Some("mb-rel-1"))])
        .await
        .unwrap();

    manager
        .set_identity(
            &release.id,
            vec![],
            crate::import::MetadataPointer::FileTags,
            &[],
        )
        .await
        .unwrap();

    // The original album was a one-release album → deleted now.
    assert!(manager
        .database
        .find_album_by_id(&album.id)
        .await
        .unwrap()
        .is_none());

    // Release moved to a brand-new album, holds nothing else.
    let new_album_id = manager
        .database
        .find_album_id_for_release(&release.id)
        .await
        .unwrap()
        .unwrap();
    assert_ne!(new_album_id, album.id);
    let siblings = manager
        .database
        .get_releases_for_album(&new_album_id)
        .await
        .unwrap();
    assert_eq!(siblings.len(), 1);
    assert_eq!(siblings[0].id, release.id);

    // Identity rows wiped, metadata source flipped to file_tags.
    let identities = manager
        .database
        .get_release_identities(&release.id)
        .await
        .unwrap();
    assert!(identities.is_empty());
    let updated = manager
        .database
        .find_release_by_id(&release.id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(
        updated.metadata_source,
        crate::db::ReleaseMetadataSource::FileTags
    );
    assert_eq!(updated.metadata_source_release_id, None);
}

#[tokio::test]
async fn set_identity_replaces_rows_when_new_identity_fits_current_album() {
    let (manager, _temp_dir) = setup_test_manager().await;

    // Album has two releases, both Approximate-MB on group g1.
    let album = create_test_album();
    let release1 = create_test_release(&album.id);
    let release2 = create_test_release(&album.id);
    manager.database.insert_album(&album).await.unwrap();
    manager.database.insert_release(&release1).await.unwrap();
    manager.database.insert_release(&release2).await.unwrap();
    manager
        .database
        .insert_release_identities(&release1.id, &[mb_identity("g1", None)])
        .await
        .unwrap();
    manager
        .database
        .insert_release_identities(&release2.id, &[mb_identity("g1", None)])
        .await
        .unwrap();

    // Promote release1 from Approximate to Exact within g1. New row
    // still agrees with release2's group, so release1 stays put.
    manager
        .set_identity(
            &release1.id,
            vec![mb_identity("g1", Some("mb-rel-99"))],
            crate::import::MetadataPointer::External {
                source: crate::import::MetadataSource::MusicBrainz,
                release_id: "mb-rel-99".to_string(),
            },
            &[],
        )
        .await
        .unwrap();

    let landing_album_id = manager
        .database
        .find_album_id_for_release(&release1.id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(landing_album_id, album.id);

    let identities = manager
        .database
        .get_release_identities(&release1.id)
        .await
        .unwrap();
    assert_eq!(identities.len(), 1);
    assert_eq!(identities[0].source_group_id, "g1");
    assert_eq!(
        identities[0].source_release_id.as_deref(),
        Some("mb-rel-99")
    );

    let updated = manager
        .database
        .find_release_by_id(&release1.id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(
        updated.metadata_source,
        crate::db::ReleaseMetadataSource::MusicBrainz
    );
    assert_eq!(
        updated.metadata_source_release_id.as_deref(),
        Some("mb-rel-99"),
    );

    // Source album still holds both releases.
    let siblings = manager
        .database
        .get_releases_for_album(&album.id)
        .await
        .unwrap();
    assert_eq!(siblings.len(), 2);
}

#[tokio::test]
async fn set_identity_creates_new_album_when_no_existing_album_fits() {
    let (manager, _temp_dir) = setup_test_manager().await;

    // Two albums, neither matching the new MB group g2.
    let album_a = create_test_album();
    let mut album_b = create_test_album();
    album_b.title = "Other Album".to_string();
    manager.database.insert_album(&album_a).await.unwrap();
    manager.database.insert_album(&album_b).await.unwrap();

    let release_alpha = create_test_release(&album_a.id);
    let release_beta = create_test_release(&album_a.id);
    let release_other = create_test_release(&album_b.id);
    manager
        .database
        .insert_release(&release_alpha)
        .await
        .unwrap();
    manager
        .database
        .insert_release(&release_beta)
        .await
        .unwrap();
    manager
        .database
        .insert_release(&release_other)
        .await
        .unwrap();
    manager
        .database
        .insert_release_identities(&release_alpha.id, &[mb_identity("g1", None)])
        .await
        .unwrap();
    manager
        .database
        .insert_release_identities(&release_beta.id, &[mb_identity("g1", None)])
        .await
        .unwrap();
    manager
        .database
        .insert_release_identities(&release_other.id, &[mb_identity("g3", None)])
        .await
        .unwrap();

    // release_alpha takes on a brand-new MB group (g2). Its current
    // album (album_a) holds release_beta on g1, so it can't stay.
    // No other album holds g2 either → fresh album.
    manager
        .set_identity(
            &release_alpha.id,
            vec![mb_identity("g2", None)],
            crate::import::MetadataPointer::External {
                source: crate::import::MetadataSource::MusicBrainz,
                release_id: "mb-rel-g2".to_string(),
            },
            &[],
        )
        .await
        .unwrap();

    let landing_album_id = manager
        .database
        .find_album_id_for_release(&release_alpha.id)
        .await
        .unwrap()
        .unwrap();
    assert_ne!(landing_album_id, album_a.id);
    assert_ne!(landing_album_id, album_b.id);

    // Source album loses release_alpha but keeps release_beta.
    let source_siblings = manager
        .database
        .get_releases_for_album(&album_a.id)
        .await
        .unwrap();
    assert_eq!(source_siblings.len(), 1);
    assert_eq!(source_siblings[0].id, release_beta.id);
}

#[tokio::test]
async fn set_identity_moves_release_to_matching_album() {
    let (manager, _temp_dir) = setup_test_manager().await;

    // Source album (album_a) carries release_alpha solo at MB g1.
    // Target album (album_b) carries release_other at MB g2 — that
    // matches the new identity we'll set on release_alpha.
    let album_a = create_test_album();
    let mut album_b = create_test_album();
    album_b.title = "Other Album".to_string();
    manager.database.insert_album(&album_a).await.unwrap();
    manager.database.insert_album(&album_b).await.unwrap();

    let release_alpha = create_test_release(&album_a.id);
    let release_other = create_test_release(&album_b.id);
    manager
        .database
        .insert_release(&release_alpha)
        .await
        .unwrap();
    manager
        .database
        .insert_release(&release_other)
        .await
        .unwrap();
    manager
        .database
        .insert_release_identities(&release_alpha.id, &[mb_identity("g1", None)])
        .await
        .unwrap();
    manager
        .database
        .insert_release_identities(&release_other.id, &[mb_identity("g2", None)])
        .await
        .unwrap();

    manager
        .set_identity(
            &release_alpha.id,
            vec![mb_identity("g2", Some("mb-rel-pressing"))],
            crate::import::MetadataPointer::External {
                source: crate::import::MetadataSource::MusicBrainz,
                release_id: "mb-rel-pressing".to_string(),
            },
            &[],
        )
        .await
        .unwrap();

    // release_alpha now lives in album_b alongside release_other.
    let landing_album_id = manager
        .database
        .find_album_id_for_release(&release_alpha.id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(landing_album_id, album_b.id);
    let target_siblings = manager
        .database
        .get_releases_for_album(&album_b.id)
        .await
        .unwrap();
    assert_eq!(target_siblings.len(), 2);

    // album_a was a single-release album → deleted now.
    assert!(manager
        .database
        .find_album_by_id(&album_a.id)
        .await
        .unwrap()
        .is_none());
}

#[tokio::test]
async fn set_identity_keeps_vacated_album_when_other_releases_remain() {
    let (manager, _temp_dir) = setup_test_manager().await;

    // album_a holds two releases, both on MB g1. Move release_alpha
    // out by giving it a different group.
    let album_a = create_test_album();
    manager.database.insert_album(&album_a).await.unwrap();
    let release_alpha = create_test_release(&album_a.id);
    let release_beta = create_test_release(&album_a.id);
    manager
        .database
        .insert_release(&release_alpha)
        .await
        .unwrap();
    manager
        .database
        .insert_release(&release_beta)
        .await
        .unwrap();
    manager
        .database
        .insert_release_identities(&release_alpha.id, &[mb_identity("g1", None)])
        .await
        .unwrap();
    manager
        .database
        .insert_release_identities(&release_beta.id, &[mb_identity("g1", None)])
        .await
        .unwrap();

    manager
        .set_identity(
            &release_alpha.id,
            vec![mb_identity("g2", None)],
            crate::import::MetadataPointer::External {
                source: crate::import::MetadataSource::MusicBrainz,
                release_id: "mb-rel-g2".to_string(),
            },
            &[],
        )
        .await
        .unwrap();

    // album_a still exists, holds release_beta only.
    let surviving = manager
        .database
        .find_album_by_id(&album_a.id)
        .await
        .unwrap();
    assert!(surviving.is_some());
    let surviving_releases = manager
        .database
        .get_releases_for_album(&album_a.id)
        .await
        .unwrap();
    assert_eq!(surviving_releases.len(), 1);
    assert_eq!(surviving_releases[0].id, release_beta.id);
}

#[tokio::test]
async fn set_identity_does_not_touch_metadata_columns() {
    let (manager, _temp_dir) = setup_test_manager().await;

    let mut album = create_test_album();
    album.title = "Initial Title".to_string();
    album.year = Some(1999);
    manager.database.insert_album(&album).await.unwrap();

    let mut release = create_test_release(&album.id);
    release.pressing.format = Some("Vinyl".to_string());
    release.pressing.label = Some("My Label".to_string());
    release.pressing.catalog_number = Some("CAT-123".to_string());
    release.pressing.country = Some("US".to_string());
    release.pressing.barcode = Some("1234567890".to_string());
    release.pressing.year = Some(1999);
    manager.database.insert_release(&release).await.unwrap();
    manager
        .database
        .insert_release_identities(&release.id, &[mb_identity("g1", Some("mb-rel-1"))])
        .await
        .unwrap();

    // Insert a track too — we want to verify it survives.
    let track = crate::db::DbTrack {
        id: Uuid::new_v4().to_string(),
        release_id: release.id.clone(),
        title: "My Track".to_string(),
        side: 1,
        track_number: Some(1),
        duration_ms: Some(180_000),
        discogs_position: None,
        created_at: Utc::now(),
    };
    manager.database.insert_track(&track).await.unwrap();

    manager
        .set_identity(
            &release.id,
            vec![discogs_identity("dg1", Some("dg-rel-1"))],
            crate::import::MetadataPointer::External {
                source: crate::import::MetadataSource::Discogs,
                release_id: "dg-rel-1".to_string(),
            },
            &[],
        )
        .await
        .unwrap();

    // Pressing fields untouched.
    let after = manager
        .database
        .find_release_by_id(&release.id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(after.pressing.format.as_deref(), Some("Vinyl"));
    assert_eq!(after.pressing.label.as_deref(), Some("My Label"));
    assert_eq!(after.pressing.catalog_number.as_deref(), Some("CAT-123"));
    assert_eq!(after.pressing.country.as_deref(), Some("US"));
    assert_eq!(after.pressing.barcode.as_deref(), Some("1234567890"));
    assert_eq!(after.pressing.year, Some(1999));

    // Album-level fields untouched (still in the same album, since
    // both old and new identities are in the only release).
    let after_album = manager
        .database
        .find_album_by_id(&after.album_id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(after_album.title, "Initial Title");
    assert_eq!(after_album.year, Some(1999));

    // Track survived.
    let tracks = manager
        .database
        .get_tracks_for_release(&release.id)
        .await
        .unwrap();
    assert_eq!(tracks.len(), 1);
    assert_eq!(tracks[0].title, "My Track");
}

#[tokio::test]
async fn set_identity_to_fresh_album_preserves_album_artists() {
    let (manager, _temp_dir) = setup_test_manager().await;

    // Two extra artists so the album carries multiple album_artists
    // rows beyond the primary (which lives on `albums.artist_id`).
    let primary = DbArtist {
        id: "755ab566-9e71-4a7f-88df-fc5f573f882f".to_string(),
        name: "Primary".to_string(),
        sort_name: None,
        discogs_artist_id: None,
        musicbrainz_artist_id: None,
        created_at: Utc::now(),
    };
    let secondary = DbArtist {
        id: "1d4f0221-7e2b-4e87-8376-93eaf8998bd7".to_string(),
        name: "Secondary".to_string(),
        sort_name: None,
        discogs_artist_id: None,
        musicbrainz_artist_id: None,
        created_at: Utc::now(),
    };
    manager.database.insert_artist(&primary).await.unwrap();
    manager.database.insert_artist(&secondary).await.unwrap();

    // album_a holds release_alpha and release_beta on g1, with both
    // primary + secondary as album artists. We're going to move
    // release_alpha out via a non-fitting identity, forcing the
    // creation of a fresh album.
    let mut album_a = create_test_album();
    album_a.artist_id = primary.id.clone();
    manager.database.insert_album(&album_a).await.unwrap();
    manager
        .database
        .insert_album_artist(&DbAlbumArtist::new(
            &album_a.id,
            &primary.id,
            0,
            Uuid::new_v4().to_string(),
            Utc::now(),
        ))
        .await
        .unwrap();
    manager
        .database
        .insert_album_artist(&DbAlbumArtist::new(
            &album_a.id,
            &secondary.id,
            1,
            Uuid::new_v4().to_string(),
            Utc::now(),
        ))
        .await
        .unwrap();

    let release_alpha = create_test_release(&album_a.id);
    let release_beta = create_test_release(&album_a.id);
    manager
        .database
        .insert_release(&release_alpha)
        .await
        .unwrap();
    manager
        .database
        .insert_release(&release_beta)
        .await
        .unwrap();
    manager
        .database
        .insert_release_identities(&release_alpha.id, &[mb_identity("g1", None)])
        .await
        .unwrap();
    manager
        .database
        .insert_release_identities(&release_beta.id, &[mb_identity("g1", None)])
        .await
        .unwrap();

    // release_alpha takes a different group → can't stay in album_a
    // (g1 disagrees with g2), no other album holds g2 → fresh album.
    manager
        .set_identity(
            &release_alpha.id,
            vec![mb_identity("g2", None)],
            crate::import::MetadataPointer::External {
                source: crate::import::MetadataSource::MusicBrainz,
                release_id: "mb-rel-g2".to_string(),
            },
            &[],
        )
        .await
        .unwrap();

    let new_album_id = manager
        .database
        .find_album_id_for_release(&release_alpha.id)
        .await
        .unwrap()
        .unwrap();
    assert_ne!(new_album_id, album_a.id);

    // The fresh album carries the same album_artists as the source.
    // get_artists_for_album joins both the primary (via albums.artist_id)
    // and album_artists rows, ordered by position.
    let new_album_artists = manager
        .database
        .get_artists_for_album(&new_album_id)
        .await
        .unwrap();
    let names: Vec<&str> = new_album_artists.iter().map(|a| a.name.as_str()).collect();
    assert_eq!(names, vec!["Primary", "Primary", "Secondary"]);
}

#[tokio::test]
async fn set_identity_clears_primary_when_it_pointed_at_moved_release() {
    let (manager, _temp_dir) = setup_test_manager().await;
    let now = Utc::now();
    let beta_track_id = TRACK_BETA.to_string();

    // album_a carries two releases on g1 and points
    // primary_release_id at release_alpha. Move release_alpha out.
    // The chosen release is gone, so primary_release_id becomes NULL
    // and the read path falls back to the remaining release_beta.
    let album_a = create_test_album();
    manager.database.insert_album(&album_a).await.unwrap();
    let release_alpha = create_test_release(&album_a.id);
    let release_beta = create_test_release(&album_a.id);

    manager
        .database
        .insert_release(&release_alpha)
        .await
        .unwrap();
    manager
        .database
        .insert_release(&release_beta)
        .await
        .unwrap();

    // A track on release_beta so the read-path resolution below has an
    // identifiable target: the fallback should surface beta's tracks.
    let beta_track = crate::db::DbTrack {
        id: beta_track_id.clone(),
        release_id: release_beta.id.clone(),
        title: "Track Title".to_string(),
        side: 1,
        track_number: Some(1),
        duration_ms: Some(180_000),
        discogs_position: None,
        created_at: now,
    };
    manager.database.insert_track(&beta_track).await.unwrap();

    // Point album_a.primary_release_id at release_alpha — the
    // release we're about to move out.
    manager
        .database
        .set_album_primary_release(&album_a.id, &release_alpha.id)
        .await
        .unwrap();

    manager
        .database
        .insert_release_identities(&release_alpha.id, &[mb_identity("g1", None)])
        .await
        .unwrap();
    manager
        .database
        .insert_release_identities(&release_beta.id, &[mb_identity("g1", None)])
        .await
        .unwrap();

    manager
        .set_identity(
            &release_alpha.id,
            vec![mb_identity("g2", None)],
            crate::import::MetadataPointer::External {
                source: crate::import::MetadataSource::MusicBrainz,
                release_id: "mb-rel-g2".to_string(),
            },
            &[],
        )
        .await
        .unwrap();

    // album_a survives; its primary_release_id is cleared to NULL now
    // that the release it pointed at has left.
    let surviving_album = manager
        .database
        .find_album_by_id(&album_a.id)
        .await
        .unwrap()
        .expect("album should still exist with release_beta");
    assert_eq!(
        surviving_album.primary_release_id, None,
        "primary_release_id should be cleared when its release moves out",
    );

    // Read path falls back to the first remaining release: album_a now
    // resolves its primary to release_beta.
    let resolved_track_ids = manager
        .database
        .get_primary_release_track_ids_for_album(&album_a.id)
        .await
        .unwrap();
    assert_eq!(
        resolved_track_ids,
        Some(vec![beta_track_id.clone()]),
        "read path should resolve the cleared primary to release_beta",
    );
}

#[tokio::test]
async fn set_identity_atomic_rechecks_source_count_inside_transaction() {
    // The TOCTOU window: a separate writer lands a release into the source album
    // between `set_identity`'s pre-flight read and its atomic call. Drive the atomic
    // API directly with `current_album_id` at the source album, after seeding an
    // extra release into it. The atomic call must NOT delete the source — its
    // in-transaction recheck sees the surviving release.
    let (manager, _temp_dir) = setup_test_manager().await;

    let album_a = create_test_album();
    manager.database.insert_album(&album_a).await.unwrap();

    let release_alpha = create_test_release(&album_a.id);
    manager
        .database
        .insert_release(&release_alpha)
        .await
        .unwrap();
    manager
        .database
        .insert_release_identities(&release_alpha.id, &[mb_identity("g1", None)])
        .await
        .unwrap();

    // Build the fresh-album row the manager would have produced —
    // we're driving the atomic API by hand.
    let now = chrono::Utc::now();
    let fresh_album = DbAlbum {
        id: Uuid::new_v4().to_string(),
        title: album_a.title.clone(),
        artist_id: album_a.artist_id.clone(),
        year: album_a.year,
        primary_release_id: None,
        is_compilation: album_a.is_compilation,
        created_at: now,
    };

    // Race window: another writer lands release_intruder into
    // album_a after the (hypothetical) pre-flight read but before
    // the atomic call.
    let release_intruder = create_test_release(&album_a.id);
    manager
        .database
        .insert_release(&release_intruder)
        .await
        .unwrap();

    let outcome = manager
        .database
        .set_identity_atomic(
            &release_alpha.id,
            &[mb_identity("g2", None)],
            crate::db::ReleaseMetadataSource::MusicBrainz,
            Some("mb-rel-g2"),
            &album_a.id,
            &fresh_album.id,
            Some(&fresh_album),
            &[],
        )
        .await
        .unwrap();

    assert!(
        !outcome.source_album_deleted,
        "atomic recheck must protect the late-arriving release"
    );

    // Source album survives, holding only release_intruder.
    let survivors = manager
        .database
        .get_releases_for_album(&album_a.id)
        .await
        .unwrap();
    assert_eq!(survivors.len(), 1);
    assert_eq!(survivors[0].id, release_intruder.id);

    // release_alpha landed in the fresh album.
    let fresh_releases = manager
        .database
        .get_releases_for_album(&fresh_album.id)
        .await
        .unwrap();
    assert_eq!(fresh_releases.len(), 1);
    assert_eq!(fresh_releases[0].id, release_alpha.id);
}

// ── re_identify_release ────────────────────────────────────────────
//
// Exact / Approximate fetch through MB / Discogs, so these tests seed the release
// cache first and `prepare_release` reads locally instead of hitting the network.
// The Unknown path makes no source claim, so it needs no seeding.

#[tokio::test]
async fn re_identify_to_unknown_clears_identities_and_moves_album() {
    use lofty::config::WriteOptions;
    use lofty::prelude::*;
    use lofty::tag::{Tag, TagType};
    use std::fs;

    let (manager, _temp_dir) = setup_test_manager().await;

    // Local audio files so the post-`set_identity` reseed can read tags.
    let media = TempDir::new().unwrap();
    let fixtures = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
        .join("flac");
    let mut filenames = Vec::new();
    for (name, title) in [("01.flac", "Tag One"), ("02.flac", "Tag Two")] {
        let dest = media.path().join(name);
        fs::copy(fixtures.join("01 Test Track 1.flac"), &dest).unwrap();
        let mut tagged = lofty::read_from_path(&dest).unwrap();
        let mut tag = Tag::new(TagType::VorbisComments);
        tag.set_title(title.to_string());
        tag.set_artist("Tag Artist".to_string());
        tag.set_album("Tag Album".to_string());
        tagged.insert_tag(tag);
        tagged.save_to_path(&dest, WriteOptions::default()).unwrap();
        filenames.push(name.to_string());
    }

    let album = create_test_album();
    let mut release = create_test_release(&album.id);
    release.metadata_source = crate::db::ReleaseMetadataSource::MusicBrainz;
    release.metadata_source_release_id = Some("mb-rel-1".to_string());
    release.remote = false;

    manager.database.insert_album(&album).await.unwrap();
    manager.database.insert_release(&release).await.unwrap();
    // Two existing track rows align positionally with the two files.
    insert_n_tracks(&manager.database, &release.id, 2).await;
    let now = Utc::now();
    for name in &filenames {
        let file = crate::db::DbFile::new(
            &release.id,
            name,
            0,
            crate::util::content_type::ContentType::Flac,
            Uuid::new_v4().to_string(),
            now,
            crate::util::fs::hash_bytes(b"fixture"),
        );
        manager.database.insert_file(&file).await.unwrap();
    }
    // Register the files as coven external refs (in-place files of a Local
    // release) AFTER inserting them, so the file-tag re-read resolves paths.
    manager
        .database
        .register_release_external_refs_for_test(&release.id, &media.path().to_string_lossy())
        .await
        .unwrap();
    manager
        .database
        .insert_release_identities(&release.id, &[mb_identity("g1", Some("mb-rel-1"))])
        .await
        .unwrap();

    // Seed a stale cache row to verify Unknown clears it.
    manager
        .database
        .insert_release_metadata(&crate::db::DbReleaseMetadata::new(
            &release.id,
            "musicbrainz",
            r#"{"id":"mb-rel-1"}"#.to_string(),
            Uuid::new_v4().to_string(),
            Utc::now(),
        ))
        .await
        .unwrap();

    manager
        .re_identify_release(&release.id, crate::import::IdentityChoice::Unknown)
        .await
        .unwrap();

    // Original (single-release) album is gone; release sits on a
    // fresh one.
    assert!(manager
        .database
        .find_album_by_id(&album.id)
        .await
        .unwrap()
        .is_none());
    let new_album_id = manager
        .database
        .find_album_id_for_release(&release.id)
        .await
        .unwrap()
        .unwrap();
    assert_ne!(new_album_id, album.id);

    // Identity rows wiped, metadata pointer flipped to file_tags,
    // cache rows cleared (file_tags has no source payload).
    let identities = manager
        .database
        .get_release_identities(&release.id)
        .await
        .unwrap();
    assert!(identities.is_empty());
    let updated = manager
        .database
        .find_release_by_id(&release.id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(
        updated.metadata_source,
        crate::db::ReleaseMetadataSource::FileTags
    );
    assert_eq!(updated.metadata_source_release_id, None);
    let cache_after = manager
        .database
        .get_release_metadata_by_source(&release.id)
        .await
        .unwrap();
    assert!(
        cache_after.is_empty(),
        "Unknown commit must clear stale cached payloads"
    );
}

// ── re_identify_release Exact / Approximate (MB cache-seeded) ────
//
// Drive the network-side `prepare_release` through the MB LRU cache
// (`seed_release_cache` + `seed_release_group_json_cache`) so these tests don't hit
// the network. The caches are process-global LRUs, so each test uses a unique MB
// release ID and no other test's seed bleeds in.

/// Build a synthetic MB release response with `n` track rows on a
/// single CD medium, plus a release group reference. Suitable for
/// driving `prepare_release` via cache seeding.
fn make_mb_release_for_re_identify(
    release_id: &str,
    release_group_id: &str,
    track_count: usize,
) -> crate::musicbrainz::MbReleaseResponse {
    use crate::musicbrainz::{
        MbArtistCredit, MbArtistRef, MbMedium, MbReleaseGroupRef, MbReleaseResponse, MbTrack,
    };
    MbReleaseResponse {
        id: release_id.to_string(),
        title: "Album Title".to_string(),
        date: Some("2024-01-01".to_string()),
        country: None,
        barcode: None,
        artist_credit: vec![MbArtistCredit {
            name: "Artist Name".to_string(),
            artist: Some(MbArtistRef {
                id: Some("mb-artist-1".to_string()),
                name: Some("Artist Name".to_string()),
                sort_name: Some("Artist Name".to_string()),
            }),
        }],
        release_group: Some(MbReleaseGroupRef {
            id: release_group_id.to_string(),
            first_release_date: Some("2024-01-01".to_string()),
            relations: Some(vec![]),
        }),
        label_info: vec![],
        media: vec![MbMedium {
            format: Some("CD".to_string()),
            tracks: (1..=track_count)
                .map(|n| MbTrack {
                    position: Some(n as i64),
                    number: Some(n.to_string()),
                    title: Some(format!("Track {n}")),
                    length: None,
                    recording: None,
                    artist_credit: vec![],
                })
                .collect(),
        }],
        relations: vec![],
    }
}

/// Insert `n` plain track rows for a release. Mirrors the row shape
/// `prepared.parsed.tracks` would produce so the track-count check
/// in `re_identify_release` accepts the picked release.
async fn insert_n_tracks(database: &Database, release_id: &str, n: usize) {
    for i in 1..=n {
        let track = crate::db::DbTrack {
            id: Uuid::new_v4().to_string(),
            release_id: release_id.to_string(),
            title: format!("Track {i}"),
            side: 1,
            track_number: Some(i as i32),
            duration_ms: None,
            discogs_position: None,
            created_at: Utc::now(),
        };
        database.insert_track(&track).await.unwrap();
    }
}

#[tokio::test]
async fn re_identify_release_exact_writes_cache() {
    use crate::import::{IdentityChoice, MetadataRef, MetadataSource};
    use crate::musicbrainz::{seed_release_cache, seed_release_group_json_cache};

    let (manager, _temp_dir) = setup_test_manager().await;

    let album = create_test_album();
    let mut release = create_test_release(&album.id);
    release.metadata_source = crate::db::ReleaseMetadataSource::MusicBrainz;
    release.metadata_source_release_id = Some("mb-rel-old".to_string());

    manager.database.insert_album(&album).await.unwrap();
    manager.database.insert_release(&release).await.unwrap();
    manager
        .database
        .insert_release_identities(&release.id, &[mb_identity("g-old", Some("mb-rel-old"))])
        .await
        .unwrap();
    insert_n_tracks(&manager.database, &release.id, 3).await;

    // Seed an old cached payload so the test can assert it was
    // replaced (not just augmented).
    manager
        .database
        .insert_release_metadata(&crate::db::DbReleaseMetadata::new(
            &release.id,
            "musicbrainz",
            r#"{"id":"mb-rel-old"}"#.to_string(),
            Uuid::new_v4().to_string(),
            Utc::now(),
        ))
        .await
        .unwrap();

    // Cache the picked release so `prepare_release` skips the
    // network. The raw JSON payload is what the cache replacement
    // step writes into `release_metadata`.
    let new_release_id = "exact-re-identify-mb-rel-new";
    let new_group_id = "exact-re-identify-mb-group-new";
    let new_response = make_mb_release_for_re_identify(new_release_id, new_group_id, 3);
    let new_raw_json = r#"{"id":"exact-re-identify-mb-rel-new"}"#.to_string();
    seed_release_cache(new_release_id, (new_response, None, new_raw_json.clone()));
    seed_release_group_json_cache(
        new_group_id,
        r#"{"id":"exact-re-identify-mb-group-new"}"#.to_string(),
    );

    manager
        .re_identify_release(
            &release.id,
            IdentityChoice::Exact {
                release_ref: MetadataRef {
                    source: MetadataSource::MusicBrainz,
                    id: new_release_id.to_string(),
                },
            },
        )
        .await
        .unwrap();

    // Identity row updated to Exact at the new pressing.
    let identities = manager
        .database
        .get_release_identities(&release.id)
        .await
        .unwrap();
    assert_eq!(identities.len(), 1);
    assert_eq!(identities[0].source, MetadataSource::MusicBrainz);
    assert_eq!(identities[0].source_group_id, new_group_id);
    assert_eq!(
        identities[0].source_release_id.as_deref(),
        Some(new_release_id)
    );

    // Pointer columns flipped to the new source release.
    let updated = manager
        .database
        .find_release_by_id(&release.id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(
        updated.metadata_source,
        crate::db::ReleaseMetadataSource::MusicBrainz
    );
    assert_eq!(
        updated.metadata_source_release_id.as_deref(),
        Some(new_release_id)
    );

    // Cache rows aligned with the new pointer: stale "mb-rel-old"
    // payload gone, fresh payload + release-group JSON in.
    let cache_after = manager
        .database
        .get_release_metadata_by_source(&release.id)
        .await
        .unwrap();
    assert_eq!(
        cache_after.get("musicbrainz").map(String::as_str),
        Some(new_raw_json.as_str()),
        "cache must hold the freshly-fetched MB JSON, not the stale row"
    );
    assert!(
        cache_after.contains_key("musicbrainz_release_group"),
        "release-group JSON must be cached alongside the release JSON"
    );
}

#[tokio::test]
async fn re_identify_release_approximate_writes_cache() {
    use crate::import::{IdentityChoice, MetadataRef, MetadataSource};
    use crate::musicbrainz::{seed_release_cache, seed_release_group_json_cache};

    let (manager, _temp_dir) = setup_test_manager().await;

    let album = create_test_album();
    let mut release = create_test_release(&album.id);
    release.metadata_source = crate::db::ReleaseMetadataSource::FileTags;
    release.metadata_source_release_id = None;

    manager.database.insert_album(&album).await.unwrap();
    manager.database.insert_release(&release).await.unwrap();
    insert_n_tracks(&manager.database, &release.id, 4).await;

    let new_release_id = "approx-re-identify-mb-rel-new";
    let new_group_id = "approx-re-identify-mb-group-new";
    let new_response = make_mb_release_for_re_identify(new_release_id, new_group_id, 4);
    let new_raw_json = r#"{"id":"approx-re-identify-mb-rel-new"}"#.to_string();
    seed_release_cache(new_release_id, (new_response, None, new_raw_json.clone()));
    seed_release_group_json_cache(
        new_group_id,
        r#"{"id":"approx-re-identify-mb-group-new"}"#.to_string(),
    );

    manager
        .re_identify_release(
            &release.id,
            IdentityChoice::Approximate {
                release_ref: MetadataRef {
                    source: MetadataSource::MusicBrainz,
                    id: new_release_id.to_string(),
                },
            },
        )
        .await
        .unwrap();

    // Approximate clears `source_release_id` on the identity row
    // (group-only claim) but the metadata pointer still names the
    // picked pressing — reset-to-source reads cached payload through it.
    let identities = manager
        .database
        .get_release_identities(&release.id)
        .await
        .unwrap();
    assert_eq!(identities.len(), 1);
    assert_eq!(identities[0].source, MetadataSource::MusicBrainz);
    assert_eq!(identities[0].source_group_id, new_group_id);
    assert_eq!(identities[0].source_release_id, None);

    let updated = manager
        .database
        .find_release_by_id(&release.id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(
        updated.metadata_source,
        crate::db::ReleaseMetadataSource::MusicBrainz
    );
    assert_eq!(
        updated.metadata_source_release_id.as_deref(),
        Some(new_release_id)
    );

    let cache_after = manager
        .database
        .get_release_metadata_by_source(&release.id)
        .await
        .unwrap();
    assert_eq!(
        cache_after.get("musicbrainz").map(String::as_str),
        Some(new_raw_json.as_str())
    );
    assert!(cache_after.contains_key("musicbrainz_release_group"));
}

#[tokio::test]
async fn re_identify_release_rejects_track_count_mismatch() {
    // Re-identify re-points the identity without re-binding any audio, so a
    // source naming a different number of tracks leaves rows with nothing to
    // point at: a 12-track release can't replace a 10-track rip. A folder
    // import maps its own audio into track slots instead, where a count
    // disagreement is a row to look at rather than a refusal.
    use crate::import::{IdentityChoice, MetadataRef, MetadataSource};
    use crate::musicbrainz::{seed_release_cache, seed_release_group_json_cache};

    let (manager, _temp_dir) = setup_test_manager().await;

    let album = create_test_album();
    let release = create_test_release(&album.id);
    manager.database.insert_album(&album).await.unwrap();
    manager.database.insert_release(&release).await.unwrap();

    // Local release has 10 tracks; picked release has 12.
    insert_n_tracks(&manager.database, &release.id, 10).await;

    let new_release_id = "mismatch-re-identify-mb-rel-new";
    let new_group_id = "mismatch-re-identify-mb-group-new";
    let new_response = make_mb_release_for_re_identify(new_release_id, new_group_id, 12);
    seed_release_cache(
        new_release_id,
        (
            new_response,
            None,
            r#"{"id":"mismatch-re-identify-mb-rel-new"}"#.to_string(),
        ),
    );
    seed_release_group_json_cache(
        new_group_id,
        r#"{"id":"mismatch-re-identify-mb-group-new"}"#.to_string(),
    );

    let err = manager
        .re_identify_release(
            &release.id,
            IdentityChoice::Exact {
                release_ref: MetadataRef {
                    source: MetadataSource::MusicBrainz,
                    id: new_release_id.to_string(),
                },
            },
        )
        .await
        .expect_err("track-count mismatch must error before identity write");
    let msg = err.to_string();
    assert!(
        msg.contains("Track count mismatch") && msg.contains("10") && msg.contains("12"),
        "error must name both counts so the UI can render a useful banner: {msg}"
    );

    // No identity row written.
    let identities = manager
        .database
        .get_release_identities(&release.id)
        .await
        .unwrap();
    assert!(
        identities.is_empty(),
        "mismatched commit must not leave a partial identity row"
    );
}

#[tokio::test]
async fn re_identify_release_followed_by_reset_succeeds() {
    // The cache-alignment invariant end to end: after a re-identify commit,
    // `reset_metadata_to_source` projects through the new pointer and new cached
    // payload without tripping the cache-divergence guard. A regression here means
    // re-identify left the cache stale against `metadata_source_release_id`.
    use crate::import::{IdentityChoice, MetadataRef, MetadataSource};
    use crate::musicbrainz::{seed_release_cache, seed_release_group_json_cache};

    let (manager, _temp_dir) = setup_test_manager().await;

    let album = create_test_album();
    let mut release = create_test_release(&album.id);
    release.metadata_source = crate::db::ReleaseMetadataSource::FileTags;
    release.metadata_source_release_id = None;

    manager.database.insert_album(&album).await.unwrap();
    manager.database.insert_release(&release).await.unwrap();
    insert_n_tracks(&manager.database, &release.id, 2).await;

    let new_release_id = "reset-re-identify-mb-rel-new";
    let new_group_id = "reset-re-identify-mb-group-new";
    let new_response = make_mb_release_for_re_identify(new_release_id, new_group_id, 2);
    seed_release_cache(
            new_release_id,
            (
                new_response,
                None,
                r#"{"id":"reset-re-identify-mb-rel-new","title":"Album Title","date":"2024-01-01","artist-credit":[{"name":"Artist Name","artist":{"id":"mb-artist-1","name":"Artist Name","sort-name":"Artist Name"}}],"release-group":{"id":"reset-re-identify-mb-group-new"},"media":[{"format":"CD","tracks":[{"position":1,"number":"1","title":"Track 1"},{"position":2,"number":"2","title":"Track 2"}]}]}"#.to_string(),
            ),
        );
    seed_release_group_json_cache(
        new_group_id,
        r#"{"id":"reset-re-identify-mb-group-new"}"#.to_string(),
    );

    manager
        .re_identify_release(
            &release.id,
            IdentityChoice::Exact {
                release_ref: MetadataRef {
                    source: MetadataSource::MusicBrainz,
                    id: new_release_id.to_string(),
                },
            },
        )
        .await
        .unwrap();

    // Reset replays the seed through the new pointer. A stale
    // cache would surface here as a missing key, a
    // parse error, or a divergence-guard `Err`. Success means
    // re_identify_release left the cache aligned.
    let edit = manager
        .reset_metadata_to_source(&release.id)
        .await
        .expect("reset must replay through aligned cache after re-identify");
    assert_eq!(edit.album_title, "Album Title");
    assert_eq!(edit.tracks.len(), 2);
}

#[tokio::test]
async fn re_identify_to_unknown_reseeds_rows_from_file_tags() {
    // A release carrying MusicBrainz-shaped rows, with local audio
    // files whose embedded tags say something different. Re-identifying
    // as Unknown must reseed the album/track rows from those tags — not
    // leave the old MB metadata displayed under a "use my files" claim.
    use crate::import::IdentityChoice;
    use lofty::config::WriteOptions;
    use lofty::prelude::*;
    use lofty::tag::{Tag, TagType};
    use std::fs;

    let (manager, _temp_dir) = setup_test_manager().await;

    // Local files live in a local folder so `local_file_path`
    // resolves to disk where lofty can read the embedded tags.
    let media = TempDir::new().unwrap();
    let fixtures = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
        .join("flac");

    let tag_file = |name: &str, title: &str| -> String {
        let src = fixtures.join("01 Test Track 1.flac");
        let dest = media.path().join(name);
        fs::copy(&src, &dest).unwrap();
        let mut tagged = lofty::read_from_path(&dest).unwrap();
        let mut tag = Tag::new(TagType::VorbisComments);
        tag.set_title(title.to_string());
        tag.set_artist("Tagged Artist".to_string());
        tag.set_album("Tagged Album".to_string());
        tagged.insert_tag(tag);
        tagged.save_to_path(&dest, WriteOptions::default()).unwrap();
        name.to_string()
    };
    let f1 = tag_file("01.flac", "Tagged One");
    let f2 = tag_file("02.flac", "Tagged Two");

    let album = create_test_album();
    let mut release = create_test_release(&album.id);
    // MusicBrainz-shaped pointer; the rows below carry MB metadata.
    release.metadata_source = crate::db::ReleaseMetadataSource::MusicBrainz;
    release.metadata_source_release_id = Some("mb-rel-1".to_string());
    release.remote = false;

    manager.database.insert_album(&album).await.unwrap();
    manager.database.insert_release(&release).await.unwrap();
    manager
        .database
        .insert_release_identities(&release.id, &[mb_identity("g1", Some("mb-rel-1"))])
        .await
        .unwrap();

    // MB-shaped track rows — distinct from the embedded tags.
    for (i, (id, title)) in [
        ("08c7ff07-b56a-4e16-8df6-ae2967fa0806", "MB Track One"),
        ("08c7fe07-b56a-4c63-8df6-ad2967fa0653", "MB Track Two"),
    ]
    .into_iter()
    .enumerate()
    {
        let track = crate::db::DbTrack {
            id: id.to_string(),
            release_id: release.id.clone(),
            title: title.to_string(),
            side: 1,
            track_number: Some(i as i32 + 1),
            duration_ms: None,
            discogs_position: None,
            created_at: Utc::now(),
        };
        manager.database.insert_track(&track).await.unwrap();
    }
    let now = Utc::now();
    for name in [&f1, &f2] {
        let file = crate::db::DbFile::new(
            &release.id,
            name,
            0,
            crate::util::content_type::ContentType::Flac,
            Uuid::new_v4().to_string(),
            now,
            crate::util::fs::hash_bytes(b"fixture"),
        );
        manager.database.insert_file(&file).await.unwrap();
    }
    // Register the in-place files as coven external refs after inserting them.
    manager
        .database
        .register_release_external_refs_for_test(&release.id, &media.path().to_string_lossy())
        .await
        .unwrap();

    manager
        .re_identify_release(&release.id, IdentityChoice::Unknown)
        .await
        .unwrap();

    // Pointer flipped to file_tags.
    let updated = manager
        .database
        .find_release_by_id(&release.id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(
        updated.metadata_source,
        crate::db::ReleaseMetadataSource::FileTags
    );

    // Album + track rows now reflect the embedded tags, not the MB seed.
    let landing_album_id = manager
        .database
        .find_album_id_for_release(&release.id)
        .await
        .unwrap()
        .unwrap();
    let landing_album = manager
        .database
        .find_album_by_id(&landing_album_id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(landing_album.title, "Tagged Album");

    let tracks = manager
        .database
        .get_tracks_for_release(&release.id)
        .await
        .unwrap();
    let titles: Vec<&str> = tracks.iter().map(|t| t.title.as_str()).collect();
    assert!(
        titles.contains(&"Tagged One") && titles.contains(&"Tagged Two"),
        "track rows must carry the embedded tag titles, got {titles:?}"
    );
    assert!(
        !titles.iter().any(|t| t.starts_with("MB ")),
        "old MusicBrainz track titles must be gone, got {titles:?}"
    );
}

#[tokio::test]
async fn discogs_client_withheld_when_rejected() {
    use crate::config::DiscogsValidation;
    let (manager, _temp_dir) = setup_test_manager_with_library_id("discogs-withheld-test").await;
    manager
        .set_discogs_key(
            "f7228aaf-52b3-40ea-8526-a7e8aa0bf5da",
            DiscogsValidation::Valid,
        )
        .unwrap();

    assert!(
        manager.discogs_client().unwrap().is_some(),
        "a Valid key is served"
    );

    manager
        .set_discogs_validation(DiscogsValidation::Unvalidated)
        .unwrap();
    assert!(
        manager.discogs_client().unwrap().is_some(),
        "an Unvalidated key is served optimistically"
    );

    manager
        .set_discogs_validation(DiscogsValidation::Rejected)
        .unwrap();
    assert!(
        manager.discogs_client().unwrap().is_none(),
        "a Rejected key is withheld"
    );
}

/// A key present in the keyring but absent from config (the residue a torn write
/// or external keyring tampering can leave) is not served: a usable key requires
/// both stores to agree it exists.
#[tokio::test]
async fn discogs_client_withheld_when_config_has_no_key() {
    use crate::keys::BaeStoreKeysExt;
    let (manager, _temp_dir) = setup_test_manager_with_library_id("discogs-orphan-key").await;

    // Keyring bytes present, config untouched (still `None`).
    manager.key_service.set_discogs_key("orphan-key").unwrap();

    assert_eq!(manager.discogs_validation(), None);
    assert!(
        manager.discogs_client().unwrap().is_none(),
        "a keyring key with no config hint is not served",
    );
}

/// `set_discogs_key` and `clear_discogs_key` move both durable stores together.
#[tokio::test]
async fn set_and_clear_discogs_key_move_both_stores() {
    use crate::config::DiscogsValidation;
    use crate::keys::BaeStoreKeysExt;
    let (manager, _temp_dir) = setup_test_manager_with_library_id("discogs-atomic").await;

    manager
        .set_discogs_key("the-key", DiscogsValidation::Valid)
        .unwrap();
    assert_eq!(manager.discogs_validation(), Some(DiscogsValidation::Valid));
    assert_eq!(
        manager.key_service.get_discogs_key().unwrap().as_deref(),
        Some("the-key"),
    );
    assert!(manager.discogs_client().unwrap().is_some());

    manager.clear_discogs_key().unwrap();
    assert_eq!(manager.discogs_validation(), None);
    assert_eq!(manager.key_service.get_discogs_key().unwrap(), None);
    assert!(manager.discogs_client().unwrap().is_none());
}

/// Revalidation surfaces the config-says-stored/keyring-empty mismatch as an
/// error, not a swallowed warning — the one torn state our writes can't produce
/// but external tampering can.
#[tokio::test]
async fn revalidate_errors_when_config_claims_a_key_the_keyring_lacks() {
    use crate::config::DiscogsValidation;

    let (manager, _temp_dir) = setup_test_manager_with_library_id("discogs-revalidate-torn").await;
    // Config claims an Unvalidated key; the keyring has none — the torn state.
    manager
        .config_handle
        .update(|c| c.discogs = Some(DiscogsValidation::Unvalidated))
        .unwrap();

    let handle = crate::import::ImportService::start(
        tokio::runtime::Handle::current(),
        manager.clone(),
        crate::import::cover_art::CoverArtArchiveClient::hermetic(),
    )
    .await
    .unwrap();

    assert!(
        handle.revalidate_discogs_token().await.is_err(),
        "a stored-but-keyless config must fail revalidation, not warn and continue",
    );
}

#[tokio::test]
async fn discogs_validation_observer_confirms_and_rejects() {
    use crate::config::DiscogsValidation;
    use crate::discogs::client::DiscogsKeySignal;
    let (manager, _temp_dir) = setup_test_manager().await;
    let observe = manager.discogs_validation_observer();

    // A success confirms a stored Unvalidated key.
    manager
        .config_handle
        .update(|c| c.discogs = Some(DiscogsValidation::Unvalidated))
        .unwrap();
    observe(DiscogsKeySignal::Accepted);
    assert_eq!(manager.discogs_validation(), Some(DiscogsValidation::Valid));

    // A 401 rejects, from any prior state.
    observe(DiscogsKeySignal::Rejected);
    assert_eq!(
        manager.discogs_validation(),
        Some(DiscogsValidation::Rejected)
    );

    // A success does NOT flip an already-Rejected key back to Valid.
    observe(DiscogsKeySignal::Accepted);
    assert_eq!(
        manager.discogs_validation(),
        Some(DiscogsValidation::Rejected)
    );

    // A success while already Valid is a no-op (only Unvalidated -> Valid).
    manager
        .set_discogs_validation(DiscogsValidation::Valid)
        .unwrap();
    observe(DiscogsKeySignal::Accepted);
    assert_eq!(manager.discogs_validation(), Some(DiscogsValidation::Valid));
}

/// Aborting the transfer driver mid-flight (the bridge future is abortable)
/// must still emit `ReleaseTransferEnded` so the UI's transfer indicator
/// clears. The drop guard inside `drive_transfer` fires it when the future
/// is dropped between progress events.
#[tokio::test]
async fn aborted_transfer_still_emits_transfer_ended() {
    let (manager, _temp_dir) = setup_test_manager().await;
    let mut rx = manager.subscribe_events();

    // A channel whose sender we hold open: `drive_transfer` parks in
    // `rx.recv().await` forever, so the only way out is the abort below.
    let (tx, progress_rx) = tokio::sync::mpsc::unbounded_channel();

    let driver = manager.clone();
    let handle = tokio::spawn(async move {
        let result = driver
            .drive_transfer(REL_ABORT, ReleaseStorageAction::Pin, progress_rx)
            .await;
        panic!("the parked driver must only exit by abort, returned {result:?}");
    });

    // Let the task reach its parked `recv()` before aborting.
    tokio::task::yield_now().await;
    handle.abort();
    let join = handle.await;
    assert!(
        join.expect_err("the parked driver can only exit by abort")
            .is_cancelled(),
        "the driver must end by cancellation, not a panic"
    );
    drop(tx);

    let event = tokio::time::timeout(std::time::Duration::from_secs(2), rx.recv())
        .await
        .expect("ReleaseTransferEnded must arrive after abort")
        .expect("event channel stays open");
    assert!(
        matches!(
            &event,
            LibraryEvent::ReleaseTransferEnded { release_id } if release_id == REL_ABORT
        ),
        "the drop guard must emit ReleaseTransferEnded for the aborted release, got {event:?}"
    );
}

/// Seed an album with two releases, each holding two tracks with explicit
/// side/track-number so the library order is deterministic. Track ids are
/// chosen so the `(release_id, side, track_number, id)` order is unambiguous.
async fn seed_two_release_library(manager: &LibraryManager) -> (String, String) {
    use crate::db::DbTrack;
    let mut album = create_test_album();
    album.id = "1250a7bb-41ed-4500-8ab4-04f5d3461e30".to_string();
    let mut rel1 = create_test_release(&album.id);
    rel1.id = REL_1.to_string();
    let mut rel2 = create_test_release(&album.id);
    rel2.id = REL_2.to_string();
    manager.database.insert_album(&album).await.unwrap();
    manager.database.insert_release(&rel1).await.unwrap();
    manager.database.insert_release(&rel2).await.unwrap();

    let track = |release_id: &str, id: &str, side: i32, number: i32| {
        let t = DbTrack {
            side,
            ..DbTrack::new_test(release_id, id, "Track Title", Some(number))
        };
        let database = &manager.database;
        async move { database.insert_track(&t).await.unwrap() }
    };
    // rel-1: side 1 then side 2; rel-2: two side-1 tracks.
    track(REL_1, "48ae00a1-d7a5-443c-8240-f999fc4ddfcc", 1, 1).await;
    track(REL_1, "48ae03a1-d7a5-4955-8240-fc99fc4de4e5", 2, 1).await;
    track(REL_2, "cc4180bc-58f5-456f-8116-f9b2099f5b7f", 1, 1).await;
    track(REL_2, "cc4181bc-58f5-4722-8116-fab2099f5d32", 1, 2).await;
    (rel1.id, rel2.id)
}

/// `get_all_track_ids` returns every library track in the deterministic base
/// order — by release, then side, track number, id — so a shuffle seed
/// permutes a stable list.
#[tokio::test]
async fn test_get_all_track_ids_returns_library_in_base_order() {
    let (manager, _temp_dir) = setup_test_manager().await;
    seed_two_release_library(&manager).await;
    let all = manager.get_all_track_ids().await.unwrap();
    assert_eq!(
        all,
        vec![
            "48ae00a1-d7a5-443c-8240-f999fc4ddfcc",
            "48ae03a1-d7a5-4955-8240-fc99fc4de4e5",
            "cc4180bc-58f5-456f-8116-f9b2099f5b7f",
            "cc4181bc-58f5-4722-8116-fab2099f5d32"
        ]
    );
}

/// The two track-id queries the service's source dispatcher routes between:
/// a release's own ordered tracks (`get_track_ids`) vs the whole library
/// (`get_all_track_ids`). The library is the union of the releases, so a
/// release's tracks are a strict subset of it.
#[tokio::test]
async fn test_release_and_library_track_id_queries_return_their_sets() {
    let (manager, _temp_dir) = setup_test_manager().await;
    let (rel1, _rel2) = seed_two_release_library(&manager).await;
    let release_tracks = manager.get_track_ids(&rel1).await.unwrap();
    assert_eq!(
        release_tracks,
        vec![
            "48ae00a1-d7a5-443c-8240-f999fc4ddfcc",
            "48ae03a1-d7a5-4955-8240-fc99fc4de4e5"
        ]
    );
    let library_tracks = manager.get_all_track_ids().await.unwrap();
    assert_eq!(
        library_tracks,
        vec![
            "48ae00a1-d7a5-443c-8240-f999fc4ddfcc",
            "48ae03a1-d7a5-4955-8240-fc99fc4de4e5",
            "cc4180bc-58f5-456f-8116-f9b2099f5b7f",
            "cc4181bc-58f5-4722-8116-fab2099f5d32"
        ]
    );
    assert!(release_tracks.iter().all(|t| library_tracks.contains(t)));
}

/// A `playback_state` row carrying a library source survives save → load: the
/// `source` column stores the library sentinel and reads back unchanged, and a
/// release row stores/reads its id. (Decoding the sentinel back to the source
/// enum is covered in `playback::persisted`.)
#[tokio::test]
async fn test_playback_state_source_column_round_trips_both_kinds() {
    use crate::db::{DbPlaybackContext, DbPlaybackState};
    use crate::playback::source_to_str;
    use crate::playback::ContextSource;
    let (manager, _temp_dir) = setup_test_manager().await;

    for source in [
        ContextSource::Library,
        ContextSource::Release(REL_1.to_string()),
    ] {
        let row = DbPlaybackState {
            context: Some(DbPlaybackContext {
                source: source_to_str(&source),
                shuffled: true,
            }),
            manual: "[]".to_string(),
            repeat: "off".to_string(),
            current_track_id: None,
            position_ms: None,
            volume: 1.0,
            is_muted: false,
        };
        manager.save_playback_state(&row).await.unwrap();
        let crate::db::LoadedPlaybackState::Present(loaded) =
            manager.load_playback_state().await.unwrap()
        else {
            panic!("a saved row loads");
        };
        assert_eq!(
            loaded.context.unwrap().source,
            source_to_str(&source),
            "the source column round-trips for {source:?}"
        );
    }
}

/// Each sync/membership/cloud-setup failure class carries a distinct diagnostic
/// category to the bridge, so the UI shows different messages for bad
/// credentials, an unreachable backend, a keyring failure, a config-write
/// failure, and a membership-chain failure. Builds the exact coven boundary
/// errors these flows return and asserts the class the bridge reads.
#[test]
fn setup_failure_classes_map_to_distinct_categories() {
    use crate::ui::UiErrorCategory as C;

    let cases: Vec<(LibraryError, C)> = vec![
        (
            coven::CloudHomeError::Configuration("rejected credentials".into()).into(),
            C::Credentials,
        ),
        (
            coven::CloudHomeError::NotFound("missing bucket".into()).into(),
            C::Credentials,
        ),
        (
            coven::CloudHomeError::Transport("unreachable endpoint".into()).into(),
            C::Network,
        ),
        (
            LibraryError::CloudSetup("oauth denied".into()),
            C::Credentials,
        ),
        (
            coven::KeyError::Persistence("keyring write failed".into()).into(),
            C::Keyring,
        ),
        (
            coven::ConfigError::Config("config write failed".into()).into(),
            C::Config,
        ),
        (
            coven::SyncError::Key(coven::KeyError::Persistence("k".into())).into(),
            C::Keyring,
        ),
        (
            coven::SyncError::CloudHome(coven::CloudHomeError::Transport("t".into())).into(),
            C::Network,
        ),
        // `SyncError::Membership` maps to `C::Membership` (see `sync_category`),
        // but its payload `MembershipOpsError` is no longer part of coven's
        // curated public API, so a host test can't fabricate that variant.
        (coven::SyncError::NotConfigured.into(), C::Internal),
        (
            LibraryError::Storage("pin ended without completion".into()),
            C::Internal,
        ),
        (
            LibraryError::Validation("library name cannot be empty".into()),
            C::Config,
        ),
    ];

    for (error, expected) in &cases {
        assert_eq!(error.category(), *expected, "{error}");
    }
}

/// A coven-typed sync error propagates through the sync controller and the
/// manager forwarder without being flattened to a string: an unconfigured
/// library surfaces `SyncError::NotConfigured` intact, and its class is Internal.
#[tokio::test]
async fn get_members_on_unconfigured_library_propagates_typed_sync_error() {
    let (manager, _temp_dir) = setup_test_manager().await;
    let Err(err) = manager.get_members().await else {
        panic!("no cloud home is connected, so get_members must fail");
    };
    assert!(
        matches!(err, LibraryError::Sync(coven::SyncError::NotConfigured)),
        "expected a typed SyncError::NotConfigured, got {err:?}"
    );
    assert_eq!(err.category(), crate::ui::UiErrorCategory::Internal);
}

// ── Queue windowing tests ───────────────────────────────────────────

/// Insert one release with `count` sequentially-numbered tracks
/// (`track-0`..`track-{count-1}`); return their ids in track order.
async fn seed_release_tracks(manager: &LibraryManager, count: usize) -> Vec<String> {
    let album = create_test_album();
    let release = create_test_release(&album.id);
    manager.database.insert_album(&album).await.unwrap();
    manager.database.insert_release(&release).await.unwrap();
    let mut track_ids = Vec::with_capacity(count);
    for i in 0..count {
        let track_id = bae_test_support::test_uuid(&format!("track-{i}"));
        let track = crate::db::DbTrack::new_test(
            &release.id,
            &track_id,
            &format!("Track Title {i}"),
            Some(i as i32),
        );
        manager.database.insert_track(&track).await.unwrap();
        track_ids.push(track_id);
    }
    track_ids
}

/// A `Library`-source context projection whose upcoming tail is `track_ids`,
/// in order, each wrapped in a freshly-minted per-instance entry id.
fn context_projection_over(track_ids: &[String]) -> crate::playback::ContextProjection {
    crate::playback::ContextProjection {
        source: crate::playback::ContextSource::Library,
        shuffled: false,
        upcoming: track_ids
            .iter()
            .enumerate()
            .map(|(i, t)| crate::playback::QueueEntry {
                id: crate::playback::QueueEntryId(format!("ctx-{i}")),
                track_id: t.clone(),
            })
            .collect(),
    }
}

/// `resolve_queue_projection` resolves only the first `QUEUE_UPCOMING_WINDOW`
/// entries of a library-scaled context tail, not the whole thing — the
/// windowing this feature exists for — while still reporting the tail's real
/// length via `upcoming_total` and preserving order.
#[tokio::test]
async fn resolve_queue_projection_windows_a_library_scaled_context_tail() {
    let (manager, _temp_dir) = setup_test_manager().await;
    let track_ids = seed_release_tracks(&manager, crate::queue::QUEUE_UPCOMING_WINDOW + 50).await;

    let projection = crate::playback::PlaybackQueueProjection {
        manual: Vec::new(),
        context: Some(context_projection_over(&track_ids)),
        has_next: true,
        has_previous: false,
        revision: 7,
    };
    let snapshot = manager.resolve_queue_projection(projection).await.unwrap();
    let context = snapshot.context.expect("a context was set");

    assert_eq!(
        context.upcoming.len(),
        crate::queue::QUEUE_UPCOMING_WINDOW,
        "only the window is resolved, not the whole library-scaled tail"
    );
    assert_eq!(
        context.upcoming_total,
        track_ids.len() as u64,
        "upcoming_total reports the full tail length"
    );
    let resolved_track_ids: Vec<&str> = context
        .upcoming
        .iter()
        .map(|i| i.track_id.as_str())
        .collect();
    let expected: Vec<&str> = track_ids[..crate::queue::QUEUE_UPCOMING_WINDOW]
        .iter()
        .map(String::as_str)
        .collect();
    assert_eq!(resolved_track_ids, expected, "the window preserves order");
    assert_eq!(
        snapshot.revision, 7,
        "the snapshot carries the projection's revision"
    );
}

/// A context tail shorter than the window resolves in full, and
/// `upcoming_total` still matches its real (smaller) length.
#[tokio::test]
async fn resolve_queue_projection_shorter_than_window_resolves_it_all() {
    let (manager, _temp_dir) = setup_test_manager().await;
    let track_ids = seed_release_tracks(&manager, 5).await;

    let projection = crate::playback::PlaybackQueueProjection {
        manual: Vec::new(),
        context: Some(context_projection_over(&track_ids)),
        has_next: false,
        has_previous: false,
        revision: 1,
    };
    let snapshot = manager.resolve_queue_projection(projection).await.unwrap();
    let context = snapshot.context.expect("a context was set");
    assert_eq!(context.upcoming.len(), 5);
    assert_eq!(context.upcoming_total, 5);
}

/// The manual lane is explicit and user-curated, not library-scaled — it
/// resolves in full even when it is larger than the context window.
#[tokio::test]
async fn resolve_queue_projection_resolves_manual_lane_in_full_regardless_of_window() {
    let (manager, _temp_dir) = setup_test_manager().await;
    let track_ids = seed_release_tracks(&manager, crate::queue::QUEUE_UPCOMING_WINDOW + 10).await;

    let manual_count = crate::queue::QUEUE_UPCOMING_WINDOW + 3;
    let manual: Vec<crate::playback::QueueEntry> = track_ids[..manual_count]
        .iter()
        .enumerate()
        .map(|(i, t)| crate::playback::QueueEntry {
            id: crate::playback::QueueEntryId(format!("m{i}")),
            track_id: t.clone(),
        })
        .collect();

    let projection = crate::playback::PlaybackQueueProjection {
        manual,
        context: None,
        has_next: false,
        has_previous: false,
        revision: 0,
    };
    let snapshot = manager.resolve_queue_projection(projection).await.unwrap();
    assert_eq!(
        snapshot.manual.len(),
        manual_count,
        "the manual lane is never windowed"
    );
}

/// A peer's `change_cover` writes exactly one row — the `covers` row — so that is
/// the whole changeset this device receives. The applied changeset has to reach the
/// album, and what has to arrive is the UI's art cache key: the cover `ImageRef`'s
/// version (`covers._updated_at`). Before this was handled the changeset was
/// dropped, and the receiving device kept rendering the old art indefinitely.
///
/// The peer here *replaces* an existing cover — a fresh `blob_id` repointing the
/// same row — which is exactly what `change_cover` does, so the version the event
/// carries has to be the new one, not the one the device already had.
#[cfg(feature = "test-utils")]
#[tokio::test]
async fn a_peers_lone_cover_change_emits_an_album_update_carrying_the_new_cache_key() {
    let (manager, _temp_dir) = setup_test_manager().await;
    let album = create_test_album();
    let release = create_test_release(&album.id);
    manager.database.insert_album(&album).await.unwrap();
    manager.database.insert_release(&release).await.unwrap();

    // The cover this device already has, and the cache key it renders it under.
    store_test_cover_image(&manager, &release.id).await;
    let stale_version = manager
        .find_album_detail(&album.id)
        .await
        .unwrap()
        .expect("album detail")
        .cover
        .expect("the release starts with a cover")
        .version;

    // The peer's `change_cover`, as it lands here: the same row repointed at a new
    // blob, and its `_updated_at` moves.
    store_test_cover_image_with_blob(&manager, &release.id, "replacement-blob").await;
    let expected_version = manager
        .database
        .cover_version(&release.id)
        .await
        .unwrap()
        .expect("the cover row has a version");
    assert_ne!(
        expected_version, stale_version,
        "replacing the cover must move its version, or this test proves nothing"
    );

    // Feed the changeset the peer would have sent: the `covers` row, alone.
    let mut rx = manager.subscribe_events();
    let (changes, _missing_fk) =
        crate::library::sync_events::changes_from_row_changes(&[coven::RowChange {
            table: "covers".to_string(),
            op: coven::ChangeOp::Update,
            columns: vec![Some(release.id.clone())],
        }]);
    manager.emit_sync_entity_changes(changes).await;

    let updated = tokio::time::timeout(std::time::Duration::from_secs(2), async {
        loop {
            match rx.recv().await {
                Ok(LibraryEvent::AlbumUpdated { album }) => return album,
                Ok(_) => continue,
                Err(e) => panic!("library event channel closed before AlbumUpdated: {e}"),
            }
        }
    })
    .await
    .expect("a lone cover change must emit an album update");

    assert_eq!(updated.album.id, album.id);
    let cover = updated
        .cover
        .expect("the album update carries its cover ref");
    assert_eq!(
        cover.version, expected_version,
        "the album update must carry the NEW cover version — that ref is the art cache key"
    );
    assert_ne!(
        cover.version, stale_version,
        "carrying the old version would re-render the stale art"
    );
}

/// Setting up an opaque cloud home establishes the master key in the keyring, and
/// only then connects the provider. If the connect fails, the key is still in the
/// keyring — so the config has to say so. It used to record the fingerprint only
/// after a successful connect, which left `encryption_key_stored` false over a
/// keyring that held the key: the launch gate
/// (`encryption_key_stored && keyring-has-key`) then never attached sync again,
/// while the provider stayed configured and the UI reported it connected.
///
/// The test manager is built with no CloudKit driver, so the connect fails at the
/// driver lookup — a failed connect with no network in it.
#[cfg(feature = "test-utils")]
#[tokio::test]
async fn a_failed_connect_still_records_the_key_it_put_in_the_keyring() {
    let (manager, _temp_dir) = setup_test_manager().await;

    let error = manager
        .use_cloudkit(crate::config::HomeStorage::Opaque)
        .await
        .expect_err("no CloudKit driver is installed, so the connect must fail");
    assert!(
        error.to_string().contains("CloudKit driver not provided"),
        "the failure must be the connect, not the key step: {error}"
    );

    assert!(
        manager.has_encryption(),
        "the master key really is established in the keyring"
    );
    let config = manager.get_config();
    assert!(
        config.encryption_key_stored,
        "the config must agree with the keyring, or the next launch never attaches sync"
    );
    assert!(
        config.encryption_key_fingerprint.is_some(),
        "the recorded key carries its fingerprint"
    );
}

/// Cancelling a release's upload has to leave the durable state telling the truth:
/// the release is Local again, coven's make-Remote intent is gone, and its outbox
/// carries no pending uploads. That outbox — not a status column — is what a restart
/// reads to know an import is still uploading, and it is what the Processing pane
/// renders. bae used to keep a second copy of that fact in an `imports.status`
/// column that the cancel never touched (so it stayed `importing` forever) and that
/// nothing ever read back; this pins the fact to the place that is actually correct.
#[cfg(feature = "test-utils")]
#[tokio::test]
async fn cancelling_an_upload_leaves_no_in_flight_import_behind() {
    let (manager, temp_dir) = setup_test_manager().await;
    connect_test_cloud(&manager).await;
    let release = insert_partially_uploaded_make_remote_release(&manager, temp_dir.path()).await;

    manager.cancel_release_upload(&release.id).await.unwrap();

    assert!(
        manager
            .database
            .make_remote_progress_for_release(&release.id)
            .await
            .unwrap()
            .is_none(),
        "the cancel clears coven's make-Remote intent"
    );
    assert!(
        manager
            .database
            .handle()
            .queued_uploads()
            .await
            .unwrap()
            .is_empty(),
        "no upload is left queued, so nothing reads as still importing"
    );
    let after = manager
        .database
        .find_release_by_id(&release.id)
        .await
        .unwrap()
        .expect("the release survives the cancel");
    assert!(!after.remote, "the cancelled release stays Local");
}
