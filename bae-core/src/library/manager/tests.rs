// Fixture row ids. coven validates every synced row's primary key as a
// canonical v4 UUID (`RowIdentity::IndependentUuid`), which is what bae's
// real ids are, so these fixtures carry UUIDs too. Each constant is named
// for the moniker it replaced, so assertions still read by name.
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

use serial_test::serial;

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
    let library_dir = StoreDir::new(temp_dir.path().to_path_buf());
    let config = Config::with_defaults(
        library_id.to_string(),
        "test-device".to_string(),
        library_dir.clone(),
        "Test Library".to_string(),
    );
    crate::config::install_test_keyring();
    assemble_test_manager(temp_dir, config).await
}

#[cfg(feature = "test-utils")]
async fn setup_browsable_test_manager() -> (LibraryManager, TempDir) {
    let library_id = format!("test-{}", Uuid::new_v4());
    let temp_dir = TempDir::new().unwrap();
    let library_dir = StoreDir::new(temp_dir.path().to_path_buf());
    let mut config = Config::with_defaults(
        library_id.clone(),
        "test-device".to_string(),
        library_dir.clone(),
        "Test Library".to_string(),
    );
    config.cloud_home.storage = crate::config::HomeStorage::Browsable;
    crate::config::install_test_keyring();
    assemble_test_manager(temp_dir, config).await
}

async fn assemble_test_manager(temp_dir: TempDir, config: Config) -> (LibraryManager, TempDir) {
    let config_handle = Arc::new(ConfigHandle::new(config));
    let manager = LibraryManager::open(
        config_handle,
        Arc::new(coven::SystemClock),
        Arc::new(coven::UuidProvider),
        crate::diagnostics::Diagnostics::noop(),
        tokio::runtime::Handle::current(),
        None,
        crate::import::cover_art::RemoteImageCache::for_test(),
    )
    .expect("open test library manager through the production object graph");

    // Insert the test artist that create_test_album() references
    let artist = DbArtist {
        id: bae_test_support::test_uuid("e36744a5-1a36-460f-891c-e7e558034edf"),
        name: "Test Artist".to_string(),
        sort_name: None,
        discogs_artist_id: None,
        musicbrainz_artist_id: None,
        created_at: Utc::now(),
    };
    manager.database.insert_artist(&artist).await.unwrap();
    (manager, temp_dir)
}

// One-line reads and writes against the manager's database. Spelled out at a
// call site each is `manager.database.x(..).await.unwrap()`, which rustfmt
// breaks across five lines.
async fn find_release(manager: &LibraryManager, release_id: &str) -> Option<DbRelease> {
    manager
        .database
        .find_release_by_id(release_id)
        .await
        .unwrap()
}

async fn find_album(manager: &LibraryManager, album_id: &str) -> Option<DbAlbum> {
    manager.database.find_album_by_id(album_id).await.unwrap()
}

async fn release_files(manager: &LibraryManager, release_id: &str) -> Vec<DbFile> {
    manager
        .database
        .get_files_for_release(release_id)
        .await
        .unwrap()
}

async fn album_releases(manager: &LibraryManager, album_id: &str) -> Vec<DbRelease> {
    manager
        .database
        .get_releases_for_album(album_id)
        .await
        .unwrap()
}

async fn make_remote_progress(
    manager: &LibraryManager,
    release_id: &str,
) -> Option<coven::MakeRemoteProgress> {
    manager
        .database
        .make_remote_progress_for_release(release_id)
        .await
        .unwrap()
}

async fn has_pending_uploads(manager: &LibraryManager, release_id: &str) -> bool {
    manager
        .database
        .has_pending_uploads_for_release(release_id)
        .await
        .unwrap()
}

async fn queued_upload_count(manager: &LibraryManager) -> usize {
    manager
        .database
        .queued_upload_count_for_test()
        .await
        .unwrap()
}

async fn queued_delete_count(manager: &LibraryManager) -> usize {
    manager
        .database
        .queued_delete_count_for_test()
        .await
        .unwrap()
}

async fn insert_release(manager: &LibraryManager, release: &DbRelease) {
    manager.database.insert_release(release).await.unwrap();
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
        metadata_provenance: Some(crate::import::MetadataProvenance::FileTags),
        remote: true,
        source_folder_name: None,
        content_hash: None,
        album_loudness_lufs: None,
        album_peak_linear: None,
        created_at: Utc::now(),
    }
}

/// A manager holding one album with one release — the spine most of these
/// tests start from before adding tracks, files, or covers of their own. The
/// `TempDir` owns the library directory, so callers keep it.
async fn manager_with_release() -> (LibraryManager, TempDir, DbAlbum, DbRelease) {
    let (manager, temp_dir) = setup_test_manager().await;
    let album = create_test_album();
    let release = create_test_release(&album.id);
    manager.database.insert_album(&album).await.unwrap();
    insert_release(&manager, &release).await;
    (manager, temp_dir, album, release)
}

include!("tests/save_and_config.rs");
include!("tests/deletion.rs");
include!("tests/release_details.rs");
include!("tests/release_edit.rs");
include!("tests/detail_subscriptions.rs");
include!("tests/storage_fixtures.rs");
include!("tests/storage.rs");
include!("tests/storage_sorting.rs");
include!("tests/transfers.rs");
include!("tests/transfer_order.rs");
include!("tests/downloads.rs");
include!("tests/output.rs");
include!("tests/identity.rs");
include!("tests/artist_resolution.rs");
include!("tests/playback_and_sync.rs");
