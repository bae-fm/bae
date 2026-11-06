//! What a list counts and what its filter entries hold, against what it
//! lists; where a candidate is revealed; and the first one being identified.

use super::*;
use crate::tables::import_list::tests::*;
use crate::ImportListItem;
use crate::ImportListProjection;
use crate::TriageTab;
use bae_catalogs::search::MetadataResult;

/// Two groups and two releases outside any, in two of Found's entries.
async fn grouped_in_two_entries() -> Library {
    let lib = Library::new().await;
    let releases = ["A/1", "A/2", "A/3", "B/1", "B/2", "C", "D"];
    let failed: Write = ["A/2", "B/1", "B/2", "C"]
        .into_iter()
        .map(|display_path| lib.verdict(display_path, "failed", Some(11)))
        .sum();
    lib.run(scanned_with_states(&lib, &releases) + failed).await;
    lib
}

/// The releases a read holds, by display path.
fn releases(projection: &ImportListProjection) -> Vec<String> {
    projection
        .windows
        .iter()
        .flat_map(|window| &window.items)
        .filter_map(|item| match item {
            ImportListItem::Candidate { row, .. } => Some(row.display_path.clone()),
            _ => None,
        })
        .collect()
}

/// Under every filter, with no group, one or both folded shut, a list counts
/// the items a read of all of it holds; its narrowed count is the releases its
/// filters keep, folded ones included; and each header counts the releases
/// its filters keep in its group.
#[tokio::test]
async fn a_list_counts_what_it_lists() {
    let lib = grouped_in_two_entries().await;
    let tables = crate::tables::ImportTables::new(lib.db.clone());
    for filter in [
        PendingFilter::All,
        PendingFilter::NeedsYou,
        PendingFilter::NotLookedUp,
        PendingFilter::Identified,
    ] {
        for text in ["", "1"] {
            let view = ImportListView {
                filter_text: text.to_string(),
                ..under(TriageTab::Pending, filter)
            };
            let kept = releases(&lib.shown(view.clone()).await);
            for folded in [&[][..], &["A"][..], &["A", "B"][..]] {
                let list = lib.open(view.clone()).await;
                for group in folded {
                    tables
                        .set_import_list_group_folded(list, lib.group_key(group), true)
                        .await
                        .unwrap();
                }
                let read = lib.read(list).await;
                let what = format!("{filter:?}, text {text:?}, folded {folded:?}");
                assert_eq!(read.total_count as usize, sequence(&read).len(), "{what}");
                match filter != PendingFilter::All || !text.is_empty() {
                    true => assert_eq!(
                        read.summary.narrowed.map(|narrowed| narrowed.shown as usize),
                        Some(kept.len()),
                        "{what}"
                    ),
                    false => assert_eq!(read.summary.narrowed, None, "{what}"),
                }
                for item in read.windows.iter().flat_map(|window| &window.items) {
                    let ImportListItem::GroupHeader { group, entry_count, .. } = item else {
                        continue;
                    };
                    let members = kept
                        .iter()
                        .filter(|path| path.starts_with(&format!("{}/", group.name)))
                        .count();
                    assert_eq!(*entry_count as usize, members, "{what}, group {}", group.name);
                }
            }
        }
    }
}

/// A tentative candidate is a release the scan found before it knew what
/// enclosed it: not a row, not a count, and not a group.
#[tokio::test]
async fn a_tentative_candidate_is_neither_a_row_nor_a_count() {
    let lib = Library::new().await;
    lib.run(lib.scan_as("Box/CD1", "tentative")).await;
    let read = lib.shown(view(TriageTab::Pending)).await;
    assert!(sequence(&read).is_empty());
    assert_eq!(read.total_count, 0);
    assert_eq!(read.summary.counts, crate::TriageTabCounts::default());
    assert!(!read.summary.has_groups);
}

#[tokio::test]
async fn a_release_counts_pending_and_an_invalid_folder_counts_skipped() {
    let lib = Library::new().await;
    lib.run(lib.scan("Release") + lib.scan_as("Broken", "invalid")).await;
    let pending = lib.shown(view(TriageTab::Pending)).await;
    assert_eq!(pending.summary.counts, crate::TriageTabCounts { pending: 1, done: 0, skipped: 1 });
    assert_eq!(sequence(&pending), ["candidate Release"]);
    assert_eq!(lib.items(view(TriageTab::Skipped)).await, ["invalid Broken"]);
}

/// A row the text filter or the filter entry narrows is counted beside the
/// tab's total, and only while one narrows it.
#[tokio::test]
async fn a_narrowed_tab_counts_what_it_shows_of_its_total() {
    let lib = Library::new().await;
    lib.run(lib.scan("Group/Wanted") + lib.scan("Group/Other") + lib.scan("Loose")).await;
    let narrowed = lib
        .shown(ImportListView { filter_text: "wanted".to_string(), ..view(TriageTab::Pending) })
        .await;
    assert_eq!(narrowed.summary.narrowed, Some(crate::NarrowedCount { shown: 1, total: 3 }));
    assert_eq!(narrowed.total_count, 2, "the header and its matching member");
    assert_eq!(narrowed.summary.counts.pending, 3, "the counts are the whole queue's");
    assert!(narrowed.summary.has_groups);
    let whole = lib.shown(view(TriageTab::Pending)).await;
    assert_eq!(whole.summary.narrowed, None);
    assert_eq!(whole.total_count, 4);
    assert_eq!(whole.summary.watched_folders.len(), 1);
    assert_eq!(
        whole.summary.narrowing,
        crate::ImportListNarrowing {
            tab: TriageTab::Pending,
            filter_text: String::new(),
            pending_filter: PendingFilter::All,
        }
    );
}

impl Library {
    /// The person's own draft of the release at `display_path`, titled
    /// Album and credited to one artist, that would import.
    fn answered(&self, display_path: &str) -> Write {
        self.metadata(
            display_path,
            crate::tables::import_state::tests::metadata_draft("Album", "Artist"),
            None,
        )
    }
}

/// Every kind of Found row the rules tell apart, and a Done and a Skipped
/// one, each written as the import writes it.
async fn every_kind() -> Library {
    let lib = Library::new().await;
    let untagged = [
        "Identified",
        "Picked Among Several",
        "Track Count Differs",
        "Several",
        "Nothing Found",
        "Lookup Failed",
        "Failed Import",
        "Unidentified",
        "Unread",
        "Nothing To Look Up",
        "Broke",
        "Imported",
        "Set Aside",
    ];
    // Read from tags that name the album, its artist and its track: a draft
    // that would import.
    let tagged = lib.scan_tagged("Several Tagged", "Album", "Artist", Some("Track title"))
        + lib.scan_tagged("Tagged", "Album", "Artist", Some("Track title"));
    // Read from tags that leave the track untitled, then changed by the
    // person: their draft, which would not import.
    let set_aside = lib.scan_tagged("Pick Set Aside", "Album", "Artist", None)
        + lib.typed("Pick Set Aside", crate::CandidateEditField::PressingYear, "2026");
    let answered: Write =
        ["Identified", "Picked Among Several", "Track Count Differs", "Failed Import"]
            .into_iter()
            .map(|display_path| lib.answered(display_path) + lib.linked(display_path))
            .sum();
    let unread = MetadataResult {
        document_failure: Some(bae_catalogs::LookupFailure::Timeout),
        ..found_release(0, FITS)
    };
    lib.run(
        scanned_with_states(&lib, &untagged)
            + tagged
            + set_aside
            + answered
            + lib.found("Identified", 1, FITS)
            + lib.found("Picked Among Several", 2, FITS)
            + lib.found("Track Count Differs", 1, (Some("listed"), Some(10)))
            + lib.found("Several", 2, FITS)
            + lib.found("Several Tagged", 2, FITS)
            + lib.found("Pick Set Aside", 1, FITS)
            + lib.concluded("Unread", found_verdict(11, [(unread, 0)]))
            + lib.verdict("Nothing Found", "not_found", None)
            + lib.verdict("Lookup Failed", "failed", Some(11))
            + lib.verdict("Nothing To Look Up", "manual_only", Some(11))
            + lib.verdict("Broke", "error", None)
            + lib.import_failed("Failed Import")
            + lib.imported("Imported", "2026-01-02T00:00:00Z")
            + lib.skip("Set Aside"),
    )
    .await;
    lib
}

/// Where three of [`every_kind`]'s candidates stand: a run for Several, an
/// import of Identified, and a run whose answer bae could not store for
/// Tagged.
async fn running(lib: &Library) {
    lib.run(
        lib.identifying("Several", "running")
            + lib.importing("Identified", "reading_files")
            + lib.not_written("Tagged", "writing the answer: the disk is full"),
    )
    .await;
}

/// The release rows `view` shows, sorted, one readable line each.
async fn sorted(lib: &Library, view: ImportListView) -> Vec<String> {
    let mut shown = lib.items(view).await;
    shown.sort();
    shown
}

/// Each entry shows exactly the rows in the states it covers, with what is
/// running for a candidate deciding its state over the tables; the entries
/// past All split Found's rows between them.
#[tokio::test]
async fn each_entry_shows_exactly_the_rows_of_its_states() {
    let lib = every_kind().await;
    running(&lib).await;
    let cases = [
        (
            PendingFilter::NeedsYou,
            vec![
                "candidate Broke",
                "candidate Failed Import",
                "candidate Lookup Failed",
                "candidate Nothing Found",
                "candidate Nothing To Look Up",
                "candidate Several Tagged",
                "candidate Tagged",
                "candidate Unread",
            ],
        ),
        (PendingFilter::InProgress, vec!["candidate Identified", "candidate Several"]),
        (
            PendingFilter::Identified,
            vec!["candidate Picked Among Several", "candidate Track Count Differs"],
        ),
        (PendingFilter::Unmatched, vec!["candidate Pick Set Aside"]),
        (PendingFilter::NotLookedUp, vec!["candidate Unidentified"]),
    ];
    let mut every: Vec<String> = Vec::new();
    for (filter, expected) in cases {
        let shown = sorted(&lib, under(TriageTab::Pending, filter)).await;
        assert_eq!(shown, expected, "{filter:?}");
        every.extend(shown);
    }
    every.sort();
    assert_eq!(
        every,
        sorted(&lib, under(TriageTab::Pending, PendingFilter::All)).await,
        "each row shows under exactly one entry past All, and All shows them all"
    );
}

/// Each entry counts the rows it holds, from a read under any view, with
/// where each candidate stands deciding its row's state over the tables: the
/// count is the rows the list shows under that entry, All's is Found's total,
/// and an entry holding a row can be chosen.
#[tokio::test]
async fn each_entry_counts_the_rows_it_holds() {
    let lib = every_kind().await;
    let at_rest = lib.shown(view(TriageTab::Pending)).await;
    assert_eq!(
        entry_counts(&at_rest.pending_filter_entries),
        [
            (PendingFilter::All, 14),
            (PendingFilter::NeedsYou, 8),
            (PendingFilter::InProgress, 0),
            (PendingFilter::Identified, 3),
            (PendingFilter::Unmatched, 1),
            (PendingFilter::NotLookedUp, 2),
        ]
    );
    let in_progress = at_rest
        .pending_filter_entries
        .iter()
        .find(|entry| entry.filter == PendingFilter::InProgress)
        .expect("In Progress is an entry");
    assert!(!in_progress.selectable, "an entry holding no row cannot be chosen");

    running(&lib).await;
    let read = lib.shown(view(TriageTab::Pending)).await;
    assert_eq!(
        entry_counts(&read.pending_filter_entries),
        [
            (PendingFilter::All, 14),
            (PendingFilter::NeedsYou, 8),
            (PendingFilter::InProgress, 2),
            (PendingFilter::Identified, 2),
            (PendingFilter::Unmatched, 1),
            (PendingFilter::NotLookedUp, 1),
        ]
    );
    assert_eq!(read.summary.counts.pending, 14);
    assert!(read.pending_filter_entries.iter().all(|entry| entry.selectable));
    for entry in &read.pending_filter_entries {
        let shown = lib.shown(under(TriageTab::Pending, entry.filter)).await;
        assert_eq!(shown.total_count as u32, entry.count, "{:?}", entry.filter);
        if entry.filter != PendingFilter::All {
            assert_eq!(
                shown.summary.narrowed,
                Some(crate::NarrowedCount { shown: entry.count, total: 14 }),
                "the list's count under {:?} is the entry's",
                entry.filter
            );
        }
    }
    for other in [
        ImportListView {
            filter_text: "several".to_string(),
            ..under(TriageTab::Pending, PendingFilter::NeedsYou)
        },
        view(TriageTab::Done),
        view(TriageTab::Skipped),
    ] {
        assert_eq!(
            lib.shown(other.clone()).await.pending_filter_entries,
            read.pending_filter_entries,
            "the counts are Found's whatever {other:?} shows"
        );
    }
}

/// An entry chosen while it held rows stays chosen once it holds none: the
/// list shows nothing under it, and says so against Found's total.
#[tokio::test]
async fn a_chosen_entry_holding_no_row_stays_chosen() {
    let lib = every_kind().await;
    running(&lib).await;
    let list = lib.open(under(TriageTab::Pending, PendingFilter::InProgress)).await;
    assert_eq!(lib.read(list).await.total_count, 2);
    lib.run(lib.reopened()).await;
    let ended = lib.read(list).await;
    assert_eq!(ended.total_count, 0);
    assert_eq!(ended.summary.narrowing.pending_filter, PendingFilter::InProgress);
    assert_eq!(ended.summary.narrowed, Some(crate::NarrowedCount { shown: 0, total: 14 }));
}

/// Done and Skipped rows are past identification, so an entry chosen on
/// Found leaves them alone.
#[tokio::test]
async fn the_pending_filter_leaves_done_and_skipped_alone() {
    let lib = every_kind().await;
    for filter in PendingFilter::ENTRIES {
        assert_eq!(lib.items(under(TriageTab::Done, filter)).await, ["candidate Imported"]);
        assert_eq!(lib.items(under(TriageTab::Skipped, filter)).await, ["candidate Set Aside"]);
        assert_eq!(
            lib.shown(under(TriageTab::Done, filter)).await.summary.narrowed,
            None,
            "the entry leaves Done alone"
        );
    }
}

/// The entry composes with the text filter: both have to keep a row.
#[tokio::test]
async fn the_text_filter_composes_with_the_pending_filter() {
    let lib = every_kind().await;
    let texted = |filter: PendingFilter, text: &str| ImportListView {
        filter_text: text.to_string(),
        ..under(TriageTab::Pending, filter)
    };
    let needs_you = lib.shown(texted(PendingFilter::NeedsYou, "album")).await;
    let mut shown = sequence(&needs_you);
    shown.sort();
    assert_eq!(
        shown,
        ["candidate Failed Import", "candidate Several Tagged"],
        "of the Needs You rows only the drafted ones read Album"
    );
    assert_eq!(needs_you.summary.counts.pending, 14);
    assert!(lib.items(texted(PendingFilter::Identified, "nothing")).await.is_empty());
    assert_eq!(
        lib.shown(texted(PendingFilter::All, "several")).await.summary.narrowed,
        Some(crate::NarrowedCount { shown: 1, total: 14 }),
        "only the row with no draft reads its folder's name"
    );
    assert_eq!(
        lib.shown(ImportListView {
            filter_text: "set aside".to_string(),
            ..view(TriageTab::Skipped)
        })
        .await
        .summary
        .narrowed,
        Some(crate::NarrowedCount { shown: 1, total: 1 })
    );
}

/// The first of several identifying candidates is the one the queue reaches
/// first, not the one named first, whatever the filter hides or the tab on
/// show; a group's is its first member.
#[tokio::test]
async fn the_first_identifying_candidate_follows_the_queue_order() {
    let lib = Library::new().await;
    let tables = crate::tables::ImportTables::new(lib.db.clone());
    lib.run(
        lib.scan("Release 1") + lib.scan("Release 2") + lib.identifying("Release 2", "running"),
    )
    .await;
    let first = |view: ImportListView| async {
        let list = lib.open(view).await;
        tables.first_identifying_import_candidate(list).await.unwrap()
    };
    assert_eq!(first(view(TriageTab::Pending)).await, Some(lib.key("Release 2")));
    let filtered =
        ImportListView { filter_text: "Release 1".to_string(), ..view(TriageTab::Pending) };
    assert_eq!(first(filtered).await, Some(lib.key("Release 2")));
    assert_eq!(first(view(TriageTab::Done)).await, Some(lib.key("Release 2")));
    lib.run(lib.identifying("Release 1", "queued")).await;
    assert_eq!(first(view(TriageTab::Pending)).await, Some(lib.key("Release 1")));
    lib.run(lib.reopened()).await;
    assert_eq!(first(view(TriageTab::Pending)).await, None);
}

#[tokio::test]
async fn the_first_identifying_candidate_of_a_group_is_its_first_member() {
    let lib = Library::new().await;
    let tables = crate::tables::ImportTables::new(lib.db.clone());
    lib.run(
        lib.scan("Group/Release 1")
            + lib.scan("Group/Release 2")
            + lib.identifying("Group/Release 2", "running")
            + lib.identifying("Group/Release 1", "running"),
    )
    .await;
    let list = lib.open(view(TriageTab::Pending)).await;
    assert_eq!(
        tables.first_identifying_import_candidate(list).await.unwrap(),
        Some(lib.key("Group/Release 1"))
    );
}

/// Locating a candidate clears the filters and opens its group, the list's
/// other folds kept.
#[tokio::test]
async fn candidate_location_opens_only_its_group() {
    let lib = Library::new().await;
    let tables = crate::tables::ImportTables::new(lib.db.clone());
    lib.run(
        lib.scan("Earlier/Release 1")
            + lib.scan("Earlier/Release 2")
            + lib.scan("Target/Release 1"),
    )
    .await;
    let list = lib
        .open(ImportListView {
            filter_text: "does not match".to_string(),
            pending_filter: PendingFilter::NeedsYou,
            ..view(TriageTab::Pending)
        })
        .await;
    let fold = |group: &str| crate::FolderReleaseDecisionKey {
        watched_folder_path: lib.root.clone(),
        relative_folder: crate::RelativeFolder::test(group),
    };
    tables.set_import_list_groups_folded(list, true).await.unwrap();
    let location = tables
        .locate_import_candidate(list, &lib.key("Target/Release 1"))
        .await
        .unwrap()
        .expect("the candidate is in Pending");
    assert_eq!(location.tab, TriageTab::Pending);
    assert_eq!(location.group_key, Some(fold("Target")));
    assert_eq!(
        location.stable_key,
        crate::list::candidate_stable_key(&lib.key("Target/Release 1"))
    );
    assert_eq!(location.visible_position, 2);
}

#[tokio::test]
async fn candidate_location_follows_an_import_from_pending_to_done() {
    let lib = Library::new().await;
    let tables = crate::tables::ImportTables::new(lib.db.clone());
    lib.run(
        scanned_with_states(&lib, &["Release"]) + lib.imported("Release", "2026-01-02T00:00:00Z"),
    )
    .await;
    let list = lib.open(view(TriageTab::Pending)).await;
    let location = tables
        .locate_import_candidate(list, &lib.key("Release"))
        .await
        .unwrap()
        .expect("the imported candidate is in Done");
    assert_eq!(location.tab, TriageTab::Done);
    assert_eq!(location.group_key, None);
    assert_eq!(location.visible_position, 0);
    assert_eq!(tables.locate_import_candidate(list, &lib.key("Nowhere")).await.unwrap(), None);
}

/// Locating a candidate clears the filters, so a row the entry hides is
/// still found where it sits.
#[tokio::test]
async fn locating_a_candidate_ignores_the_pending_filter() {
    let lib = every_kind().await;
    let tables = crate::tables::ImportTables::new(lib.db.clone());
    let list = lib.open(under(TriageTab::Pending, PendingFilter::Identified)).await;
    let location = tables
        .locate_import_candidate(list, &lib.key("Unidentified"))
        .await
        .unwrap()
        .expect("the candidate is found");
    assert_eq!(location.tab, TriageTab::Pending);
}
