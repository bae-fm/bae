#![cfg(feature = "test-utils")]
//! The import list's selection through the app: the list owns it and holds
//! only the rows it shows, and a bulk action runs in core over the selected
//! keys in the order the list shows them.

use bae_core::import::selection::{SelectionChange, SelectionSummary, SingleSelection};
use bae_core::import::{
    Admission, ArtistAssignment, CandidateAction, CandidateRuntimeChange, CandidateRuntimeSnapshot,
    ImportListItem, ImportListOrder, ImportListSnapshot, ImportListSubscription, ImportListView,
    PendingFilter, TriageTab,
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

/// The list's snapshots until one `accept` admits.
async fn until(
    list: &ImportListSubscription,
    accept: impl Fn(&ImportListSnapshot) -> bool,
) -> ImportListSnapshot {
    tokio::time::timeout(std::time::Duration::from_secs(20), async {
        loop {
            let snapshot = list.next().await.expect("the list answers");
            if accept(&snapshot) {
                return snapshot;
            }
        }
    })
    .await
    .expect("the list gets there")
}

/// A list over `view`, read through one window, and its snapshot once it
/// shows `count` rows.
async fn list_showing(
    services: &AppServices,
    view: ImportListView,
    count: u64,
) -> (ImportListSubscription, ImportListSnapshot) {
    let list = services.subscribe_import_list(view, &tokio::runtime::Handle::current());
    list.set_windows(
        std::iter::once(LibraryPageWindow {
            offset: 0,
            limit: 50,
        })
        .collect(),
    )
    .unwrap();
    let snapshot = until(&list, |snapshot| {
        snapshot.total_count == count
            && snapshot
                .windows
                .iter()
                .map(|window| window.items.len() as u64)
                .sum::<u64>()
                == count
    })
    .await;
    (list, snapshot)
}

/// The keys the snapshot's page shows selected, top to bottom.
fn selected_on_page(snapshot: &ImportListSnapshot) -> Vec<String> {
    snapshot
        .windows
        .iter()
        .flat_map(|window| &window.items)
        .filter_map(|item| match item {
            ImportListItem::Candidate { row, .. } if row.selected => {
                Some(row.candidate_key.clone())
            }
            _ => None,
        })
        .collect()
}

/// The candidate keys the snapshot's page shows, top to bottom.
fn on_page(snapshot: &ImportListSnapshot) -> Vec<String> {
    snapshot
        .windows
        .iter()
        .flat_map(|window| &window.items)
        .filter_map(|item| match item {
            ImportListItem::Candidate { row, .. } => Some(row.candidate_key.clone()),
            _ => None,
        })
        .collect()
}

/// Skipping the selection runs in core over every selected row, and each
/// skipped row leaves the selection as it leaves the rows Found shows.
#[tokio::test(flavor = "multi_thread")]
async fn a_bulk_action_runs_over_every_selected_row() {
    support::tracing_init();
    let tmp = TempDir::new().unwrap();
    let services = open(&tmp.path().join("db")).await;
    scan_albums(&services, &tmp.path().join("Collection"), 3).await;
    let (list, _) = list_showing(&services, ImportListView::default(), 3).await;

    let revision = list.select_all().unwrap();
    until(&list, |snapshot| {
        snapshot.selection_revision >= revision && snapshot.selection.count == 3
    })
    .await;

    let failures = services
        .run_import_selection_action(&list.selected(), CandidateAction::Skip, |_, _| {})
        .await
        .unwrap();

    assert!(failures.is_empty(), "{failures:?}");
    let settled = until(&list, |snapshot| snapshot.summary.counts.pending == 0).await;
    assert_eq!(settled.summary.counts.skipped, 3);
    assert_eq!(settled.selection, SelectionSummary::default());
    assert!(list.selected().candidates.is_empty());
    services.close().await.unwrap();
}

/// Rows whose state moves them out of the filter the list shows leave the
/// selection in the read that stops showing them, and what the selection
/// offers follows: with nothing it shows selected, it offers nothing. Shown
/// again, they come back unselected.
#[tokio::test(flavor = "multi_thread")]
async fn rows_that_leave_the_filter_leave_the_selection() {
    support::tracing_init();
    let tmp = TempDir::new().unwrap();
    let services = open(&tmp.path().join("db")).await;
    scan_albums(&services, &tmp.path().join("Collection"), 3).await;
    let not_looked_up = ImportListView {
        pending_filter: PendingFilter::NotLookedUp,
        ..ImportListView::default()
    };
    let (list, shown) = list_showing(&services, not_looked_up, 3).await;
    let picked: Vec<String> = on_page(&shown).into_iter().take(2).collect();
    let revision = list
        .change_selection(SelectionChange::Replace {
            keys: picked.clone(),
        })
        .unwrap();
    let selected = until(&list, |snapshot| {
        snapshot.selection_revision >= revision && snapshot.selection.count == 2
    })
    .await;
    assert_eq!(selected_on_page(&selected), picked);
    assert!(selected
        .selection
        .offers
        .iter()
        .any(|offer| offer.action == CandidateAction::Identify));

    let failures = services
        .run_import_selection_action(&list.selected(), CandidateAction::Identify, |_, _| {})
        .await
        .unwrap();
    assert!(failures.is_empty(), "{failures:?}");

    let left = until(&list, |snapshot| snapshot.total_count == 1).await;
    assert_eq!(left.selection, SelectionSummary::default());
    assert!(selected_on_page(&left).is_empty());
    assert!(list.selected().candidates.is_empty());

    list.set_view(ImportListView::default()).unwrap();
    let all = until(&list, |snapshot| on_page(snapshot).len() == 3).await;
    assert!(
        selected_on_page(&all).is_empty(),
        "rows shown again come back unselected"
    );
    assert_eq!(all.selection.count, 0);
    services.close().await.unwrap();
}

/// A filter that hides selected rows takes them out of the selection; the
/// rows it still shows stay selected, and clearing it brings the hidden rows
/// back unselected.
#[tokio::test(flavor = "multi_thread")]
async fn a_filter_change_keeps_only_the_selected_rows_it_shows() {
    support::tracing_init();
    let tmp = TempDir::new().unwrap();
    let services = open(&tmp.path().join("db")).await;
    scan_albums(&services, &tmp.path().join("Collection"), 3).await;
    let (list, _) = list_showing(&services, ImportListView::default(), 3).await;
    let revision = list.select_all().unwrap();
    until(&list, |snapshot| {
        snapshot.selection_revision >= revision && snapshot.selection.count == 3
    })
    .await;

    list.set_view(ImportListView {
        filter_text: "Album 1".to_string(),
        ..ImportListView::default()
    })
    .unwrap();
    let narrowed = until(&list, |snapshot| snapshot.total_count == 1).await;
    assert_eq!(narrowed.selection.count, 1);
    assert_eq!(selected_on_page(&narrowed).len(), 1);
    assert_eq!(list.selected().candidates.len(), 1);

    list.set_view(ImportListView::default()).unwrap();
    let cleared = until(&list, |snapshot| on_page(snapshot).len() == 3).await;
    assert_eq!(cleared.selection.count, 1);
    assert_eq!(selected_on_page(&cleared), selected_on_page(&narrowed));
    services.close().await.unwrap();
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
    list: &ImportListSubscription,
    view: ImportListView,
    picks: &[usize],
) -> Vec<String> {
    let shown = shown_keys(services, view).await;
    let revision = list
        .change_selection(SelectionChange::Toggle {
            add: picks.iter().map(|&pick| shown[pick].clone()).collect(),
            remove: Vec::new(),
        })
        .unwrap();
    until(list, |snapshot| {
        snapshot.selection_revision >= revision && snapshot.selection.count == picks.len() as u64
    })
    .await;
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
    let (list, _) = list_showing(&services, descending(), 4).await;
    let expected = select_out_of_order(&services, &list, descending(), &[3, 0, 2]).await;

    let mut changes = services.every_candidate_runtime_change_for_test();
    let failures = services
        .run_import_selection_action(&list.selected(), CandidateAction::Identify, |_, _| {})
        .await
        .unwrap();
    assert!(failures.is_empty(), "{failures:?}");

    let admitted = reached_in_order(&mut changes, expected.len(), |runtime| {
        runtime.queued == Some(Admission::Requested)
    })
    .await;
    assert_eq!(admitted, expected);
    services.close().await.unwrap();
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
            .import_set_candidate_album_artists(
                &key,
                vec![bae_core::import::Credited::plain(ArtistAssignment::named(
                    "Artist Name",
                ))],
            )
            .await
            .unwrap();
    }
    let (list, _) = list_showing(&services, descending(), 4).await;
    let expected = select_out_of_order(&services, &list, descending(), &[3, 0, 2]).await;

    let mut changes = services.every_candidate_runtime_change_for_test();
    let failures = services
        .run_import_selection_action(&list.selected(), CandidateAction::Import, |_, _| {})
        .await
        .unwrap();
    assert!(failures.is_empty(), "{failures:?}");

    let started = reached_in_order(&mut changes, expected.len(), |runtime| {
        runtime.import.is_some()
    })
    .await;
    assert_eq!(started, expected);
    services.close().await.unwrap();
}

/// A folder that failed validation is a row the person can select on the
/// Skipped tab to look into: alone it is the one selected row, and it offers
/// nothing to do.
#[tokio::test(flavor = "multi_thread")]
async fn a_skipped_folder_is_selected_like_a_row_and_offers_nothing() {
    support::tracing_init();
    let tmp = TempDir::new().unwrap();
    let services = open(&tmp.path().join("db")).await;
    let collection = tmp.path().join("Collection");
    let broken = collection.join("Half Copied");
    std::fs::create_dir_all(&broken).unwrap();
    std::fs::write(
        broken.join("01 Track.flac"),
        std::fs::read(bae_test_support::fixture_dir!(
            "flac",
            "01 Test Track 1.flac"
        ))
        .unwrap(),
    )
    .unwrap();
    std::fs::write(broken.join("02 Track.flac"), b"").unwrap();
    scan_albums(&services, &collection, 1).await;
    let skipped = ImportListView {
        tab: TriageTab::Skipped,
        ..ImportListView::default()
    };
    let (list, _) = list_showing(&services, skipped, 1).await;
    let key = broken.to_string_lossy().into_owned();

    let revision = list
        .change_selection(SelectionChange::Replace {
            keys: vec![key.clone()],
        })
        .unwrap();
    let selected = until(&list, |snapshot| {
        snapshot.selection_revision >= revision && snapshot.selection.count == 1
    })
    .await;
    assert_eq!(
        selected.selection.single,
        Some(SingleSelection::InvalidCandidate { key: key.clone() })
    );
    assert!(selected.selection.offers.is_empty());
    assert_eq!(list.selected().invalid, [key]);
    services.close().await.unwrap();
}
