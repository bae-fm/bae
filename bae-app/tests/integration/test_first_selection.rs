//! How long the import pane takes to show its first value in a fresh
//! library, then the first candidate the person selects, then a second one:
//! the read of each, and the statements, steps and scope plans it ran. The
//! library holds 10,000 albums of 10 tracks and one watched folder of 1,000
//! identified candidates. Run explicitly in Debug:
//!
//!   BUILDS_CARGO_HOST=local scripts/cargo.sh test -p bae-app --features test-utils \
//!     --test integration -- test_first_selection:: --ignored --nocapture
//!
//! `scripts/perf.sh run first-selection` runs it in the release profile.
//! It prints the time of the first value and of each selection, and their
//! statements, steps and scope plans, as costs.

use bae_import::CandidateKey;
use bae_library::{
    FixtureAlbum, FixtureCandidate, FixtureCandidateState, FixtureWatchedFolder, LibraryFixture,
};
use std::time::{Duration, Instant};

const ALBUMS: usize = 10_000;
const TRACKS_PER_ALBUM: usize = 10;
const ARTISTS: usize = 1_000;
const CANDIDATES: usize = 1_000;

/// How long the pane may take to show a candidate.
const SETTLE: Duration = Duration::from_secs(120);

fn fixture(root: &std::path::Path) -> LibraryFixture {
    let albums = (0..ALBUMS)
        .map(|album| FixtureAlbum {
            title: format!("Album {album:05}"),
            artists: vec![format!("Artist {:04}", album % ARTISTS)],
            tracks: (0..TRACKS_PER_ALBUM).map(|track| format!("Track {track:02}")).collect(),
            cover: None,
        })
        .collect();
    let candidates = (0..CANDIDATES)
        .map(|candidate| FixtureCandidate {
            folder: format!("Folder {candidate:04}"),
            tracks: vec!["01.flac".to_string()],
            state: FixtureCandidateState::Identified,
        })
        .collect();
    LibraryFixture {
        albums,
        watched_folders: vec![FixtureWatchedFolder {
            path: root.to_string_lossy().into_owned(),
            candidates,
        }],
        playing: None,
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "a measurement of the import pane's first selection, run on its own"]
async fn the_import_pane_first_selection() {
    let temp = bae_util::test_files::TestFolder::new();
    let library = temp.create_folder("library");
    let test_library = bae_library::test_libraries::open_test_library(&library).await;
    test_library.manager().set_identify_automatically(false).await.unwrap();
    let root = temp.path().join("watched");
    test_library.manager().write_fixture(&fixture(&root)).await.expect("write the fixture");
    let services = bae_app::test_services::start_services(&test_library)
        .await
        .expect("the test library's services start");

    let work = test_library.store().track_work_for_test().await.unwrap();
    work.take();
    let started = Instant::now();
    let mut pane = services.subscribe_import_candidate_query_for_test();
    tokio::time::timeout(SETTLE, pane.next_inline())
        .await
        .expect("the pane's first read")
        .expect("the pane's query runs");
    report("first_selection.import_pane.open", started.elapsed(), &work.take());

    for (metric, candidate) in [("first", 0), ("second", 1)] {
        let key = CandidateKey::folder(&root.join(format!("Folder {candidate:04}")));
        work.take();
        let started = Instant::now();
        pane.requests().set(Some(key.clone())).unwrap();
        tokio::time::timeout(SETTLE, async {
            loop {
                let event = pane.next_inline().await.expect("the pane's query runs");
                if event.request.as_ref() == Some(&key)
                    && event.result.expect("the pane reads").is_some()
                {
                    break;
                }
            }
        })
        .await
        .unwrap_or_else(|_| panic!("the pane did not show candidate {candidate} in {SETTLE:?}"));
        let metric = format!("first_selection.import_pane.{metric}");
        report(&metric, started.elapsed(), &work.take());
    }

    services.close().await.unwrap();
}

/// Report `metric`, work that took `elapsed` and did `counts`.
fn report(metric: &str, elapsed: Duration, counts: &coven::WorkCounts) {
    let report = bae_store::test_fixtures::window_cost::report_cost;
    report(metric, elapsed.as_secs_f64() * 1_000.0, "ms");
    report(&format!("{metric}.app_read_statements"), counts.app_read_statements as f64, "count");
    report(&format!("{metric}.app_read_steps"), counts.app_read_steps as f64, "count");
    report(&format!("{metric}.scope_plans_built"), counts.scope_plans_built as f64, "count");
    report(&format!("{metric}.scope_plan_sources"), counts.scope_plan_size.sources as f64, "count");
    report(&format!("{metric}.scope_plan_cores"), counts.scope_plan_size.cores as f64, "count");
    report(&format!("{metric}.scope_plan_edges"), counts.scope_plan_size.edges as f64, "count");
    report(
        &format!("{metric}.scope_plan_time"),
        counts.scope_plan_time.as_secs_f64() * 1_000.0,
        "ms",
    );
}
