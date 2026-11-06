//! A list's selection and the writes that change it: it holds only rows the
//! list shows, in its order, each offering what its tables and what is
//! running for it allow.

use super::*;
use crate::selection::SelectionChange;
use crate::selection::SingleSelection;
use crate::tables::import_list::tests::*;
use crate::tables::tests::*;
use crate::triage::{keys_for, SelectionOffer};
use crate::CandidateAction;
use crate::ImportListItem;
use crate::ImportListOrder;
use crate::TriageTab;

/// The keys `list`'s selection runs `action` over, in the list's order, with
/// what is running for each.
async fn keys_running(
    lib: &Library,
    list: ImportListId,
    action: CandidateAction,
) -> Vec<crate::CandidateKey> {
    let tables = crate::tables::ImportTables::new(lib.db.clone());
    let selected = tables.load_import_list_selected(list).await.unwrap();
    keys_for(&crate::selection::members(&selected.candidates), action)
}

/// The offer for `action` of `list`'s selection.
async fn offer(
    lib: &Library,
    list: ImportListId,
    action: CandidateAction,
) -> Option<SelectionOffer> {
    lib.read(list).await.selection.offers.into_iter().find(|offer| offer.action == action)
}

/// A row whose own change moves it out of what the list shows leaves the
/// selection in the write that moves it; one the list still shows stays.
#[tokio::test]
async fn a_row_moved_out_of_the_view_leaves_the_selection_in_its_write() {
    let lib = five_rows().await;
    let list = lib.open(under(TriageTab::Pending, PendingFilter::NotLookedUp)).await;
    lib.select(list, SelectionChange::Replace { keys: vec![lib.key("A"), lib.key("B")] }).await;
    lib.run(lib.linked("A")).await;
    let read = lib.read(list).await;
    assert_eq!(read.selection.count, 1);
    assert_eq!(read.selection.single, Some(SingleSelection::Candidate { key: lib.key("B") }));
    lib.run(lib.draft("B", "Album")).await;
    assert_eq!(
        lib.read(list).await.selection.count,
        1,
        "a change that keeps the row shown keeps it selected"
    );
}

/// The selected rows a bulk action takes are in the order the list shows
/// them, whatever order they were selected in.
#[tokio::test]
async fn the_selected_rows_come_in_the_list_s_order() {
    let lib = five_rows().await;
    let tables = crate::tables::ImportTables::new(lib.db.clone());
    let list = lib.open(ordered(TriageTab::Pending, ImportListOrder::PathDescending)).await;
    lib.select(
        list,
        SelectionChange::Toggle {
            add: vec![lib.key("B"), lib.key("D"), lib.key("A")],
            remove: Vec::new(),
        },
    )
    .await;
    let selected = tables.load_import_list_selected(list).await.unwrap();
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
    tables.select_all_in_import_list(list).await.unwrap();
    let read = lib.read(list).await;
    assert_eq!(read.selection.count, 1);
    assert_eq!(
        read.selection.single,
        Some(SingleSelection::InvalidCandidate { key: lib.key("Broken") })
    );
    assert!(read.selection.offers.is_empty());
    assert!(matches!(&read.windows[0].items[0], ImportListItem::Invalid { selected: true, .. }));
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
    lib.select(list, SelectionChange::Replace { keys: vec![lib.key("A"), lib.key("C")] }).await;
    assert_eq!(offer(&lib, list, CandidateAction::Identify).await, None);
    lib.select(
        list,
        SelectionChange::Replace {
            keys: vec![lib.key("A"), lib.key("B"), lib.key("D"), lib.key("E")],
        },
    )
    .await;
    assert_eq!(
        offer(&lib, list, CandidateAction::Identify).await.map(|offer| offer.count),
        Some(2)
    );
    assert_eq!(
        offer(&lib, list, CandidateAction::RetryIdentification).await.map(|offer| offer.count),
        Some(1)
    );
}

/// Selecting every identified row and importing them takes each one — a pick
/// among several releases and a track count that differs as much as one
/// lookup's pick — and a tags-only row selected beside them imports too.
#[tokio::test]
async fn importing_a_selection_takes_every_row_with_a_draft_to_import() {
    let lib = Library::new().await;
    let tables = crate::tables::ImportTables::new(lib.db.clone());
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
    tables.select_all_in_import_list(identified).await.unwrap();
    let mut imported = keys_running(&lib, identified, CandidateAction::Import).await;
    imported.sort();
    assert_eq!(
        imported,
        [lib.key("Identified"), lib.key("Picked Among Several")],
        "selecting everything Identified shows takes every shown row and no hidden one"
    );

    let everything = lib.open(view(TriageTab::Pending)).await;
    lib.select(
        everything,
        SelectionChange::Replace {
            keys: vec![
                lib.key("Identified"),
                lib.key("Picked Among Several"),
                lib.key("Several"),
                lib.key("Tagged"),
            ],
        },
    )
    .await;
    let mut imported = keys_running(&lib, everything, CandidateAction::Import).await;
    imported.sort();
    assert_eq!(
        imported,
        [lib.key("Identified"), lib.key("Picked Among Several"), lib.key("Tagged")]
    );
    assert_eq!(
        offer(&lib, everything, CandidateAction::Import).await.map(|offer| offer.count),
        Some(3)
    );
}

/// Rows being identified offer no import: selecting everything In Progress
/// shows while runs go offers to cancel them instead.
#[tokio::test]
async fn selecting_all_rows_in_progress_offers_their_cancel_and_no_import() {
    let lib = Library::new().await;
    let tables = crate::tables::ImportTables::new(lib.db.clone());
    lib.run(
        scanned_with_states(&lib, &["Identified", "Tagged"])
            + lib.linked("Identified")
            + lib.valid_draft("Identified")
            + lib.identifying("Identified", "running")
            + lib.identifying("Tagged", "running"),
    )
    .await;
    let list = lib.open(under(TriageTab::Pending, PendingFilter::InProgress)).await;
    tables.select_all_in_import_list(list).await.unwrap();
    let read = lib.read(list).await;
    assert_eq!(read.selection.count, 2, "the entry shows the rows being identified");
    assert!(!read.selection.offers.iter().any(|offer| offer.action == CandidateAction::Import));
    assert_eq!(
        offer(&lib, list, CandidateAction::CancelIdentification).await.map(|offer| offer.count),
        Some(2)
    );
}

/// What Needs You's rows offer at rest is each row's own: a draft imports, a
/// failed import imports again, and a lookup that failed or that bae broke on
/// is retried.
#[tokio::test]
async fn needs_you_rows_offer_what_their_drafts_and_lookups_allow() {
    let lib = Library::new().await;
    let tables = crate::tables::ImportTables::new(lib.db.clone());
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
    tables.select_all_in_import_list(list).await.unwrap();
    let sorted = |mut keys: Vec<crate::CandidateKey>| {
        keys.sort();
        keys
    };
    assert_eq!(
        sorted(keys_running(&lib, list, CandidateAction::Import).await),
        [lib.key("Failed Import"), lib.key("Several Tagged")],
        "a draft imports and a failed import imports again; no draft, no import"
    );
    assert_eq!(
        sorted(keys_running(&lib, list, CandidateAction::RetryIdentification).await),
        [lib.key("Broke"), lib.key("Lookup Failed")]
    );
}

/// A release the scan writes again over its stored row — its files changed —
/// keeps its place in the list: it stays selected, and the group it is
/// folded under stays folded.
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
    lib.select(list, SelectionChange::Replace { keys: vec![lib.key("Loose")] }).await;
    let group = crate::FolderReleaseDecisionKey {
        watched_folder_path: lib.root.clone(),
        relative_folder: crate::RelativeFolder::test("Box"),
    };
    tables.set_import_list_group_folded(list, group, true).await.unwrap();
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
    let read = lib.read(list).await;
    assert_eq!(sequence(&read), ["group Box", "candidate Loose"]);
    assert_eq!(read.selection.single, Some(SingleSelection::Candidate { key: lib.key("Loose") }));
}

/// A selected release the scan no longer holds leaves the selection with it,
/// and comes back unselected when the scan finds it again.
#[tokio::test]
async fn a_release_gone_from_the_scan_leaves_the_selection() {
    let lib = five_rows().await;
    let list = lib.open(view(TriageTab::Pending)).await;
    lib.select(list, SelectionChange::Replace { keys: vec![lib.key("A")] }).await;
    lib.run(lib.unscan("A")).await;
    assert_eq!(lib.read(list).await.selection.count, 0);
    lib.run(
        lib.begin_pass()
            + scanned_with_states(&lib, &["A", "B", "C", "D", "E"])
            + lib.finish_pass(),
    )
    .await;
    let read = lib.read(list).await;
    assert_eq!(read.total_count, 5);
    assert_eq!(read.selection.count, 0, "it comes back unselected");
}

/// A selection only ever holds rows the list shows: a key it does not show is
/// never selected, and the rows a view change hides leave it in that write.
#[tokio::test]
async fn a_selection_holds_only_rows_the_list_shows() {
    let lib = five_rows().await;
    let tables = crate::tables::ImportTables::new(lib.db.clone());
    lib.run(lib.skip("E") + lib.linked("D")).await;
    let list = lib.open(ordered(TriageTab::Pending, ImportListOrder::PathAscending)).await;
    let opened = lib.read(list).await.selection_revision;
    let revision = lib
        .select(
            list,
            SelectionChange::Replace { keys: vec![lib.key("A"), lib.key("D"), lib.key("E")] },
        )
        .await;
    let read = lib.read(list).await;
    assert_eq!(read.selection.count, 2, "E is on Skipped, which the list does not show");
    assert_eq!(read.selection_revision, revision);
    assert!(revision > opened, "the change moves the selection's revision");

    let narrowed = tables
        .set_import_list_view(
            list,
            ImportListView {
                pending_filter: PendingFilter::Identified,
                ..ordered(TriageTab::Pending, ImportListOrder::PathAscending)
            },
        )
        .await
        .unwrap();
    let read = lib.read(list).await;
    assert_eq!(read.view_revision, narrowed);
    assert_eq!(read.selection.count, 1, "A left the selection as the view hid it");
    assert_eq!(read.selection.single, Some(SingleSelection::Candidate { key: lib.key("D") }));
    assert!(read.selection_revision > revision);

    tables
        .set_import_list_view(list, ordered(TriageTab::Pending, ImportListOrder::PathAscending))
        .await
        .unwrap();
    assert_eq!(
        lib.read(list).await.selection.count,
        1,
        "a row the view shows again comes back unselected"
    );
}

/// Folding a group takes its selected rows out of the selection.
#[tokio::test]
async fn folding_a_group_takes_its_rows_out_of_the_selection() {
    let lib = Library::new().await;
    let tables = crate::tables::ImportTables::new(lib.db.clone());
    lib.run(lib.scan("Box/1") + lib.scan("Box/2")).await;
    let list = lib.open(view(TriageTab::Pending)).await;
    tables.select_all_in_import_list(list).await.unwrap();
    assert_eq!(lib.read(list).await.selection.count, 2);
    tables.set_import_list_groups_folded(list, true).await.unwrap();
    assert_eq!(lib.read(list).await.selection.count, 0);
}

/// A run of rows is selected along the list's order, both ends included,
/// headers between them no row; ends the list does not show select nothing.
#[tokio::test]
async fn extending_selects_the_run_between_two_rows() {
    let lib = Library::new().await;
    let tables = crate::tables::ImportTables::new(lib.db.clone());
    lib.run(lib.scan("A") + lib.scan("Box/1") + lib.scan("Box/2") + lib.scan("C")).await;
    let list = lib.open(ordered(TriageTab::Pending, ImportListOrder::PathAscending)).await;
    lib.select(list, SelectionChange::Extend { from: lib.key("C"), to: lib.key("Box/1") }).await;
    let selected = tables.load_import_list_selected(list).await.unwrap();
    assert_eq!(
        selected
            .candidates
            .iter()
            .map(|candidate| candidate.candidate_key.clone())
            .collect::<Vec<_>>(),
        [lib.key("Box/1"), lib.key("Box/2"), lib.key("C")],
        "the run in the list's order"
    );
    assert_eq!(
        tables
            .change_import_list_selection(
                list,
                SelectionChange::Extend { from: lib.key("A"), to: lib.key("Nowhere") },
            )
            .await
            .unwrap(),
        crate::tables::SelectionWrite::RunNotShown
    );
    lib.select(
        list,
        SelectionChange::Toggle { add: vec![lib.key("A")], remove: vec![lib.key("Box/2")] },
    )
    .await;
    assert_eq!(lib.read(list).await.selection.count, 3);
}
