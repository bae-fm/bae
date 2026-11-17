//! Tests of the import list's selection. The surface (the app's import list
//! screen) keeps the selection as keys. These tests check that the summary
//! reads the rows at the keys whether or not the list shows them, that the
//! selected rows a bulk action runs over are the ones the list shows, in its
//! order, and that each row offers what its tables and what is running for
//! it allow. They also check the keys that Shift-clicking a row and Select
//! All select.

use super::*;
use crate::selection::SingleSelection;
use crate::tables::import_list::tests::*;
use crate::tables::tests::*;
use crate::triage::SelectionOffer;
use crate::CandidateAction;
use crate::ImportListItem;
use crate::ImportListOrder;
use crate::TriageTab;

/// The keys a bulk action of `action` over the rows at `selected` runs over in
/// `list`, in the list's order.
async fn keys_running(
    lib: &Library,
    list: ImportListId,
    selected: &[crate::CandidateKey],
    action: CandidateAction,
) -> Vec<crate::CandidateKey> {
    let selected = lib
        .tables()
        .load_import_list_selected(list, selected.to_vec(), Some(action))
        .await
        .unwrap();
    selected.candidates.into_iter().map(|candidate| candidate.candidate_key).collect()
}

/// The keys of every row `list` shows: what Select All selects.
async fn all_keys(lib: &Library, list: ImportListId) -> Vec<crate::CandidateKey> {
    lib.tables().import_list_keys(list).await.unwrap()
}

/// The offer for `action` of the rows at `selected`.
async fn offer(
    lib: &Library,
    list: ImportListId,
    selected: &[crate::CandidateKey],
    action: CandidateAction,
) -> Option<SelectionOffer> {
    lib.read_selecting(list, selected.to_vec())
        .await
        .selection
        .offers
        .into_iter()
        .find(|offer| offer.action == action)
}

/// The summary reads the rows at the keys asked for, whether or not the list
/// shows them: the summary still counts a row the list hides after a change
/// to it, and a key whose row the tables no longer hold names nothing.
#[tokio::test]
async fn the_summary_reads_the_rows_at_its_keys_whatever_the_list_shows() {
    let lib = five_rows().await;
    let list = lib.open(under(TriageTab::Pending, PendingFilter::NotLookedUp)).await;
    let selected = vec![lib.key("A"), lib.key("B")];
    let read = lib.read_selecting(list, selected.clone()).await;
    assert_eq!(read.selection.count, 2);
    assert_eq!(read.selection.single, None);
    lib.run(lib.linked("A")).await;
    let read = lib.read_selecting(list, selected.clone()).await;
    assert!(!sequence(&read).contains(&"candidate A".to_string()), "the filter hides A");
    assert_eq!(read.selection.count, 2, "A is still selected while the list hides it");
    let read = lib.read_selecting(list, vec![lib.key("B"), lib.key("Nowhere")]).await;
    assert_eq!(read.selection.count, 1);
    assert_eq!(read.selection.single, Some(SingleSelection::Candidate { key: lib.key("B") }));
}

/// A selected release whose import finishes is still in the summary, as the
/// one selected row, so its pane stays open showing it as imported, while the
/// Pending list no longer shows its row.
#[tokio::test]
async fn a_selected_release_whose_import_lands_stays_in_the_summary() {
    let lib = five_rows().await;
    let list = lib.open(view(TriageTab::Pending)).await;
    imported_release(&lib.db, &lib.candidate("A")).await;
    let read = lib.read_selecting(list, vec![lib.key("A")]).await;
    assert_eq!(read.selection.count, 1);
    assert_eq!(read.selection.single, Some(SingleSelection::Candidate { key: lib.key("A") }));
    assert!(
        !sequence(&read).contains(&"candidate A".to_string()),
        "the list no longer shows the row"
    );
    let pane = lib.tables().load_import_candidate(&lib.key("A")).await.unwrap().unwrap();
    assert!(pane.draft().is_none(), "the pane holds the imported release");
}

/// A bulk action runs over the rows at its keys the list shows, and skips one
/// the list hides: here a release whose import finished, and one the filter
/// hides.
#[tokio::test]
async fn a_bulk_action_runs_over_the_keys_the_list_shows() {
    let lib = five_rows().await;
    let list = lib.open(under(TriageTab::Pending, PendingFilter::NotLookedUp)).await;
    imported_release(&lib.db, &lib.candidate("A")).await;
    lib.run(lib.linked("B")).await;
    let selected = [lib.key("A"), lib.key("B"), lib.key("C"), lib.key("D")];
    let rows = lib.tables().load_import_list_selected(list, selected.to_vec(), None).await.unwrap();
    let mut keys: Vec<_> =
        rows.candidates.into_iter().map(|candidate| candidate.candidate_key).collect();
    keys.sort();
    assert_eq!(keys, [lib.key("C"), lib.key("D")]);
}

/// Changing the tab, filter or search answers with the selected rows the new
/// view doesn't show, which leave the selection, a release whose import
/// finished among them; another order answers none.
#[tokio::test]
async fn a_view_change_answers_the_selected_rows_that_leave() {
    let lib = five_rows().await;
    lib.run(lib.linked("D")).await;
    let list = lib.open(ordered(TriageTab::Pending, ImportListOrder::PathAscending)).await;
    imported_release(&lib.db, &lib.candidate("A")).await;
    let selected = vec![lib.key("A"), lib.key("B"), lib.key("D")];
    let left = |view| {
        let (tables, selected) = (lib.tables(), selected.clone());
        async move {
            let mut left =
                tables.set_import_list_view(list, view, selected).await.unwrap().unwrap().left;
            left.sort();
            left
        }
    };
    assert!(
        left(ordered(TriageTab::Pending, ImportListOrder::PathDescending)).await.is_empty(),
        "another order keeps every selected row, the imported one too"
    );
    let mut hidden = vec![lib.key("A"), lib.key("B")];
    hidden.sort();
    assert_eq!(
        left(ImportListView {
            pending_filter: PendingFilter::Identified,
            ..ordered(TriageTab::Pending, ImportListOrder::PathDescending)
        })
        .await,
        hidden,
        "Identified shows D alone"
    );
    let mut hidden = vec![lib.key("B"), lib.key("D")];
    hidden.sort();
    assert_eq!(left(view(TriageTab::Done)).await, hidden, "Done shows A alone");
}

/// Folding a group answers with the selected rows it hid; opening it hides
/// none, and a selected row the list hid by a change to it is no row the fold
/// hid.
#[tokio::test]
async fn folding_a_group_answers_the_selected_rows_it_hid() {
    let lib = Library::new().await;
    let tables = lib.tables();
    lib.run(lib.scan("Box/1") + lib.scan("Box/2") + lib.scan("Loose")).await;
    let list = lib.open(under(TriageTab::Pending, PendingFilter::NotLookedUp)).await;
    lib.run(lib.linked("Box/2")).await;
    let selected = vec![lib.key("Box/1"), lib.key("Box/2"), lib.key("Loose")];
    assert_eq!(
        tables
            .set_import_list_group_folded(list, lib.group_key("Box"), true, selected.clone())
            .await
            .unwrap(),
        [lib.key("Box/1")]
    );
    assert!(tables
        .set_import_list_group_folded(list, lib.group_key("Box"), false, selected.clone())
        .await
        .unwrap()
        .is_empty());
    assert_eq!(
        tables.set_import_list_groups_folded(list, true, selected).await.unwrap(),
        [lib.key("Box/1")]
    );
}

/// The selected rows a bulk action takes are in the order the list shows
/// them, whatever order they were selected in.
#[tokio::test]
async fn the_selected_rows_come_in_the_list_s_order() {
    let lib = five_rows().await;
    let tables = crate::tables::ImportTables::new(lib.db.clone());
    let list = lib.open(ordered(TriageTab::Pending, ImportListOrder::PathDescending)).await;
    let selected = tables
        .load_import_list_selected(list, vec![lib.key("B"), lib.key("D"), lib.key("A")], None)
        .await
        .unwrap();
    assert_eq!(
        selected
            .candidates
            .iter()
            .map(|candidate| candidate.candidate_key.clone())
            .collect::<Vec<_>>(),
        [lib.key("D"), lib.key("B"), lib.key("A")]
    );
    assert_eq!(selected.candidates[0].name, "D");
}

/// A folder that failed validation is selected like a row: it is the one
/// selected row, and it offers nothing.
#[tokio::test]
async fn a_folder_that_failed_validation_is_selected_and_offers_nothing() {
    let lib = Library::new().await;
    let tables = crate::tables::ImportTables::new(lib.db.clone());
    lib.run(lib.scan_as("Broken", "invalid")).await;
    let list = lib.open(view(TriageTab::Skipped)).await;
    let selected = all_keys(&lib, list).await;
    let read = lib.read_selecting(list, selected.clone()).await;
    assert_eq!(read.selection.count, 1);
    assert_eq!(
        read.selection.single,
        Some(SingleSelection::InvalidCandidate { key: lib.key("Broken") })
    );
    assert!(read.selection.offers.is_empty());
    assert!(matches!(&read.windows[0].rows[0], ImportListItem::Invalid { .. }));
    assert_eq!(
        tables.load_import_list_selected(list, selected, None).await.unwrap().invalid,
        [lib.key("Broken")]
    );
}

/// A selection of identified rows offers no Identify; a mixed one counts only
/// the rows no lookup is stored for.
#[tokio::test]
async fn identify_is_offered_for_the_rows_no_lookup_is_stored_for() {
    let lib = five_rows().await;
    lib.run(
        lib.verdict("A", "not_found", None)
            + lib.verdict("B", "failed", Some(11))
            + lib.verdict("C", "not_found", None),
    )
    .await;
    let list = lib.open(view(TriageTab::Pending)).await;
    let identified = [lib.key("A"), lib.key("C")];
    assert_eq!(offer(&lib, list, &identified, CandidateAction::Identify).await, None);
    let mixed = [lib.key("A"), lib.key("B"), lib.key("D"), lib.key("E")];
    assert_eq!(
        offer(&lib, list, &mixed, CandidateAction::Identify).await.map(|offer| offer.count),
        Some(2)
    );
    assert_eq!(
        offer(&lib, list, &mixed, CandidateAction::RetryIdentification)
            .await
            .map(|offer| offer.count),
        Some(1)
    );
}

/// Selecting every identified row and importing them takes each one — a pick
/// among several releases and a track count that differs as much as one
/// lookup's pick — and a tags-only row selected beside them imports too.
#[tokio::test]
async fn importing_a_selection_takes_every_row_with_a_draft_to_import() {
    let lib = Library::new().await;
    let identified: Write = ["Identified", "Picked Among Several"]
        .into_iter()
        .map(|display_path| lib.linked(display_path) + lib.valid_draft(display_path))
        .sum();
    lib.run(
        scanned_with_states(&lib, &["Identified", "Picked Among Several", "Several"])
            + lib.scan_tagged("Tagged", "Tagged", "Artist", Some("Track title"))
            + identified
            + lib.verdict("Several", "not_found", None),
    )
    .await;

    let identified = lib.open(under(TriageTab::Pending, PendingFilter::Identified)).await;
    let all = all_keys(&lib, identified).await;
    let mut imported = keys_running(&lib, identified, &all, CandidateAction::Import).await;
    imported.sort();
    assert_eq!(
        imported,
        [lib.key("Identified"), lib.key("Picked Among Several")],
        "selecting everything Identified shows takes every shown row and no hidden one"
    );

    let everything = lib.open(view(TriageTab::Pending)).await;
    let picked = [
        lib.key("Identified"),
        lib.key("Picked Among Several"),
        lib.key("Several"),
        lib.key("Tagged"),
    ];
    let mut imported = keys_running(&lib, everything, &picked, CandidateAction::Import).await;
    imported.sort();
    assert_eq!(
        imported,
        [lib.key("Identified"), lib.key("Picked Among Several"), lib.key("Tagged")]
    );
    assert_eq!(
        offer(&lib, everything, &picked, CandidateAction::Import).await.map(|offer| offer.count),
        Some(3)
    );
}

/// Rows being identified offer no import: selecting everything In Progress
/// shows while runs go offers to cancel them instead.
#[tokio::test]
async fn selecting_all_rows_in_progress_offers_their_cancel_and_no_import() {
    let lib = Library::new().await;
    lib.run(
        scanned_with_states(&lib, &["Identified", "Tagged"])
            + lib.linked("Identified")
            + lib.valid_draft("Identified")
            + lib.identifying("Identified", "running")
            + lib.identifying("Tagged", "running"),
    )
    .await;
    let list = lib.open(under(TriageTab::Pending, PendingFilter::InProgress)).await;
    let all = all_keys(&lib, list).await;
    let read = lib.read_selecting(list, all.clone()).await;
    assert_eq!(read.selection.count, 2, "the entry shows the rows being identified");
    assert!(!read.selection.offers.iter().any(|offer| offer.action == CandidateAction::Import));
    assert_eq!(
        offer(&lib, list, &all, CandidateAction::CancelIdentification)
            .await
            .map(|offer| offer.count),
        Some(2)
    );
}

/// What Needs You's rows offer at rest is each row's own: a draft imports, a
/// failed import imports again, and a lookup that failed or that bae broke on
/// is retried.
#[tokio::test]
async fn needs_you_rows_offer_what_their_drafts_and_lookups_allow() {
    let lib = Library::new().await;
    let releases = ["Broke", "Failed Import", "Lookup Failed", "Nothing Found", "Several Tagged"];
    lib.run(
        scanned_with_states(&lib, &releases)
            + lib.verdict("Broke", "error", None)
            + lib.verdict("Lookup Failed", "failed", Some(11))
            + lib.verdict("Nothing Found", "not_found", None)
            + lib.verdict("Several Tagged", "not_found", None)
            + lib.valid_draft("Several Tagged")
            + lib.valid_draft("Failed Import")
            + lib.import_failed("Failed Import"),
    )
    .await;
    let list = lib.open(under(TriageTab::Pending, PendingFilter::NeedsYou)).await;
    let all = all_keys(&lib, list).await;
    let sorted = |mut keys: Vec<crate::CandidateKey>| {
        keys.sort();
        keys
    };
    assert_eq!(
        sorted(keys_running(&lib, list, &all, CandidateAction::Import).await),
        [lib.key("Failed Import"), lib.key("Several Tagged")],
        "a draft imports and a failed import imports again; no draft, no import"
    );
    assert_eq!(
        sorted(keys_running(&lib, list, &all, CandidateAction::RetryIdentification).await),
        [lib.key("Broke"), lib.key("Lookup Failed")]
    );
}

/// A release the scan writes again over its stored row — its files changed —
/// keeps its place in the list: its key still names it, so it stays
/// selected, and the group it is folded under stays folded.
#[tokio::test]
async fn a_release_written_again_keeps_its_selection_and_its_group_s_fold() {
    let lib = Library::new().await;
    let tables = crate::tables::ImportTables::new(lib.db.clone());
    let release = |name: &str| candidate(&lib.root, name);
    for name in ["Box/1", "Box/2", "Loose"] {
        tables
            .save_folder_scan_item(
                &lib.root,
                1,
                &crate::folder_scanner::ScanItem::Valid(release(name)),
            )
            .await
            .unwrap();
    }
    let list = lib.open(ordered(TriageTab::Pending, ImportListOrder::PathAscending)).await;
    let group = crate::FolderReleaseDecisionKey {
        watched_folder_path: lib.root.clone(),
        relative_folder: crate::RelativeFolder::test("Box"),
    };
    tables.set_import_list_group_folded(list, group, true, Vec::new()).await.unwrap();
    for name in ["Box/2", "Loose"] {
        let mut changed = release(name);
        changed.files.files[0].file = crate::folder_scanner::ScannedFile::new(
            folder_below(&lib.root, name).join("01.flac"),
            "01.flac".to_string(),
            2_000,
            2,
        )
        .with_test_flac_audio();
        tables
            .save_folder_scan_item(&lib.root, 1, &crate::folder_scanner::ScanItem::Valid(changed))
            .await
            .unwrap();
    }
    let read = lib.read_selecting(list, vec![lib.key("Loose")]).await;
    assert_eq!(sequence(&read), ["group Box", "candidate Loose"]);
    assert_eq!(read.selection.single, Some(SingleSelection::Candidate { key: lib.key("Loose") }));
}

/// A selected release the scan no longer holds names no row, and the
/// summary counts it again when the scan finds it again.
#[tokio::test]
async fn a_release_gone_from_the_scan_names_no_row_until_it_is_back() {
    let lib = five_rows().await;
    let list = lib.open(view(TriageTab::Pending)).await;
    let selected = vec![lib.key("A")];
    lib.run(lib.unscan("A")).await;
    assert_eq!(lib.read_selecting(list, selected.clone()).await.selection.count, 0);
    lib.run(
        lib.begin_pass()
            + scanned_with_states(&lib, &["A", "B", "C", "D", "E"])
            + lib.finish_pass(),
    )
    .await;
    let read = lib.read_selecting(list, selected).await;
    assert_eq!(read.total_count, 5);
    assert_eq!(read.selection.count, 1);
}

/// Select All takes every release the list shows and no header.
#[tokio::test]
async fn select_all_takes_every_release_the_list_shows() {
    let lib = Library::new().await;
    lib.run(lib.scan("A") + lib.scan("Box/1") + lib.scan("Box/2") + lib.scan("C")).await;
    let list = lib.open(view(TriageTab::Pending)).await;
    let mut all = all_keys(&lib, list).await;
    all.sort();
    let mut expected = vec![lib.key("A"), lib.key("Box/1"), lib.key("Box/2"), lib.key("C")];
    expected.sort();
    assert_eq!(all, expected);
}

/// `import_list_keys_between` reads the rows between two rows in the list's
/// order, both ends included, whichever end comes first, and leaves out the
/// headers between them. When the list doesn't show one end, the method
/// returns the other end alone; when it shows neither, the method returns no
/// keys. On a closed list the method fails.
#[tokio::test]
async fn the_keys_between_two_rows_are_the_run_between_them() {
    let lib = Library::new().await;
    let tables = crate::tables::ImportTables::new(lib.db.clone());
    lib.run(lib.scan("A") + lib.scan("Box/1") + lib.scan("Box/2") + lib.scan("C")).await;
    let list = lib.open(ordered(TriageTab::Pending, ImportListOrder::PathAscending)).await;
    let between = |from: &str, to: &str| {
        let (tables, from, to) = (tables.clone(), lib.key(from), lib.key(to));
        async move {
            tables
                .import_list_keys_between(
                    list,
                    TriageTab::Pending,
                    ImportListOrder::PathAscending,
                    from,
                    to,
                )
                .await
                .unwrap()
        }
    };
    let run = [lib.key("Box/1"), lib.key("Box/2"), lib.key("C")];
    assert_eq!(between("C", "Box/1").await, run, "the run in the list's order");
    assert_eq!(between("Box/1", "C").await, run);
    assert_eq!(between("A", "Nowhere").await, [lib.key("A")]);
    assert!(between("Nowhere", "Nor here").await.is_empty());
    let on_done = tables
        .import_list_keys_between(
            list,
            TriageTab::Done,
            ImportListOrder::PathAscending,
            lib.key("A"),
            lib.key("C"),
        )
        .await
        .unwrap();
    assert!(on_done.is_empty(), "a tab the list doesn't show reads nothing");

    tables.close_import_list(list).await.unwrap();
    let closed = tables
        .import_list_keys_between(
            list,
            TriageTab::Pending,
            ImportListOrder::PathAscending,
            lib.key("A"),
            lib.key("C"),
        )
        .await;
    assert!(
        matches!(closed, Err(crate::tables::ImportListError::Closed(id)) if id == list),
        "{closed:?}"
    );
}
