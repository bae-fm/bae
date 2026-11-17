//! The import list's selection through the app: the surface keeps the
//! selected keys, a view change takes out the ones the new view doesn't show,
//! and a bulk action runs in core over the selected keys the list shows, in
//! the order it shows them.

use bae_app::AppServices;
use bae_import::selection::SingleSelection;
use bae_import::{
    Admission, ArtistAssignment, CandidateAction, CandidateKey, CandidateRuntimeChange,
    CandidateRuntimeSnapshot, FollowedImportList, ImportListItem, ImportListOrder, ImportListRead,
    ImportListView, PendingFilter, TriageTab,
};
use bae_store::ListWindow;
use bae_util::test_channels::HANG_GUARD;
use std::path::Path;
use tempfile::TempDir;

/// A library at `dir` with automatic identification off, so nothing but the
/// test acts on the candidates.
pub(crate) async fn open(dir: &Path) -> AppServices {
    let library = bae_library::test_libraries::open_test_library(dir).await;
    library.manager().set_identify_automatically(false).await.unwrap();
    bae_app::test_services::start_services(&library)
        .await
        .expect("the test library's services start")
}

/// `count` one-track albums under `collection`, watched and scanned. Each
/// track has its own name, so no two albums are the same files.
pub(crate) async fn scan_albums(services: &AppServices, collection: &Path, count: usize) {
    let flac = std::fs::read(bae_import::test_fixtures::flac_dir().join("01 Test Track 1.flac"))
        .expect("FLAC fixture");
    for n in 0..count {
        let album = collection.join(format!("Album {n}"));
        std::fs::create_dir_all(&album).unwrap();
        std::fs::write(album.join(format!("01 Track {n}.flac")), &flac).unwrap();
    }
    services.import_add_watched_folder(collection.to_path_buf()).await.unwrap();
    tab_counts_until(services, |counts| counts.pending as usize == count).await;
}

/// Waits on the import list's count until `accept` returns true for the tab
/// counts it holds.
async fn tab_counts_until(
    services: &AppServices,
    accept: impl Fn(&bae_import::TriageTabCounts) -> bool,
) {
    let list = services.subscribe_import_list(ImportListView::default()).await.unwrap();
    let counts = bae_app::import_list_count(&list);
    tokio::time::timeout(HANG_GUARD, async {
        loop {
            let count = counts.next().await.expect("the list's count answers");
            let bae_views::ListCountExtra::Import { tabs, .. } = count.extra else {
                panic!("an import list's count carries the tab counts: {count:?}");
            };
            if accept(&tabs) {
                return;
            }
        }
    })
    .await
    .expect("the tab counts get there");
}

/// The list's snapshots, each with the selection of `selected`, until one
/// `accept` admits.
async fn until(
    list: &FollowedImportList,
    selected: &[CandidateKey],
    accept: impl Fn(&ImportListRead) -> bool,
) -> ImportListRead {
    tokio::time::timeout(HANG_GUARD, async {
        loop {
            let snapshot = list.next(selected.to_vec()).await;
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
) -> (FollowedImportList, ImportListRead) {
    let list = services.subscribe_import_list(view).await.unwrap();
    let list = FollowedImportList::follow(
        list,
        ListWindow::Forward { from: None, until: None, limit: 50 },
    );
    let snapshot = until(&list, &[], |snapshot| {
        snapshot.total_count == count
            && snapshot.windows.iter().map(|window| window.rows.len() as u64).sum::<u64>() == count
    })
    .await;
    (list, snapshot)
}

/// The keys of `selected` the list shows, in its order.
async fn selected_keys(list: &FollowedImportList, selected: &[CandidateKey]) -> Vec<CandidateKey> {
    list.selected(selected.to_vec())
        .await
        .unwrap()
        .candidates
        .into_iter()
        .map(|candidate| candidate.candidate_key)
        .collect()
}

/// The candidate keys the snapshot's page shows, top to bottom.
fn on_page(snapshot: &ImportListRead) -> Vec<bae_import::CandidateKey> {
    snapshot
        .windows
        .iter()
        .flat_map(|window| &window.rows)
        .filter_map(|item| match item {
            ImportListItem::Candidate { row, .. } => Some(row.candidate_key.clone()),
            _ => None,
        })
        .collect()
}

/// Skipping the selection runs in core over every selected row; each skipped
/// row leaves the rows Found shows, so no later action applies to it, while
/// the selection still holds it.
#[tokio::test(flavor = "multi_thread")]
async fn a_bulk_action_runs_over_every_selected_row() {
    bae_util::test_logs::install();
    let tmp = TempDir::new().unwrap();
    let services = open(&tmp.path().join("db")).await;
    scan_albums(&services, &tmp.path().join("Collection"), 3).await;
    let (list, _) = list_showing(&services, ImportListView::default(), 3).await;

    let selected = list.keys().await.unwrap();
    assert_eq!(list.read(selected.clone()).await.selection.count, 3);

    let failures = services
        .run_import_selection_action(
            &list.selected(selected.clone()).await.unwrap(),
            CandidateAction::Skip,
            |_, _| {},
        )
        .await
        .unwrap();

    assert!(failures.is_empty(), "{failures:?}");
    let settled = until(&list, &selected, |snapshot| snapshot.total_count == 0).await;
    tab_counts_until(&services, |counts| counts.pending == 0 && counts.skipped == 3).await;
    assert_eq!(settled.selection.count, 3, "the selection keeps the skipped rows");
    assert!(selected_keys(&list, &selected).await.is_empty(), "the list shows none of them");
    services.close().await.unwrap();
}

/// Rows whose state moves them out of the filter the list shows stay
/// selected, though no action applies to them while the list hides them, and
/// a view that shows them again shows them selected.
#[tokio::test(flavor = "multi_thread")]
async fn rows_that_leave_the_filter_leave_the_selection() {
    bae_util::test_logs::install();
    let tmp = TempDir::new().unwrap();
    let services = open(&tmp.path().join("db")).await;
    scan_albums(&services, &tmp.path().join("Collection"), 3).await;
    let not_looked_up =
        ImportListView { pending_filter: PendingFilter::NotLookedUp, ..ImportListView::default() };
    let (list, shown) = list_showing(&services, not_looked_up, 3).await;
    let picked: Vec<CandidateKey> = on_page(&shown).into_iter().take(2).collect();
    let selected = list.read(picked.clone()).await;
    assert_eq!(selected.selection.count, 2);
    assert_eq!(selected_keys(&list, &picked).await, picked);
    assert!(selected
        .selection
        .offers
        .iter()
        .any(|offer| offer.action == CandidateAction::Identify));

    let failures = services
        .run_import_selection_action(
            &list.selected(picked.clone()).await.unwrap(),
            CandidateAction::Identify,
            |_, _| {},
        )
        .await
        .unwrap();
    assert!(failures.is_empty(), "{failures:?}");

    let left = until(&list, &picked, |snapshot| snapshot.total_count == 1).await;
    assert_eq!(left.selection.count, 2, "the hidden rows stay selected");
    assert!(selected_keys(&list, &picked).await.is_empty(), "no action applies to a hidden row");

    let change = list.set_view(ImportListView::default(), picked.clone()).await.unwrap().unwrap();
    assert!(change.left.is_empty(), "the new view shows both rows");
    let all = until(&list, &picked, |snapshot| on_page(snapshot).len() == 3).await;
    assert_eq!(all.selection.count, 2);
    let mut shown_selected = selected_keys(&list, &picked).await;
    shown_selected.sort();
    let mut picked = picked;
    picked.sort();
    assert_eq!(shown_selected, picked, "rows shown again are still selected");
    services.close().await.unwrap();
}

/// A filter that hides selected rows takes them out of the selection; the
/// rows it still shows stay selected, and clearing it brings the hidden rows
/// back unselected.
#[tokio::test(flavor = "multi_thread")]
async fn a_filter_change_keeps_only_the_selected_rows_it_shows() {
    bae_util::test_logs::install();
    let tmp = TempDir::new().unwrap();
    let services = open(&tmp.path().join("db")).await;
    scan_albums(&services, &tmp.path().join("Collection"), 3).await;
    let (list, _) = list_showing(&services, ImportListView::default(), 3).await;
    let mut selected = list.keys().await.unwrap();

    let change = list
        .set_view(
            ImportListView { filter_text: "Album 1".to_string(), ..ImportListView::default() },
            selected.clone(),
        )
        .await
        .unwrap()
        .unwrap();
    assert_eq!(change.left.len(), 2, "the filter hides two selected rows");
    selected.retain(|key| !change.left.contains(key));
    let narrowed = until(&list, &selected, |snapshot| snapshot.total_count == 1).await;
    assert_eq!(narrowed.selection.count, 1);
    let kept = selected_keys(&list, &selected).await;
    assert_eq!(kept, selected);

    let change = list.set_view(ImportListView::default(), selected.clone()).await.unwrap().unwrap();
    assert!(change.left.is_empty(), "clearing the filter hides nothing");
    let cleared = until(&list, &selected, |snapshot| on_page(snapshot).len() == 3).await;
    assert_eq!(cleared.selection.count, 1);
    assert_eq!(selected_keys(&list, &selected).await, kept);
    services.close().await.unwrap();
}

/// The list sorted by path, last to first, so the order it shows is the
/// reverse of the keys' own.
fn descending() -> ImportListView {
    ImportListView { order: ImportListOrder::PathDescending, ..ImportListView::default() }
}

/// The candidate keys `view` shows, top to bottom.
async fn shown_keys(services: &AppServices, view: ImportListView) -> Vec<bae_import::CandidateKey> {
    services
        .load_import_items(view)
        .await
        .unwrap()
        .into_iter()
        .filter_map(|item| match item {
            ImportListItem::Candidate { row, .. } => Some(row.candidate_key),
            _ => None,
        })
        .collect()
}

/// The keys of the shown rows `picks` names, by their place in the list and
/// out of order, as a surface keeps them selected, and the same keys in the
/// order the list shows them.
async fn select_out_of_order(
    services: &AppServices,
    view: ImportListView,
    picks: &[usize],
) -> (Vec<CandidateKey>, Vec<CandidateKey>) {
    let shown = shown_keys(services, view).await;
    let selected = picks.iter().map(|&pick| shown[pick].clone()).collect();
    let mut in_view_order = picks.to_vec();
    in_view_order.sort_unstable();
    (selected, in_view_order.into_iter().map(|pick| shown[pick].clone()).collect())
}

/// The keys the runtime's changes bring to `reached`, in the order they get
/// there, until `count` have.
async fn reached_in_order(
    changes: &mut bae_import::CandidateRuntimeChanges,
    count: usize,
    reached: impl Fn(&CandidateRuntimeSnapshot) -> bool,
) -> Vec<bae_import::CandidateKey> {
    let mut order: Vec<bae_import::CandidateKey> = Vec::new();
    tokio::time::timeout(HANG_GUARD, async {
        while order.len() < count {
            let arrivals: Vec<bae_import::CandidateKey> =
                match changes.recv().await.expect("the runtime answers") {
                    CandidateRuntimeChange::Updated { key, runtime } => (reached(&runtime)
                        && !order.contains(&key))
                    .then_some(key)
                    .into_iter()
                    .collect(),
                    CandidateRuntimeChange::Removed { .. } => Vec::new(),
                };
            assert!(arrivals.len() <= 1, "one change brings one candidate at a time: {arrivals:?}");
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
    bae_util::test_logs::install();
    let tmp = TempDir::new().unwrap();
    let services = open(&tmp.path().join("db")).await;
    scan_albums(&services, &tmp.path().join("Collection"), 4).await;
    let (list, _) = list_showing(&services, descending(), 4).await;
    let (selected, expected) = select_out_of_order(&services, descending(), &[3, 0, 2]).await;

    let mut changes = services.every_candidate_runtime_change_for_test();
    let failures = services
        .run_import_selection_action(
            &list.selected(selected).await.unwrap(),
            CandidateAction::Identify,
            |_, _| {},
        )
        .await
        .unwrap();
    assert!(failures.is_empty(), "{failures:?}");

    let admitted = reached_in_order(&mut changes, expected.len(), |runtime| {
        runtime.identification
            == Some(bae_import::IdentificationRuntime::Queued(Admission::Requested))
    })
    .await;
    assert_eq!(admitted, expected);
    services.close().await.unwrap();
}

/// A bulk Import over rows selected out of order starts them in the order the
/// list shows them.
#[tokio::test(flavor = "multi_thread")]
async fn a_bulk_import_starts_the_rows_as_the_list_shows_them() {
    bae_util::test_logs::install();
    let tmp = TempDir::new().unwrap();
    let services = open(&tmp.path().join("db")).await;
    scan_albums(&services, &tmp.path().join("Collection"), 4).await;
    for key in shown_keys(&services, descending()).await {
        services.import_select_candidate_file_tags(key.clone()).await.unwrap();
        services
            .import_set_candidate_album_artists(
                &key,
                vec![bae_import::Credited::plain(ArtistAssignment::named("Artist Name"))],
            )
            .await
            .unwrap();
    }
    let (list, _) = list_showing(&services, descending(), 4).await;
    let (selected, expected) = select_out_of_order(&services, descending(), &[3, 0, 2]).await;

    let mut changes = services.every_candidate_runtime_change_for_test();
    let failures = services
        .run_import_selection_action(
            &list.selected(selected).await.unwrap(),
            CandidateAction::Import,
            |_, _| {},
        )
        .await
        .unwrap();
    assert!(failures.is_empty(), "{failures:?}");

    let started =
        reached_in_order(&mut changes, expected.len(), |runtime| runtime.import.is_some()).await;
    assert_eq!(started, expected);
    services.close().await.unwrap();
}

/// A folder that failed validation is a row the person can select on the
/// Skipped tab to look into: alone it is the one selected row, and it offers
/// nothing to do.
#[tokio::test(flavor = "multi_thread")]
async fn a_skipped_folder_is_selected_like_a_row_and_offers_nothing() {
    bae_util::test_logs::install();
    let tmp = TempDir::new().unwrap();
    let services = open(&tmp.path().join("db")).await;
    let collection = tmp.path().join("Collection");
    let broken = collection.join("Half Copied");
    std::fs::create_dir_all(&broken).unwrap();
    std::fs::write(
        broken.join("01 Track.flac"),
        std::fs::read(bae_import::test_fixtures::flac_dir().join("01 Test Track 1.flac")).unwrap(),
    )
    .unwrap();
    std::fs::write(broken.join("02 Track.flac"), b"").unwrap();
    scan_albums(&services, &collection, 1).await;
    let skipped = ImportListView { tab: TriageTab::Skipped, ..ImportListView::default() };
    let (list, _) = list_showing(&services, skipped, 1).await;
    let key = bae_import::CandidateKey::folder(&broken);

    let selected = list.read(vec![key.clone()]).await;
    assert_eq!(selected.selection.count, 1);
    assert_eq!(
        selected.selection.single,
        Some(SingleSelection::InvalidCandidate { key: key.clone() })
    );
    assert!(selected.selection.offers.is_empty());
    assert_eq!(list.selected(vec![key.clone()]).await.unwrap().invalid, [key]);
    services.close().await.unwrap();
}
