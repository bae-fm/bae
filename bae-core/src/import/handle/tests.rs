// Fixture row ids are v4 UUIDs because coven checks every synced row's
// primary key is one.
const REL_1: &str = "cccb6034-5922-40d2-8d0b-d94619230882";
const TRACK_1: &str = "f2f77437-aa03-4583-8b1c-d12bcf984967";

use super::*;
use crate::db::{Database, DbArtist};
use crate::library::LibraryError;
use bae_test_support::HANG_GUARD;
use chrono::Utc;
use std::path::{Path, PathBuf};
use tempfile::TempDir;
use uuid::Uuid;

async fn setup_test_manager() -> (LibraryManager, TempDir) {
    setup_test_manager_with(bae_util::http::Http::for_test()).await
}

/// A manager whose providers and image downloads send requests through `http`.
async fn setup_test_manager_with(http: bae_util::http::Http) -> (LibraryManager, TempDir) {
    let temp_dir = TempDir::new().unwrap();
    let db_path = temp_dir.path().join("test.db");
    let database = Database::new_test(
        db_path.to_str().unwrap(),
        std::sync::Arc::new(coven::SystemClock),
    )
    .await
    .unwrap();
    let config_handle = Arc::new(crate::config::ConfigHandle::new(
        crate::config::Config::with_defaults(
            database.store_id(),
            temp_dir.path().join("preferences"),
        ),
    ));
    let manager = LibraryManager::new(
        database,
        crate::config::AppDir::under_home(temp_dir.path()).unwrap(),
        config_handle,
        std::sync::Arc::new(coven::SystemClock),
        std::sync::Arc::new(coven::UuidIds),
        crate::diagnostics::Diagnostics::noop(),
        tokio::runtime::Handle::current(),
        bae_catalogs::remote_image::RemoteImageCache::for_test(http.clone()),
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
        let event = tokio::time::timeout(HANG_GUARD, events.recv())
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
#[tokio::test]
async fn the_event_feed_holds_every_event_and_is_taken_once() {
    let bus = ImportEventBus::new(CandidateRuntime::for_test().await);
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

/// A reader that hears how far an import has got and then asks the runtime
/// finds it already there: the bus records an event before any reader hears
/// it.
#[tokio::test]
async fn an_import_s_progress_is_recorded_before_any_reader_hears_it() {
    let runtime = CandidateRuntime::for_test().await;
    let bus = ImportEventBus::new(runtime.clone());
    let mut events = bus.every_event();
    runtime
        .claim_for_import(
            &crate::import::CandidateKey::test("candidate"),
            "import-1",
            false,
        )
        .await
        .unwrap();
    runtime
        .move_import(
            &crate::import::CandidateKey::test("candidate"),
            "import-1",
            Some(crate::import::ImportStanding::Running),
        )
        .await
        .unwrap();

    bus.send(ImportEvent::ImportProgress {
        candidate_key: crate::import::CandidateKey::test("candidate"),
        progress: crate::import::ImportProgress::Progress {
            id: REL_1.to_string(),
            percent: Some(40),
            phase: crate::import::ImportPhase::ReadingFiles,
            import_id: "import-1".to_string(),
        },
    });

    events.recv().await.unwrap();
    assert_eq!(
        runtime
            .snapshot(&crate::import::CandidateKey::test("candidate"))
            .await
            .unwrap()
            .and_then(|candidate| candidate.import)
            .map(|import| import.step),
        Some(crate::import::ImportStep::Running(
            crate::import::ImportPhase::ReadingFiles
        ))
    );
}

/// Put `key` on the identification queue as the queue admits it, with the
/// files the scan holds it with.
async fn admit(handle: &ImportServiceHandle, key: &crate::import::CandidateKey) {
    let candidate = handle
        .get_release_candidate(key)
        .await
        .unwrap()
        .expect("the candidate is stored");
    handle
        .admit_identification(
            vec![(key.clone(), crate::import::CandidateFiles::of(&candidate))],
            crate::import::Admission::Requested,
        )
        .await
        .unwrap();
}

/// Start `run` of the queued `key`, as the identify driver records its first
/// state.
async fn start_run(
    handle: &ImportServiceHandle,
    key: &crate::import::CandidateKey,
    run: crate::identify::IdentifyRunId,
) {
    let candidate = handle
        .get_release_candidate(key)
        .await
        .unwrap()
        .expect("the candidate is stored");
    handle.runtime.record_run_state(
        key,
        run,
        true,
        &crate::import::candidate_runtime::tests::triangulating(),
    );
    assert!(handle
        .runtime
        .run_started(key, &crate::import::CandidateFiles::of(&candidate), run)
        .await
        .unwrap());
}

include!("tests/identity.rs");
include!("tests/edit_shape.rs");
include!("tests/candidate_state.rs");
include!("tests/pane.rs");
include!("tests/draft_values.rs");
include!("tests/album_pick.rs");
mod combinations;
mod parent_files;
