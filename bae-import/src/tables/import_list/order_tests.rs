//! Which items a list shows and in what order: its roots, its natural path
//! order, its dates, its groups and their headers, folds, windows, what its
//! text filter finds, and the indexes each order reads off.

use super::*;
use crate::tables::import_list::tests::*;
use crate::ImportListItem;
use crate::ImportListProjection;
use bae_catalogs::search::MetadataResult;

impl Library {
    /// The scan's releases at `display_paths`, each its folder's date.
    fn dated(&self, releases: &[(&str, i64)]) -> Write {
        releases.iter().map(|(display_path, at)| self.scan_dated(display_path, *at)).sum()
    }
}

#[tokio::test]
async fn rows_order_by_watched_root_then_natural_path() {
    let lib = Library::new().await;
    let second = crate::tables::watched_path::host_root("/second");
    lib.run(
        lib.scan("Release 10")
            + lib.scan("Release 2")
            + lib.scan_other_root(&second, "Release 1", None),
    )
    .await;
    assert_eq!(
        lib.items(view(TriageTab::Pending)).await,
        ["candidate Release 2", "candidate Release 10", "candidate Release 1"],
        "natural order within a root, and the roots in their stored order"
    );
}

#[tokio::test]
async fn the_descending_order_reverses_the_whole_queue() {
    let lib = Library::new().await;
    lib.run(lib.scan("Release 2") + lib.scan("Release 10")).await;
    assert_eq!(
        lib.items(ordered(TriageTab::Pending, ImportListOrder::PathDescending)).await,
        ["candidate Release 10", "candidate Release 2"]
    );
}

#[tokio::test]
async fn a_collapsed_group_keeps_its_header_and_drops_its_entries() {
    let lib = Library::new().await;
    let tables = crate::tables::ImportTables::new(lib.db.clone());
    lib.run(lib.scan("Group/Release 1") + lib.scan("Group/Release 2")).await;
    let list = lib.open(view(TriageTab::Pending)).await;
    tables.set_import_list_group_folded(list, lib.group_key("Group"), true).await.unwrap();
    let read = lib.read(list).await;
    assert_eq!(sequence(&read), ["group Group"]);
    assert_eq!(read.total_count, 1);
    let ImportListItem::GroupHeader { expanded, entry_count, .. } = &read.windows[0].items[0]
    else {
        panic!("the header is the one item");
    };
    assert!(!expanded);
    assert_eq!(*entry_count, 2, "the header still says how much is folded away");

    tables.set_import_list_group_folded(list, lib.group_key("Group"), false).await.unwrap();
    assert_eq!(
        sequence(&lib.read(list).await),
        ["group Group", "candidate Group/Release 1", "candidate Group/Release 2"]
    );
}

/// The position of the header each release is listed under, in a read's
/// order: `None` for a release in no group, and for each header.
fn header_positions(projection: &ImportListProjection) -> Vec<Option<u64>> {
    projection
        .windows
        .iter()
        .flat_map(|window| &window.items)
        .map(|item| match item {
            ImportListItem::Candidate { header_position, .. } => *header_position,
            ImportListItem::GroupHeader { .. } | ImportListItem::Invalid { .. } => None,
        })
        .collect()
}

/// A release in a group names where its group's header sits in the list,
/// and one in no group names none.
#[tokio::test]
async fn a_group_member_names_its_headers_position() {
    let lib = Library::new().await;
    lib.run(lib.scan("A/One") + lib.scan("A/Two") + lib.scan("B/Three") + lib.scan("Loose")).await;
    let read = lib.shown(ordered(TriageTab::Pending, ImportListOrder::PathAscending)).await;
    assert_eq!(
        sequence(&read),
        [
            "group A",
            "candidate A/One",
            "candidate A/Two",
            "group B",
            "candidate B/Three",
            "candidate Loose"
        ]
    );
    assert_eq!(header_positions(&read), [None, Some(0), Some(0), None, Some(3), None]);
}

/// A window that starts among a group's members names the header before it,
/// which it does not hold: the header the rows on top are listed under,
/// whichever windows a surface has loaded.
#[tokio::test]
async fn a_window_starting_inside_a_group_names_the_header_before_it() {
    let lib = Library::new().await;
    lib.run(lib.scan("A/Disc 1") + lib.scan("A/Disc 2") + lib.scan("A/Disc 3") + lib.scan("Loose"))
        .await;
    let list = lib.open(ordered(TriageTab::Pending, ImportListOrder::PathAscending)).await;
    let read = lib.read_through(list, LibraryPageWindow { offset: 2, limit: 3 }).await;
    assert_eq!(sequence(&read), ["candidate A/Disc 2", "candidate A/Disc 3", "candidate Loose"]);
    assert_eq!(header_positions(&read), [Some(0), Some(0), None]);
}

/// A list that folds a group shut reads its windows past the folded members,
/// and an open group's members still name where their header sits: its place
/// among the items the list shows, the folded group's header counted and its
/// members not.
#[tokio::test]
async fn a_folded_lists_members_name_their_headers_position() {
    let lib = Library::new().await;
    let tables = crate::tables::ImportTables::new(lib.db.clone());
    lib.run(
        lib.scan("A/Disc 1")
            + lib.scan("A/Disc 2")
            + lib.scan("B/Disc 1")
            + lib.scan("B/Disc 2")
            + lib.scan("B/Disc 3")
            + lib.scan("Loose"),
    )
    .await;
    let list = lib.open(ordered(TriageTab::Pending, ImportListOrder::PathAscending)).await;
    tables.set_import_list_group_folded(list, lib.group_key("A"), true).await.unwrap();
    let read = lib.read(list).await;
    assert_eq!(
        sequence(&read),
        [
            "group A",
            "group B",
            "candidate B/Disc 1",
            "candidate B/Disc 2",
            "candidate B/Disc 3",
            "candidate Loose"
        ]
    );
    assert_eq!(header_positions(&read), [None, None, Some(1), Some(1), Some(1), None]);
    let window = lib.read_through(list, LibraryPageWindow { offset: 3, limit: 3 }).await;
    assert_eq!(sequence(&window), ["candidate B/Disc 2", "candidate B/Disc 3", "candidate Loose"]);
    assert_eq!(header_positions(&window), [Some(1), Some(1), None]);
}

/// A group header asks how the folder under it is read, and offers to read it
/// the other way. Neither question survives the import, so Done and Skipped are
/// flat lists of releases and only Pending groups.
#[tokio::test]
async fn only_pending_groups_its_rows() {
    let lib = Library::new().await;
    lib.run(
        scanned_with_states(&lib, &["Group/Release 1", "Group/Release 2"])
            + lib.imported("Group/Release 1", "2026-01-02T00:00:00Z")
            + lib.skip("Group/Release 2"),
    )
    .await;
    let pending = lib.shown(view(TriageTab::Pending)).await;
    assert!(sequence(&pending).is_empty(), "both rows left Pending");
    assert!(!pending.summary.has_groups);
    assert_eq!(lib.items(view(TriageTab::Done)).await, ["candidate Group/Release 1"]);
    assert_eq!(lib.items(view(TriageTab::Skipped)).await, ["candidate Group/Release 2"]);
}

/// Done is ordered by when the import happened, newest first, then by path.
#[tokio::test]
async fn done_orders_by_newest_import_first() {
    let lib = Library::new().await;
    lib.run(
        scanned_with_states(&lib, &["A old", "B new", "C middle"])
            + lib.imported("A old", "2026-01-01T00:00:00Z")
            + lib.imported("B new", "2026-01-03T00:00:00.250+00:00")
            + lib.imported("C middle", "2026-01-02T00:00:00Z"),
    )
    .await;
    assert_eq!(
        lib.items(view(TriageTab::Done)).await,
        ["candidate B new", "candidate C middle", "candidate A old"]
    );
    assert_eq!(
        lib.items(ordered(TriageTab::Done, ImportListOrder::OldestFirst)).await,
        ["candidate A old", "candidate C middle", "candidate B new"]
    );
}

/// The list is newest first unless asked otherwise; a group sorts by its
/// newest member, and a folder the filesystem gives no date for by when the
/// scans first saw it.
#[tokio::test]
async fn default_is_newest_first_and_groups_use_their_newest_member() {
    let lib = Library::new().await;
    let tables = crate::tables::ImportTables::new(lib.db.clone());
    lib.run(
        lib.dated(&[("A/Old", 10), ("B/Recent", 30), ("A/New", 50), ("Single", 40)])
            + lib.scan("Seen"),
    )
    .await;
    assert_eq!(ImportListView::default().order, ImportListOrder::NewestFirst);
    assert_eq!(
        lib.items(view(TriageTab::Pending)).await,
        [
            "candidate Seen",
            "group A",
            "candidate A/New",
            "candidate A/Old",
            "candidate Single",
            "group B",
            "candidate B/Recent",
        ]
    );
    assert_eq!(
        lib.items(ordered(TriageTab::Pending, ImportListOrder::OldestFirst)).await,
        [
            "group B",
            "candidate B/Recent",
            "candidate Single",
            "group A",
            "candidate A/Old",
            "candidate A/New",
            "candidate Seen",
        ]
    );
    let list = lib.open(view(TriageTab::Pending)).await;
    let location = tables.locate_import_candidate(list, &lib.key("Single")).await.unwrap().unwrap();
    assert_eq!(location.visible_position, 4);
}

#[tokio::test]
async fn dates_cross_watched_roots_and_ties_have_a_stable_natural_path_order() {
    let lib = Library::new().await;
    let second = crate::tables::watched_path::host_root("/second");
    lib.run(
        lib.dated(&[("Release 10", 10), ("Release 2", 10)])
            + lib.scan_other_root(&second, "Newest", Some(20)),
    )
    .await;
    assert_eq!(
        lib.items(view(TriageTab::Pending)).await,
        ["candidate Newest", "candidate Release 2", "candidate Release 10"]
    );
}

#[tokio::test]
async fn name_orders_ignore_dates_and_case_ties_do_not_split_groups() {
    let lib = Library::new().await;
    lib.run(lib.dated(&[("Box/1", 10), ("box/2", 30), ("Box/3", 20)])).await;
    assert_eq!(
        lib.items(ordered(TriageTab::Pending, ImportListOrder::PathAscending)).await,
        ["group Box", "candidate Box/1", "candidate Box/3", "group box", "candidate box/2"]
    );
    assert_eq!(
        lib.items(ordered(TriageTab::Pending, ImportListOrder::PathDescending)).await,
        ["group box", "candidate box/2", "group Box", "candidate Box/3", "candidate Box/1"]
    );
}

#[tokio::test]
async fn filtering_collapsing_and_paging_preserve_group_date_order() {
    let lib = Library::new().await;
    let tables = crate::tables::ImportTables::new(lib.db.clone());
    lib.run(lib.dated(&[("A/Old", 10), ("B/Old", 20), ("A/New", 30)])).await;
    let filtered = ImportListView { filter_text: "Old".into(), ..view(TriageTab::Pending) };
    let list = lib.open(filtered).await;
    assert_eq!(
        sequence(&lib.read(list).await),
        ["group A", "candidate A/Old", "group B", "candidate B/Old"]
    );
    assert_eq!(
        sequence(&lib.read_through(list, LibraryPageWindow { offset: 1, limit: 2 }).await),
        ["candidate A/Old", "group B"]
    );
    tables.set_import_list_group_folded(list, lib.group_key("A"), true).await.unwrap();
    assert_eq!(sequence(&lib.read(list).await), ["group A", "group B", "candidate B/Old"]);
}

/// A window past the end holds what is left of the list, and one wholly past
/// it holds nothing.
#[tokio::test]
async fn a_window_past_the_end_holds_what_is_left() {
    let lib = Library::new().await;
    lib.run(lib.scan("A") + lib.scan("B") + lib.scan("C")).await;
    let list = lib.open(ordered(TriageTab::Pending, ImportListOrder::PathAscending)).await;
    let read = lib.read_through(list, LibraryPageWindow { offset: 2, limit: 5 }).await;
    assert_eq!(sequence(&read), ["candidate C"]);
    assert_eq!(read.total_count, 3);
    assert!(sequence(&lib.read_through(list, LibraryPageWindow { offset: 5, limit: 5 }).await)
        .is_empty());
}

impl Library {
    /// The items a list on `tab` shows with `needle` typed into its filter.
    async fn found_by(&self, tab: TriageTab, needle: &str) -> Vec<String> {
        self.items(ImportListView { filter_text: needle.to_string(), ..view(tab) }).await
    }
}

/// A row with no draft shows its folder's name and nothing else, so that is
/// what finds it; its path is not on screen and finds nothing.
#[tokio::test]
async fn the_filter_finds_an_undrafted_row_by_its_folder_name_not_its_path() {
    let lib = Library::new().await;
    lib.run(lib.scan("Group/Wanted") + lib.scan("Other")).await;
    assert_eq!(
        lib.found_by(TriageTab::Pending, "want").await,
        ["group Group", "candidate Group/Wanted"],
        "an emptied group drops its header, a matching one keeps it"
    );
    assert!(lib.found_by(TriageTab::Pending, "group/").await.is_empty());
}

/// A drafted row shows its draft's title and artists — not the verdict's
/// lead, and not its folder — so those are what find it, whatever their case.
#[tokio::test]
async fn the_filter_finds_a_drafted_row_by_the_draft_it_shows() {
    let lib = Library::new().await;
    let lead = MetadataResult { title: "Album Title".to_string(), ..found_release(0, FITS) };
    lib.run(
        scanned_with_states(&lib, &["Folder", "Other"])
            + lib.draft("Folder", "Album")
            + lib.credited_to("Folder", "Artist")
            + lib.concluded("Folder", found_verdict(11, [(lead, 0)])),
    )
    .await;
    for needle in ["album", "ARTIST"] {
        assert_eq!(
            lib.found_by(TriageTab::Pending, needle).await,
            ["candidate Folder"],
            "{needle} is on the row"
        );
    }
    for needle in ["album title", "folder"] {
        assert!(
            lib.found_by(TriageTab::Pending, needle).await.is_empty(),
            "{needle} is the verdict's lead or the folder, which the row does not show"
        );
    }
}

/// The filter folds case as library search does, beyond ASCII: a needle in
/// one case finds the text in another.
#[tokio::test]
async fn the_filter_folds_case_beyond_ascii() {
    let lib = Library::new().await;
    lib.run(scanned_with_states(&lib, &["Folder"]) + lib.draft("Folder", "ÉTÉ Œuvre")).await;
    assert_eq!(lib.found_by(TriageTab::Pending, "été œuvre").await, ["candidate Folder"]);
}

/// What is typed is matched as itself: `*`, `?` and `[`, which a `GLOB`
/// pattern reads as wildcards, find only text holding them.
#[tokio::test]
async fn the_filter_matches_wildcard_characters_as_themselves() {
    let lib = Library::new().await;
    lib.run(
        scanned_with_states(&lib, &["Starred", "Asked", "Bracketed", "Plain"])
            + lib.draft("Starred", "A*B")
            + lib.draft("Asked", "Why?")
            + lib.draft("Bracketed", "[Live]"),
    )
    .await;
    for (needle, found) in [
        ("*", "candidate Starred"),
        ("a*b", "candidate Starred"),
        ("?", "candidate Asked"),
        ("why?", "candidate Asked"),
        ("[", "candidate Bracketed"),
        ("[live]", "candidate Bracketed"),
    ] {
        assert_eq!(lib.found_by(TriageTab::Pending, needle).await, [found], "{needle}");
    }
}

/// A search of one or two characters, shorter than any run the search index
/// holds, finds every row holding it all the same.
#[tokio::test]
async fn the_filter_finds_one_or_two_characters() {
    let lib = Library::new().await;
    lib.run(
        scanned_with_states(&lib, &["Folder", "Other"])
            + lib.draft("Folder", "Xy")
            + lib.draft("Other", "Yz"),
    )
    .await;
    assert_eq!(
        lib.found_by(TriageTab::Pending, "y").await,
        ["candidate Folder", "candidate Other"]
    );
    assert_eq!(lib.found_by(TriageTab::Pending, "XY").await, ["candidate Folder"]);
    assert!(lib.found_by(TriageTab::Pending, "q").await.is_empty());
}

/// A Done row shows its library album's title, so that is what finds it, and
/// nothing the candidate's draft or folder says does.
#[tokio::test]
async fn the_filter_finds_a_done_row_by_its_library_album() {
    let lib = Library::new().await;
    lib.run(
        scanned_with_states(&lib, &["Folder", "Other"])
            + lib.draft("Other", "Draft Title")
            + lib.credited_to("Other", "Draft Artist")
            + lib.imported("Folder", "2026-01-02T00:00:00Z")
            + lib.imported_as("Other", "2026-01-03T00:00:00Z", "Second Album"),
    )
    .await;
    assert_eq!(
        lib.found_by(TriageTab::Done, "album").await,
        ["candidate Other", "candidate Folder"]
    );
    assert_eq!(lib.found_by(TriageTab::Done, "second").await, ["candidate Other"]);
    for needle in ["folder", "draft title", "draft artist"] {
        assert!(
            lib.found_by(TriageTab::Done, needle).await.is_empty(),
            "{needle} is not on a Done row"
        );
    }
}

#[tokio::test]
async fn the_filter_matches_an_invalid_folder_by_its_name_and_its_tags() {
    let lib = Library::new().await;
    lib.run(
        lib.scan_as("Broken", "invalid") + lib.scan_invalid("Tagged", Some(("Tagged Title", None))),
    )
    .await;
    assert_eq!(lib.found_by(TriageTab::Skipped, "broken").await, ["invalid Broken"]);
    assert_eq!(lib.found_by(TriageTab::Skipped, "tagged title").await, ["invalid Tagged"]);
}

impl ImportTables {
    /// The plans each part of `list`'s windows is read by, by offset and by
    /// each seek from a row, for a test that holds every order to its index.
    async fn import_list_window_plans(
        &self,
        list: ImportListId,
    ) -> Result<Vec<Vec<Vec<String>>>, DbError> {
        self.store
            .read(move |sql| {
                let row = list_row(&sql, list)?;
                row.items(Shown::View).window_plans(&sql)
            })
            .await
    }

    /// The plans a searching list's counts and its windows read from the
    /// rows its search finds are read by, one line per step, the counts
    /// first: what a test holds them to the search index with.
    async fn import_list_search_plans(
        &self,
        list: ImportListId,
    ) -> Result<Vec<Vec<String>>, DbError> {
        self.store
            .read(move |sql| {
                let row = list_row(&sql, list)?;
                let counts = sql.query(
                    &format!("EXPLAIN QUERY PLAN {}", summary::SEARCH_COUNTS),
                    named_params! { ":list": list.0 },
                    |row| row.get::<_, String>(3),
                )?;
                let mut plans = vec![counts];
                plans.extend(row.items(Shown::Found).window_plans(&sql)?.concat());
                Ok(plans)
            })
            .await
    }
}

/// Every tab's window in every order is read off that order's index, in its
/// order, by its offset and from an item beside it: no sort of the tab's
/// rows, so a window costs about its offset and limit whatever the tab holds,
/// and one read from an item beside it about its limit.
#[tokio::test]
async fn every_order_reads_its_windows_off_its_index() {
    let lib = Library::new().await;
    let tables = crate::tables::ImportTables::new(lib.db.clone());
    lib.run(lib.scan("Box/1") + lib.scan("Loose")).await;
    for (order, index) in [
        (ImportListOrder::NewestFirst, "import_list_newest_first"),
        (ImportListOrder::OldestFirst, "import_list_oldest_first"),
        (ImportListOrder::PathAscending, "import_list_path_ascending"),
        (ImportListOrder::PathDescending, "import_list_path_descending"),
    ] {
        for tab in [TriageTab::Pending, TriageTab::Done, TriageTab::Skipped] {
            let list = lib.open(ordered(tab, order)).await;
            let plans = tables.import_list_window_plans(list).await.unwrap();
            // Done's last part is its settled releases, read off the index;
            // its first, the handful still uploading, is sorted by queue.
            // Each part is read by its offset, off its order's index, and
            // from an item beside a window by each seek, which searches an
            // order index; one holding every term but the key equal may take
            // any of them, since each holds those terms.
            let (by_offset, seeks) = plans
                .last()
                .expect("a tab reads in at least one part")
                .split_first()
                .expect("a part is read by its offset");
            assert!(
                by_offset.iter().any(|step| step.contains(index)),
                "{order:?} {tab:?}: {by_offset:?}"
            );
            for plan in std::iter::once(by_offset).chain(seeks) {
                assert!(
                    plan.iter().any(|step| step.starts_with("SEARCH")
                        && step.contains("USING INDEX import_list_")),
                    "{order:?} {tab:?}: {plan:?}"
                );
                assert!(
                    !plan.iter().any(|step| step.contains("TEMP B-TREE FOR ORDER BY")),
                    "{order:?} {tab:?} sorts: {plan:?}"
                );
            }
        }
    }
}

/// A search's counts and its windows read from the rows it finds reach them
/// through the search index, each row by its number: none reads the tab's
/// rows to test them.
#[tokio::test]
async fn a_search_reads_the_rows_its_index_finds() {
    let lib = Library::new().await;
    let tables = crate::tables::ImportTables::new(lib.db.clone());
    lib.run(lib.scan("Box/Wanted") + lib.scan("Loose")).await;
    for tab in [TriageTab::Pending, TriageTab::Done, TriageTab::Skipped] {
        let list =
            lib.open(ImportListView { filter_text: "wanted".to_string(), ..view(tab) }).await;
        for plan in tables.import_list_search_plans(list).await.unwrap() {
            assert!(
                plan.iter().any(|step| step.contains("VIRTUAL TABLE INDEX") && step.contains(":G")),
                "{tab:?}: {plan:?}"
            );
            assert!(
                !plan.iter().any(|step| step.contains("(tab=?)")),
                "{tab:?} reads its tab: {plan:?}"
            );
        }
    }
}

/// Found reads each order in runs off that order's indexes, in its order, by
/// a window's offset and from an item beside it, folding groups shut or not:
/// the headers and the rows outside every group, and each open group's
/// members in their header's place, with no sort and no folded member passed,
/// so a window costs about its offset and limit however many members are
/// folded, and one read from an item beside it about its limit.
#[tokio::test]
async fn found_reads_its_windows_off_its_runs() {
    let lib = Library::new().await;
    let tables = crate::tables::ImportTables::new(lib.db.clone());
    lib.run(lib.scan("Box/1") + lib.scan("Other/1") + lib.scan("Loose")).await;
    for (order, index) in [
        (ImportListOrder::NewestFirst, "import_list_newest_first"),
        (ImportListOrder::OldestFirst, "import_list_oldest_first"),
        (ImportListOrder::PathAscending, "import_list_path_ascending"),
        (ImportListOrder::PathDescending, "import_list_path_descending"),
    ] {
        for folded in [false, true] {
            let list = lib.open(ordered(TriageTab::Pending, order)).await;
            tables.set_import_list_group_folded(list, lib.group_key("Box"), folded).await.unwrap();
            // Found runs in one part, read by its offset off the three runs'
            // indexes, and by each seek from its first item, which searches
            // them, or reaches the row it holds every term up to the folder
            // and its number equal to by that number.
            let plans = tables.import_list_window_plans(list).await.unwrap().concat();
            let (by_offset, seeks) = plans.split_first().expect("a part is read by its offset");
            assert!(!seeks.is_empty(), "{order:?} folded {folded}: {plans:?}");
            for run in ["_unfolded", "_headers", "_members"] {
                assert!(
                    by_offset.iter().any(|step| step.contains(&format!("{index}{run} "))),
                    "{order:?} folded {folded} reads {run}: {by_offset:?}"
                );
            }
            for plan in &plans {
                assert!(
                    !plan.iter().any(|step| step.starts_with("SCAN")),
                    "{order:?} folded {folded} scans: {plan:?}"
                );
                assert!(
                    plan.iter().any(|step| step.contains("MERGE")),
                    "{order:?} folded {folded}: {plan:?}"
                );
                assert!(
                    !plan.iter().any(|step| step.contains("TEMP B-TREE")),
                    "{order:?} folded {folded} sorts: {plan:?}"
                );
            }
        }
    }
}
