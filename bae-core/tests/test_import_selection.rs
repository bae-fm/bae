#![cfg(feature = "test-utils")]
//! The import list's selection through the app: a bulk action runs in core
//! over the selected keys, and a library reopens with nothing selected.

use bae_core::import::selection::SelectionSummary;
use bae_core::import::{CandidateAction, ImportListView};
use bae_core::library::AppServices;
use bae_test_support as support;
use std::path::Path;
use tempfile::TempDir;

/// A library at `dir` with automatic identification off, so nothing but the
/// test acts on the candidates.
async fn open(dir: &Path) -> AppServices {
    let (manager, _db) = support::open_test_library(dir).await;
    manager.set_identify_automatically(false).await.unwrap();
    AppServices::for_test(manager).await.unwrap()
}

/// `count` one-track albums under `collection`, watched and scanned.
async fn scan_albums(services: &AppServices, collection: &Path, count: usize) {
    let flac = std::fs::read(bae_test_support::fixture_dir!(
        "flac",
        "01 Test Track 1.flac"
    ))
    .expect("FLAC fixture");
    for n in 0..count {
        let album = collection.join(format!("Album {n}"));
        std::fs::create_dir_all(&album).unwrap();
        std::fs::write(album.join("01 Track.flac"), &flac).unwrap();
    }
    services
        .import_add_watched_folder(collection.to_string_lossy().into_owned())
        .await
        .unwrap();
    let list = services.subscribe_import_list(
        ImportListView::default(),
        &tokio::runtime::Handle::current(),
    );
    tokio::time::timeout(std::time::Duration::from_secs(10), async {
        loop {
            let snapshot = list.next().await.expect("the list answers");
            if snapshot.summary.counts.pending as usize == count {
                return;
            }
        }
    })
    .await
    .expect("the scan lists every album");
}

async fn summary(services: &AppServices) -> SelectionSummary {
    services
        .subscribe_import_selection(&tokio::runtime::Handle::current())
        .recv()
        .await
        .expect("the selection answers")
}

/// Skipping the selection runs in core over every selected row, and each
/// skipped row leaves the selection with its skip.
#[tokio::test(flavor = "multi_thread")]
async fn a_bulk_action_runs_over_every_selected_row() {
    support::tracing_init();
    let tmp = TempDir::new().unwrap();
    let services = open(&tmp.path().join("db")).await;
    scan_albums(&services, &tmp.path().join("Collection"), 3).await;

    services
        .select_all_import_candidates(ImportListView::default())
        .await
        .unwrap();
    assert_eq!(summary(&services).await.count, 3);

    let failures = services
        .run_import_selection_action(CandidateAction::Skip, |_, _| {})
        .await
        .unwrap();

    assert!(failures.is_empty(), "{failures:?}");
    assert_eq!(summary(&services).await.count, 0);
    let pending = services
        .load_import_list(ImportListView::default(), Default::default())
        .await
        .unwrap();
    assert_eq!(pending.summary.counts.pending, 0);
    assert_eq!(pending.summary.counts.skipped, 3);
    services.close().await;
}

/// The selection lasts one app session: a library reopens with none.
#[tokio::test(flavor = "multi_thread")]
async fn a_reopened_library_has_nothing_selected() {
    support::tracing_init();
    let tmp = TempDir::new().unwrap();
    let db_dir = tmp.path().join("db");
    let services = open(&db_dir).await;
    scan_albums(&services, &tmp.path().join("Collection"), 2).await;
    services
        .select_all_import_candidates(ImportListView::default())
        .await
        .unwrap();
    assert_eq!(summary(&services).await.count, 2);
    services.close().await;

    let reopened = open(&db_dir).await;
    assert_eq!(summary(&reopened).await.count, 0);
    reopened.close().await;
}
