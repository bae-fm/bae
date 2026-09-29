#![cfg(feature = "test-utils")]
//! The import list's selection through the app: a bulk action runs in core
//! over the selected keys in the order the list shows them, and a library
//! reopens with nothing selected.

use bae_core::import::selection::{SelectionChange, SelectionSummary};
use bae_core::import::{
    Admission, ArtistAssignment, CandidateAction, CandidateRuntimeChange, CandidateRuntimeSnapshot,
    ImportListItem, ImportListOrder, ImportListView,
};
use bae_core::library::{AppServices, LibraryPageWindow};
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

/// `count` one-track albums under `collection`, watched and scanned. Each
/// track has its own name, so no two albums are the same files.
async fn scan_albums(services: &AppServices, collection: &Path, count: usize) {
    let flac = std::fs::read(bae_test_support::fixture_dir!(
        "flac",
        "01 Test Track 1.flac"
    ))
    .expect("FLAC fixture");
    for n in 0..count {
        let album = collection.join(format!("Album {n}"));
        std::fs::create_dir_all(&album).unwrap();
        std::fs::write(album.join(format!("01 Track {n}.flac")), &flac).unwrap();
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
        .run_import_selection_action(ImportListView::default(), CandidateAction::Skip, |_, _| {})
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

/// The list sorted by path, last to first, so the order it shows is the
/// reverse of the keys' own.
fn descending() -> ImportListView {
    ImportListView {
        order: ImportListOrder::PathDescending,
        ..ImportListView::default()
    }
}

/// The candidate keys `view` shows, top to bottom.
async fn shown_keys(services: &AppServices, view: ImportListView) -> Vec<String> {
    let list = services
        .load_import_list(
            view,
            std::iter::once(LibraryPageWindow {
                offset: 0,
                limit: 50,
            })
            .collect(),
        )
        .await
        .unwrap();
    list.windows
        .into_iter()
        .flat_map(|window| window.items)
        .filter_map(|item| match item {
            ImportListItem::Candidate { row, .. } => Some(row.candidate_key),
            _ => None,
        })
        .collect()
}

/// Select the shown rows `picks` names, by their place in the list and out of
/// order, and return the selected keys in the order the list shows them.
async fn select_out_of_order(
    services: &AppServices,
    view: ImportListView,
    picks: &[usize],
) -> Vec<String> {
    let shown = shown_keys(services, view.clone()).await;
    services
        .change_import_selection(
            view,
            SelectionChange::Toggle {
                add: picks.iter().map(|&pick| shown[pick].clone()).collect(),
                remove: Vec::new(),
            },
        )
        .await
        .unwrap();
    let mut in_view_order = picks.to_vec();
    in_view_order.sort_unstable();
    in_view_order
        .into_iter()
        .map(|pick| shown[pick].clone())
        .collect()
}

/// The keys the runtime's changes bring to `reached`, in the order they get
/// there, until `count` have.
async fn reached_in_order(
    changes: &mut tokio::sync::mpsc::UnboundedReceiver<CandidateRuntimeChange>,
    count: usize,
    reached: impl Fn(&CandidateRuntimeSnapshot) -> bool,
) -> Vec<String> {
    let mut order: Vec<String> = Vec::new();
    tokio::time::timeout(std::time::Duration::from_secs(20), async {
        while order.len() < count {
            let arrivals: Vec<String> = match changes.recv().await.expect("the runtime answers") {
                CandidateRuntimeChange::Updated { key, runtime } => (reached(&runtime)
                    && !order.contains(&key))
                .then_some(key)
                .into_iter()
                .collect(),
                CandidateRuntimeChange::Reset { runtimes } => runtimes
                    .into_iter()
                    .filter(|(key, runtime)| reached(runtime) && !order.contains(key))
                    .map(|(key, _)| key)
                    .collect(),
                CandidateRuntimeChange::Removed { .. } => Vec::new(),
            };
            assert!(
                arrivals.len() <= 1,
                "one change brings one candidate at a time: {arrivals:?}"
            );
            order.extend(arrivals);
        }
    })
    .await
    .expect("every candidate gets there");
    order
}

/// A bulk Identify over rows selected out of order admits them in the order
/// the list shows them, not the keys' own order, so the top rows start first.
#[tokio::test(flavor = "multi_thread")]
async fn a_bulk_identify_admits_the_rows_as_the_list_shows_them() {
    support::tracing_init();
    let tmp = TempDir::new().unwrap();
    let services = open(&tmp.path().join("db")).await;
    scan_albums(&services, &tmp.path().join("Collection"), 4).await;
    let expected = select_out_of_order(&services, descending(), &[3, 0, 2]).await;

    let mut changes = services.every_candidate_runtime_change_for_test();
    let failures = services
        .run_import_selection_action(descending(), CandidateAction::Identify, |_, _| {})
        .await
        .unwrap();
    assert!(failures.is_empty(), "{failures:?}");

    let admitted = reached_in_order(&mut changes, expected.len(), |runtime| {
        runtime.queued == Some(Admission::Requested)
    })
    .await;
    assert_eq!(admitted, expected);
    services.close().await;
}

/// A bulk Import over rows selected out of order starts them in the order the
/// list shows them.
#[tokio::test(flavor = "multi_thread")]
async fn a_bulk_import_starts_the_rows_as_the_list_shows_them() {
    support::tracing_init();
    let tmp = TempDir::new().unwrap();
    let services = open(&tmp.path().join("db")).await;
    scan_albums(&services, &tmp.path().join("Collection"), 4).await;
    for key in shown_keys(&services, descending()).await {
        services
            .import_select_candidate_file_tags(key.clone())
            .await
            .unwrap();
        services
            .import_set_candidate_album_artists(&key, vec![ArtistAssignment::named("Artist Name")])
            .await
            .unwrap();
    }
    let expected = select_out_of_order(&services, descending(), &[3, 0, 2]).await;

    let mut changes = services.every_candidate_runtime_change_for_test();
    let failures = services
        .run_import_selection_action(descending(), CandidateAction::Import, |_, _| {})
        .await
        .unwrap();
    assert!(failures.is_empty(), "{failures:?}");

    let started = reached_in_order(&mut changes, expected.len(), |runtime| {
        runtime.import.is_some()
    })
    .await;
    assert_eq!(started, expected);
    services.close().await;
}
