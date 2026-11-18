//! What a list counts and what its filter entries hold, against what it
//! lists; where a candidate is revealed; and the first one being identified.

use super::*;
use crate::list::ImportListRead;
use crate::tables::import_list::tests::*;
use crate::ImportListItem;
use crate::ImportListView;
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
fn releases(projection: &ImportListRead) -> Vec<String> {
    projection
        .windows
        .iter()
        .flat_map(|window| window.rows.iter().map(|(item, _key)| item))
        .filter_map(|item| match item {
            ImportListItem::Candidate { row, .. } => Some(display_path(row)),
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
    for filter in [
        PendingFilter::All,
        PendingFilter::NeedsYou,
        PendingFilter::NotLookedUp,
        PendingFilter::Identified,
    ] {
        for text in ["", "1"] {
            let view = ImportListView {
                search: crate::SearchText::of(text),
                ..under(TriageTab::Pending, filter)
            };
            let kept = releases(&lib.shown(view.clone()).await);
            for folded in [&[][..], &["A"][..], &["A", "B"][..]] {
                let mut list = ImportListRequest::of(view.clone());
                for group in folded {
                    list = list.folding(lib.group_key(group));
                }
                let read = lib.read(&list).await;
                let what = format!("{filter:?}, text {text:?}, folded {folded:?}");
                assert_eq!(read.count.rows as usize, sequence(&read).len(), "{what}");
                match filter != PendingFilter::All || !text.is_empty() {
                    true => assert_eq!(
                        read.count.narrowed.map(|narrowed| narrowed.shown as usize),
                        Some(kept.len()),
                        "{what}"
                    ),
                    false => assert_eq!(read.count.narrowed, None, "{what}"),
                }
                for item in read
                    .windows
                    .iter()
                    .flat_map(|window| window.rows.iter().map(|(item, _key)| item))
                {
                    let ImportListItem::GroupHeader(crate::ImportGroupHeader {
                        group,
                        entry_count,
                        ..
                    }) = item
                    else {
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
    assert_eq!(read.count.rows, 0);
    assert_eq!(lib.tab_counts().await, crate::TriageTabCounts::default());
    assert!(!read.summary.has_groups);
}

#[tokio::test]
async fn a_release_counts_pending_and_an_invalid_folder_counts_skipped() {
    let lib = Library::new().await;
    lib.run(lib.scan("Release") + lib.scan_as("Broken", "invalid")).await;
    let pending = lib.shown(view(TriageTab::Pending)).await;
    assert_eq!(lib.tab_counts().await, crate::TriageTabCounts { pending: 1, done: 0, skipped: 1 });
    assert_eq!(sequence(&pending), ["candidate Release"]);
    assert_eq!(lib.items(view(TriageTab::Skipped)).await, ["invalid Broken"]);
}

/// The summary says whether bae watches any folder, so the screen knows to
/// show its list, and stops saying so after bae stops watching the last one.
#[tokio::test]
async fn the_summary_says_whether_any_folder_is_watched() {
    let lib = Library::new().await;
    assert!(lib.shown(view(TriageTab::Pending)).await.summary.has_watched_folders);
    lib.tables().remove_watched_import_folders(vec![lib.root.clone()], None).await.unwrap();
    assert!(!lib.shown(view(TriageTab::Pending)).await.summary.has_watched_folders);
}

/// A row the text filter or the filter entry narrows is counted beside the
/// tab's total, and only while one narrows it.
#[tokio::test]
async fn a_narrowed_tab_counts_what_it_shows_of_its_total() {
    let lib = Library::new().await;
    lib.run(lib.scan("Group/Wanted") + lib.scan("Group/Other") + lib.scan("Loose")).await;
    let narrowed = lib
        .shown(ImportListView {
            search: crate::SearchText::of("wanted"),
            ..view(TriageTab::Pending)
        })
        .await;
    assert_eq!(narrowed.count.narrowed, Some(crate::NarrowedCount { shown: 1, total: 3 }));
    assert_eq!(narrowed.count.rows, 2, "the header and its matching member");
    assert_eq!(lib.tab_counts().await.pending, 3, "the counts are the whole queue's");
    assert!(narrowed.summary.has_groups);
    let whole = lib.shown(view(TriageTab::Pending)).await;
    assert_eq!(whole.count.narrowed, None);
    assert_eq!(whole.count.rows, 4);
    assert!(whole.summary.has_watched_folders);
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
    assert_eq!(lib.tab_counts().await.pending, 14);
    assert!(read.pending_filter_entries.iter().all(|entry| entry.selectable));
    for entry in &read.pending_filter_entries {
        let shown = lib.shown(under(TriageTab::Pending, entry.filter)).await;
        assert_eq!(shown.count.rows as u32, entry.count, "{:?}", entry.filter);
        if entry.filter != PendingFilter::All {
            assert_eq!(
                shown.count.narrowed,
                Some(crate::NarrowedCount { shown: entry.count, total: 14 }),
                "the list's count under {:?} is the entry's",
                entry.filter
            );
        }
    }
    for other in [
        ImportListView {
            search: crate::SearchText::of("several"),
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
    let list = ImportListRequest::of(under(TriageTab::Pending, PendingFilter::InProgress));
    assert_eq!(lib.read(&list).await.count.rows, 2);
    lib.run(lib.reopened()).await;
    let ended = lib.read(&list).await;
    assert_eq!(ended.count.rows, 0);
    assert_eq!(ended.count.narrowed, Some(crate::NarrowedCount { shown: 0, total: 14 }));
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
            lib.shown(under(TriageTab::Done, filter)).await.count.narrowed,
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
        search: crate::SearchText::of(text),
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
    assert_eq!(lib.tab_counts().await.pending, 14);
    assert!(lib.items(texted(PendingFilter::Identified, "nothing")).await.is_empty());
    assert_eq!(
        lib.shown(texted(PendingFilter::All, "several")).await.count.narrowed,
        Some(crate::NarrowedCount { shown: 1, total: 14 }),
        "only the row with no draft reads its folder's name"
    );
    assert_eq!(
        lib.shown(ImportListView {
            search: crate::SearchText::of("set aside"),
            ..view(TriageTab::Skipped)
        })
        .await
        .count
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
    let first = |view: ImportListView| {
        let tables = tables.clone();
        async move { tables.first_identifying_import_candidate(view.order).await.unwrap() }
    };
    assert_eq!(first(view(TriageTab::Pending)).await, Some(lib.key("Release 2")));
    let filtered =
        ImportListView { search: crate::SearchText::of("Release 1"), ..view(TriageTab::Pending) };
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
    assert_eq!(
        tables.first_identifying_import_candidate(view(TriageTab::Pending).order).await.unwrap(),
        Some(lib.key("Group/Release 1"))
    );
}

/// Revealing a candidate clears the filters and opens its group, the list's
/// other folds and its order kept, whether the folds name the folded groups
/// or the open ones, and places the candidate where the revealed list shows
/// it.
#[tokio::test]
async fn revealing_a_candidate_opens_only_its_group() {
    let lib = Library::new().await;
    let tables = crate::tables::ImportTables::new(lib.db.clone());
    lib.run(
        lib.scan("Earlier/Release 1")
            + lib.scan("Earlier/Release 2")
            + lib.scan("Target/Release 1"),
    )
    .await;
    let target = lib.key("Target/Release 1");
    let searching = ImportListRequest::of(ImportListView {
        search: crate::SearchText::of("nothing matches this"),
        ..ordered(TriageTab::Pending, crate::ImportListOrder::PathAscending)
    });
    for (from, kept) in [
        (
            searching.clone().folding(lib.group_key("Earlier")).folding(lib.group_key("Target")),
            searching.clone().folding(lib.group_key("Earlier")).folds,
        ),
        (
            searching.clone().folding_all(),
            searching.clone().folding_all().unfolding(&lib.group_key("Target")).folds,
        ),
    ] {
        let reveal = tables
            .reveal_import_candidate(from.clone(), &target)
            .await
            .unwrap()
            .expect("the candidate is in Found");
        assert_eq!(
            reveal.request,
            ImportListRequest {
                view: ordered(TriageTab::Pending, crate::ImportListOrder::PathAscending),
                folds: kept,
            }
        );
        assert_eq!(reveal.position, 2, "the folded group's header, then the open one's");
        assert_eq!(
            tables.import_candidate_position(reveal.request, &target).await.unwrap(),
            Some(reveal.position)
        );
    }
}

/// Revealing a candidate follows its import from Found to Imported, and a key
/// no tab lists reveals nothing.
#[tokio::test]
async fn revealing_a_candidate_follows_an_import_from_pending_to_done() {
    let lib = Library::new().await;
    let tables = crate::tables::ImportTables::new(lib.db.clone());
    lib.run(
        scanned_with_states(&lib, &["Release", "Other"])
            + lib.imported("Release", "2026-01-02T00:00:00Z"),
    )
    .await;
    let reveal = tables
        .reveal_import_candidate(ImportListRequest::default(), &lib.key("Release"))
        .await
        .unwrap()
        .expect("the imported candidate is in Done");
    assert_eq!(reveal.request.view.tab, TriageTab::Done);
    assert_eq!(reveal.position, 0);
    assert_eq!(
        tables
            .reveal_import_candidate(ImportListRequest::default(), &lib.key("Nowhere"))
            .await
            .unwrap(),
        None
    );
}

/// Revealing a candidate clears the filters, so a row the entry hides is
/// still found where it sits.
#[tokio::test]
async fn revealing_a_candidate_ignores_the_pending_filter() {
    let lib = every_kind().await;
    let tables = crate::tables::ImportTables::new(lib.db.clone());
    let key = lib.key("Unidentified");
    let hiding = ImportListRequest::of(under(TriageTab::Pending, PendingFilter::Identified));
    assert_eq!(tables.import_candidate_position(hiding.clone(), &key).await.unwrap(), None);
    let reveal = tables
        .reveal_import_candidate(hiding, &key)
        .await
        .unwrap()
        .expect("the candidate is found");
    assert_eq!(reveal.request, ImportListRequest::of(view(TriageTab::Pending)));
    assert_eq!(
        tables.import_candidate_position(reveal.request, &key).await.unwrap(),
        Some(reveal.position)
    );
}

/// A candidate's position in the view counts only the rows the view shows: a
/// folded group shows its header alone, and a candidate the filter hides has
/// no position.
#[tokio::test]
async fn a_candidate_position_counts_what_the_view_shows() {
    let lib = Library::new().await;
    let tables = crate::tables::ImportTables::new(lib.db.clone());
    lib.run(
        lib.scan("Earlier/Release 1")
            + lib.scan("Earlier/Release 2")
            + lib.scan("Target/Release 1"),
    )
    .await;
    let list = ImportListRequest::of(view(TriageTab::Pending));
    let target = lib.key("Target/Release 1");
    let open = tables.import_candidate_position(list.clone(), &target).await.unwrap();
    let list = list.folding(lib.group_key("Earlier"));
    let folded = tables.import_candidate_position(list.clone(), &target).await.unwrap();
    assert_eq!(
        (open, folded),
        (Some(4), Some(2)),
        "two members of the folded group above it leave the view"
    );
    let member = lib.key("Earlier/Release 1");
    assert_eq!(
        tables.import_candidate_position(list.clone(), &member).await.unwrap(),
        None,
        "the view does not show a member of a folded group"
    );
}

/// A list's count includes every tab's count, whatever the list is narrowed
/// to: placing a candidate on Found adds one to Found's count, and skipping it
/// moves that one from Found to Skipped, even while the list is open on
/// Imported.
#[tokio::test]
async fn the_list_count_carries_the_tab_counts() {
    let lib = Library::new().await;
    let tables = crate::tables::ImportTables::new(lib.db.clone());
    let list = ImportListRequest::of(view(TriageTab::Done));
    let mut live = tables.subscribe_import_count(list.clone());
    let none = live.next_inline().await.expect("the query stays open").unwrap();
    assert_eq!(none.tabs, crate::TriageTabCounts::default());

    lib.run(lib.scan("Release")).await;
    let found = tokio::time::timeout(bae_util::test_channels::HANG_GUARD, live.next_inline())
        .await
        .expect("a placed candidate wakes the count")
        .expect("the query stays open")
        .unwrap();
    assert_eq!(found.tabs, crate::TriageTabCounts { pending: 1, done: 0, skipped: 0 });

    lib.run(lib.skip("Release")).await;
    let skipped = tokio::time::timeout(bae_util::test_channels::HANG_GUARD, live.next_inline())
        .await
        .expect("a skip wakes the count")
        .expect("the query stays open")
        .unwrap();
    assert_eq!(skipped.tabs, crate::TriageTabCounts { pending: 0, done: 0, skipped: 1 });
}

/// The tab counts query and the filter entries query read only the
/// placement's indexes, so no table stores the counts and neither query reads
/// every placement row's table row.
#[tokio::test]
async fn the_counts_are_counted_off_the_placements_indexes() {
    let lib = Library::new().await;
    let plans = lib
        .db
        .read(|sql| {
            [super::TAB_COUNTS.to_string(), super::filter_entries_sql()]
                .into_iter()
                .map(|statement| {
                    Ok(sql
                        .query(&format!("EXPLAIN QUERY PLAN {statement}"), [], |row| {
                            row.get::<_, String>(3)
                        })?
                        .join(" | "))
                })
                .collect::<Result<Vec<_>, bae_store::DbError>>()
        })
        .await
        .unwrap();
    assert!(
        plans[0].contains("USING COVERING INDEX import_list_placement_by_tab"),
        "the tab counts walk the tab and item index: {}",
        plans[0]
    );
    assert!(
        plans[1].contains("USING INDEX import_list_placement_by_entry")
            && !plans[1].contains("SCAN import_list_placement")
            && !plans[1].contains("SCAN p"),
        "the entry counts walk the entry index: {}",
        plans[1]
    );
}
