// Fixture row ids. coven validates every synced row's primary key as a
// canonical v4 UUID (`RowIdentity::IndependentUuid`), which is what bae's
// real ids are, so these fixtures carry UUIDs too. Each constant is named
// for the moniker it replaced, so assertions still read by name.
const ARTIST_ACTUAL_1: &str = "f83b9e90-bd64-470f-82e6-cf28db1996a3"; // was "artist-actual-1"
const REL_1: &str = "cccb6034-5922-40d2-8d0b-d94619230882"; // was "e6cdc1f3-3a7b-473e-86aa-fe093cc5e94e"
const TRACK_1: &str = "f2f77437-aa03-4583-8b1c-d12bcf984967"; // was "track-1"

use super::*;
use crate::db::{Database, DbArtist};
use crate::test_logs::capture_warn_logs_async;
use chrono::Utc;
use serial_test::serial;
use std::path::{Path, PathBuf};
use tempfile::TempDir;
use uuid::Uuid;

fn test_config(library_dir: &coven::StoreDir) -> std::sync::Arc<crate::config::ConfigHandle> {
    // Unique id per test so keyring entries don't collide in the shared
    // process-global mock store (see `install_test_keyring`).
    let library_id = format!("test-{}", uuid::Uuid::new_v4());
    let config = crate::config::Config::with_defaults(
        library_id.clone(),
        "test-device".to_string(),
        library_dir.clone(),
        "Test Library".to_string(),
    );
    crate::config::install_test_keyring();
    std::sync::Arc::new(crate::config::ConfigHandle::new(config))
}

async fn setup_test_manager() -> (LibraryManager, TempDir) {
    let temp_dir = TempDir::new().unwrap();
    let db_path = temp_dir.path().join("test.db");
    let database = Database::new_test(
        db_path.to_str().unwrap(),
        std::sync::Arc::new(coven::SystemClock),
        std::sync::Arc::new(coven::UuidProvider),
    )
    .await
    .unwrap();
    let library_dir = coven::StoreDir::new(temp_dir.path());
    let config_handle = test_config(&library_dir);
    let manager = LibraryManager::new(
        database,
        config_handle,
        std::sync::Arc::new(coven::SystemClock),
        std::sync::Arc::new(coven::UuidProvider),
        crate::diagnostics::Diagnostics::noop(),
        tokio::runtime::Handle::current(),
        crate::import::cover_art::RemoteImageCache::for_test(),
    );
    (manager, temp_dir)
}

fn unresolved_boundary(root: &Path, relative_path: &str) -> FolderReleaseBoundary {
    FolderReleaseBoundary {
        key: FolderReleaseDecisionKey {
            watched_folder_path: root.to_string_lossy().into_owned(),
            relative_folder_path: relative_path.to_string(),
        },
        name: "Collection".to_string(),
        display_path: relative_path.to_string(),
        shared_file_count: 0,
        tree_rows: Vec::new(),
        candidate_keys: Vec::new(),
    }
}

fn make_artist(name: &str, discogs_id: Option<&str>, mb_id: Option<&str>) -> DbArtist {
    let now = Utc::now();
    DbArtist {
        id: Uuid::new_v4().to_string(),
        name: name.to_string(),
        sort_name: None,
        discogs_artist_id: discogs_id.map(|s| s.to_string()),
        musicbrainz_artist_id: mb_id.map(|s| s.to_string()),
        created_at: now,
    }
}

include!("tests/identity.rs");
include!("tests/edit_shape.rs");
include!("tests/candidate_state.rs");
