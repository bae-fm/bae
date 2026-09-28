// Fixture row ids are v4 UUIDs because coven checks every synced row's
// primary key is one.
const REL_1: &str = "cccb6034-5922-40d2-8d0b-d94619230882";
const TRACK_1: &str = "f2f77437-aa03-4583-8b1c-d12bcf984967";

use super::*;
use crate::db::{Database, DbArtist};
use crate::library::LibraryError;
use chrono::Utc;
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
    setup_test_manager_with(crate::util::http::Http::for_test()).await
}

/// A manager whose providers and image downloads send requests through `http`.
async fn setup_test_manager_with(http: crate::util::http::Http) -> (LibraryManager, TempDir) {
    let temp_dir = TempDir::new().unwrap();
    let db_path = temp_dir.path().join("test.db");
    let database = Database::new_test(
        db_path.to_str().unwrap(),
        std::sync::Arc::new(coven::SystemClock),
    )
    .await
    .unwrap();
    let library_dir = coven::StoreDir::new(temp_dir.path());
    let config_handle = test_config(&library_dir);
    let manager = LibraryManager::new(
        database,
        crate::config::AppDir::under_home(temp_dir.path()),
        config_handle,
        std::sync::Arc::new(coven::SystemClock),
        std::sync::Arc::new(coven::UuidProvider),
        crate::diagnostics::Diagnostics::noop(),
        tokio::runtime::Handle::current(),
        crate::import::cover_art::RemoteImageCache::for_test(http.clone()),
        crate::providers::Providers::for_test(http),
    );
    (manager, temp_dir)
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

/// What the import `import_id` reported: the release and album it created, or
/// the error it failed with. Every other event on the stream is skipped.
async fn await_import_outcome(
    events: &mut tokio::sync::mpsc::UnboundedReceiver<ImportEvent>,
    import_id: &str,
) -> Result<(String, String), String> {
    loop {
        let event = tokio::time::timeout(std::time::Duration::from_secs(30), events.recv())
            .await
            .expect("the import reports its result")
            .expect("the import event stream remains open");
        let ImportEvent::ImportProgress { progress, .. } = event else {
            continue;
        };
        match progress {
            crate::import::ImportProgress::Complete {
                import_id: completed,
                id,
                album_id,
            } if completed == import_id => return Ok((id, album_id)),
            crate::import::ImportProgress::Failed {
                import_id: failed,
                error,
            } if failed == import_id => return Err(error),
            _ => {}
        }
    }
}

/// The event feed holds every event sent after it was taken, however many,
/// and only one reader takes it.
#[test]
fn the_event_feed_holds_every_event_and_is_taken_once() {
    let bus = ImportEventBus::new(CandidateRuntime::default());
    let mut feed = bus.take_feed().expect("the feed is there to take");
    for _ in 0..3 {
        bus.send(ImportEvent::Scan(ScanEvent::Finished));
    }

    let held = std::iter::from_fn(|| feed.try_recv().ok())
        .filter(|event| matches!(event, ImportEvent::Scan(ScanEvent::Finished)))
        .count();
    assert_eq!(held, 3, "the feed dropped none of them");
    assert!(bus.take_feed().is_none(), "the feed is taken once");
}

/// A reader that hears an import end and then asks the runtime finds the
/// claim already released: the bus records an event before any reader hears
/// it.
#[tokio::test]
async fn an_import_outcome_is_recorded_before_any_reader_hears_it() {
    let runtime = CandidateRuntime::default();
    let bus = ImportEventBus::new(runtime.clone());
    let mut events = bus.every_event();
    runtime.claim_for_import("candidate", "import-1").unwrap();
    assert!(runtime
        .get("candidate")
        .is_some_and(|candidate| candidate.import.is_some()));

    bus.send(ImportEvent::ImportProgress {
        candidate_key: "candidate".to_string(),
        progress: crate::import::ImportProgress::Complete {
            id: REL_1.to_string(),
            import_id: "import-1".to_string(),
            album_id: "album-1".to_string(),
        },
    });

    let outcome = events.recv().await.unwrap();
    assert!(matches!(
        outcome,
        ImportEvent::ImportProgress {
            progress: crate::import::ImportProgress::Complete { .. },
            ..
        }
    ));
    assert!(runtime
        .get("candidate")
        .is_none_or(|candidate| candidate.import.is_none()));
}

include!("tests/identity.rs");
include!("tests/edit_shape.rs");
include!("tests/candidate_state.rs");
include!("tests/pane.rs");
mod combinations;
mod parent_files;
