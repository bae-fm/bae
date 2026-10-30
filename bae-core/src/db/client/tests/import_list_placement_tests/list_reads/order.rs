//! Which items a list shows and in what order: its roots, its natural path
//! order, its dates, its groups and their headers, folds, and windows.

use super::*;
use crate::import::FolderReleaseDecisionKey;

impl Library {
    /// A second watched folder, `root`, at `position`, with the settled
    /// release `name` directly in it.
    fn second_root_release(&self, root: &str, position: i64, name: &str) -> String {
        let path = super::super::super::folder_below(root, name)
            .to_string_lossy()
            .into_owned();
        format!(
            "{}INSERT INTO scan_candidate
                 (watched_folder_path, path, generation, kind, name, display_path, folder,
                  file_root, scope, content_hash)
             VALUES ('{root}', '{path}', 1, 'valid', '{name}', '{name}', '{path}', '{path}',
                     'direct', 'hash-{root}-{name}');",
            self.watch(root, position)
        )
    }

    /// The group at `name` under the watched folder.
    pub(super) fn group_key(&self, name: &str) -> FolderReleaseDecisionKey {
        FolderReleaseDecisionKey {
            watched_folder_path: self.root.clone(),
            relative_folder_path: name.to_string(),
        }
    }

    /// The scan's releases at `display_paths`, each found at its date.
    fn dated(&self, releases: &[(&str, i64)]) -> String {
        releases
            .iter()
            .map(|(display_path, at)| {
                format!(
                    "{}{}",
                    self.scan(display_path),
                    self.discovered(display_path, *at)
                )
            })
            .collect()
    }
}

#[tokio::test]
async fn rows_order_by_watched_root_then_natural_path() {
    let lib = Library::new().await;
    let second = crate::import::watched_folder::host_root("/second");
    lib.run(&format!(
        "{}{}{}",
        lib.scan("Release 10"),
        lib.scan("Release 2"),
        lib.second_root_release(&second, 1, "Release 1")
    ))
    .await;
    assert_eq!(
        lib.items(view(TriageTab::Pending)).await,
        [
            "candidate Release 2",
            "candidate Release 10",
            "candidate Release 1"
        ],
        "natural order within a root, and the roots in their stored order"
    );
}

#[tokio::test]
async fn the_descending_order_reverses_the_whole_queue() {
    let lib = Library::new().await;
    lib.run(&format!(
        "{}{}",
        lib.scan("Release 2"),
        lib.scan("Release 10")
    ))
    .await;
    assert_eq!(
        lib.items(ordered(TriageTab::Pending, ImportListOrder::PathDescending))
            .await,
        ["candidate Release 10", "candidate Release 2"]
    );
}

/// A tentative candidate is a release the scan found before it knew what
/// enclosed it: not a row, not a count, and not a group.
#[tokio::test]
async fn a_tentative_candidate_is_neither_a_row_nor_a_count() {
    let lib = Library::new().await;
    lib.run(&lib.scan_as("Box/CD1", "tentative")).await;
    let read = lib.shown(view(TriageTab::Pending)).await;
    assert!(sequence(&read).is_empty());
    assert_eq!(read.total_count, 0);
    assert_eq!(
        read.summary.counts,
        crate::import::TriageTabCounts::default()
    );
    assert!(!read.summary.has_groups);
}

#[tokio::test]
async fn a_release_counts_pending_and_an_invalid_folder_counts_skipped() {
    let lib = Library::new().await;
    lib.run(&format!(
        "{}{}",
        lib.scan("Release"),
        lib.scan_as("Broken", "invalid")
    ))
    .await;
    let pending = lib.shown(view(TriageTab::Pending)).await;
    assert_eq!(
        pending.summary.counts,
        crate::import::TriageTabCounts {
            pending: 1,
            done: 0,
            skipped: 1,
        }
    );
    assert_eq!(sequence(&pending), ["candidate Release"]);
    assert_eq!(
        lib.items(view(TriageTab::Skipped)).await,
        ["invalid Broken"]
    );
}

#[tokio::test]
async fn a_collapsed_group_keeps_its_header_and_drops_its_entries() {
    let lib = Library::new().await;
    lib.run(&format!(
        "{}{}",
        lib.scan("Group/Release 1"),
        lib.scan("Group/Release 2")
    ))
    .await;
    let list = lib.open(view(TriageTab::Pending)).await;
    lib.db
        .set_import_list_group_folded(list, lib.group_key("Group"), true)
        .await
        .unwrap();
    let read = lib.read(list).await;
    assert_eq!(sequence(&read), ["group Group"]);
    assert_eq!(read.total_count, 1);
    let ImportListItem::GroupHeader {
        expanded,
        entry_count,
        ..
    } = &read.windows[0].items[0]
    else {
        panic!("the header is the one item");
    };
    assert!(!expanded);
    assert_eq!(
        *entry_count, 2,
        "the header still says how much is folded away"
    );

    lib.db
        .set_import_list_group_folded(list, lib.group_key("Group"), false)
        .await
        .unwrap();
    assert_eq!(
        sequence(&lib.read(list).await),
        [
            "group Group",
            "candidate Group/Release 1",
            "candidate Group/Release 2"
        ]
    );
}

/// Folding every group is one write, and so is opening them all again.
#[tokio::test]
async fn every_group_folds_and_opens_at_once() {
    let lib = Library::new().await;
    lib.run(&format!(
        "{}{}{}",
        lib.scan("A/One"),
        lib.scan("B/Two"),
        lib.scan("Loose")
    ))
    .await;
    let list = lib
        .open(ordered(TriageTab::Pending, ImportListOrder::PathAscending))
        .await;
    let folded = lib
        .db
        .set_import_list_groups_folded(list, true)
        .await
        .unwrap();
    let read = lib.read(list).await;
    assert_eq!(sequence(&read), ["group A", "group B", "candidate Loose"]);
    assert_eq!(read.view_revision, folded);
    let opened = lib
        .db
        .set_import_list_groups_folded(list, false)
        .await
        .unwrap();
    assert_eq!(opened, folded + 1, "each fold moves the view's revision");
    assert_eq!(
        sequence(&lib.read(list).await),
        [
            "group A",
            "candidate A/One",
            "group B",
            "candidate B/Two",
            "candidate Loose"
        ]
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
            ImportListItem::Candidate {
                header_position, ..
            } => *header_position,
            ImportListItem::GroupHeader { .. } | ImportListItem::Invalid { .. } => None,
        })
        .collect()
}

/// A release in a group names where its group's header sits in the list,
/// and one in no group names none.
#[tokio::test]
async fn a_group_member_names_its_headers_position() {
    let lib = Library::new().await;
    lib.run(&format!(
        "{}{}{}{}",
        lib.scan("A/One"),
        lib.scan("A/Two"),
        lib.scan("B/Three"),
        lib.scan("Loose")
    ))
    .await;
    let read = lib
        .shown(ordered(TriageTab::Pending, ImportListOrder::PathAscending))
        .await;
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
    assert_eq!(
        header_positions(&read),
        [None, Some(0), Some(0), None, Some(3), None]
    );
}

/// A window that starts among a group's members names the header before it,
/// which it does not hold: the header the rows on top are listed under,
/// whichever windows a surface has loaded.
#[tokio::test]
async fn a_window_starting_inside_a_group_names_the_header_before_it() {
    let lib = Library::new().await;
    lib.run(&format!(
        "{}{}{}{}",
        lib.scan("A/Disc 1"),
        lib.scan("A/Disc 2"),
        lib.scan("A/Disc 3"),
        lib.scan("Loose")
    ))
    .await;
    let list = lib
        .open(ordered(TriageTab::Pending, ImportListOrder::PathAscending))
        .await;
    let read = lib
        .read_through(
            list,
            LibraryPageWindow {
                offset: 2,
                limit: 3,
            },
        )
        .await;
    assert_eq!(
        sequence(&read),
        [
            "candidate A/Disc 2",
            "candidate A/Disc 3",
            "candidate Loose"
        ]
    );
    assert_eq!(header_positions(&read), [Some(0), Some(0), None]);
}

/// A list that folds a group shut reads its windows past the folded members,
/// and an open group's members still name where their header sits: its place
/// among the items the list shows, the folded group's header counted and its
/// members not.
#[tokio::test]
async fn a_folded_lists_members_name_their_headers_position() {
    let lib = Library::new().await;
    lib.run(&format!(
        "{}{}{}{}{}{}",
        lib.scan("A/Disc 1"),
        lib.scan("A/Disc 2"),
        lib.scan("B/Disc 1"),
        lib.scan("B/Disc 2"),
        lib.scan("B/Disc 3"),
        lib.scan("Loose")
    ))
    .await;
    let list = lib
        .open(ordered(TriageTab::Pending, ImportListOrder::PathAscending))
        .await;
    lib.db
        .set_import_list_group_folded(list, lib.group_key("A"), true)
        .await
        .unwrap();
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
    assert_eq!(
        header_positions(&read),
        [None, None, Some(1), Some(1), Some(1), None]
    );
    let window = lib
        .read_through(
            list,
            LibraryPageWindow {
                offset: 3,
                limit: 3,
            },
        )
        .await;
    assert_eq!(
        sequence(&window),
        [
            "candidate B/Disc 2",
            "candidate B/Disc 3",
            "candidate Loose"
        ]
    );
    assert_eq!(header_positions(&window), [Some(1), Some(1), None]);
}

/// A group header asks how the folder under it is read, and offers to read it
/// the other way. Neither question survives the import, so Done and Skipped are
/// flat lists of releases and only Pending groups.
#[tokio::test]
async fn only_pending_groups_its_rows() {
    let lib = Library::new().await;
    lib.run(&format!(
        "{}{}{}{}",
        scanned_with_states(&lib, &["Group/Release 1", "Group/Release 2"]),
        lib.imported("Group/Release 1", "2026-01-02T00:00:00Z"),
        lib.skip("Group/Release 2"),
        ""
    ))
    .await;
    let pending = lib.shown(view(TriageTab::Pending)).await;
    assert!(sequence(&pending).is_empty(), "both rows left Pending");
    assert!(!pending.summary.has_groups);
    assert_eq!(
        lib.items(view(TriageTab::Done)).await,
        ["candidate Group/Release 1"]
    );
    assert_eq!(
        lib.items(view(TriageTab::Skipped)).await,
        ["candidate Group/Release 2"]
    );
}

/// Done is ordered by when the import happened, newest first, then by path.
#[tokio::test]
async fn done_orders_by_newest_import_first() {
    let lib = Library::new().await;
    lib.run(&format!(
        "{}{}{}{}",
        scanned_with_states(&lib, &["A old", "B new", "C middle"]),
        lib.imported("A old", "2026-01-01T00:00:00Z"),
        lib.imported("B new", "2026-01-03T00:00:00.250+00:00"),
        lib.imported("C middle", "2026-01-02T00:00:00Z"),
    ))
    .await;
    assert_eq!(
        lib.items(view(TriageTab::Done)).await,
        ["candidate B new", "candidate C middle", "candidate A old"]
    );
    assert_eq!(
        lib.items(ordered(TriageTab::Done, ImportListOrder::OldestFirst))
            .await,
        ["candidate A old", "candidate C middle", "candidate B new"]
    );
}

#[tokio::test]
async fn default_is_newest_first_and_groups_use_their_newest_member() {
    let lib = Library::new().await;
    lib.run(&format!(
        "{}{}",
        lib.dated(&[
            ("A/Old", 10),
            ("B/Recent", 30),
            ("A/New", 50),
            ("Single", 40)
        ]),
        lib.scan("Unknown")
    ))
    .await;
    assert_eq!(
        ImportListView::default().order,
        ImportListOrder::NewestFirst
    );
    assert_eq!(
        lib.items(view(TriageTab::Pending)).await,
        [
            "group A",
            "candidate A/New",
            "candidate A/Old",
            "candidate Single",
            "group B",
            "candidate B/Recent",
            "candidate Unknown",
        ]
    );
    assert_eq!(
        lib.items(ordered(TriageTab::Pending, ImportListOrder::OldestFirst))
            .await,
        [
            "group B",
            "candidate B/Recent",
            "candidate Single",
            "group A",
            "candidate A/Old",
            "candidate A/New",
            "candidate Unknown",
        ]
    );
    let list = lib.open(view(TriageTab::Pending)).await;
    let location = lib
        .db
        .locate_import_candidate(list, &lib.path("Single"))
        .await
        .unwrap()
        .unwrap();
    assert_eq!(location.visible_position, 3);
}

#[tokio::test]
async fn dates_cross_watched_roots_and_ties_have_a_stable_natural_path_order() {
    let lib = Library::new().await;
    let second = crate::import::watched_folder::host_root("/second");
    lib.run(&format!(
        "{}{}INSERT INTO folder_discovery (folder, first_seen_at, settled)
             VALUES ('{}', 20, 1);",
        lib.dated(&[("Release 10", 10), ("Release 2", 10)]),
        lib.second_root_release(&second, 1, "Newest"),
        super::super::super::folder_below(&second, "Newest").to_string_lossy(),
    ))
    .await;
    assert_eq!(
        lib.items(view(TriageTab::Pending)).await,
        [
            "candidate Newest",
            "candidate Release 2",
            "candidate Release 10"
        ]
    );
}

#[tokio::test]
async fn name_orders_ignore_dates_and_case_ties_do_not_split_groups() {
    let lib = Library::new().await;
    lib.run(&lib.dated(&[("Box/1", 10), ("box/2", 30), ("Box/3", 20)]))
        .await;
    assert_eq!(
        lib.items(ordered(TriageTab::Pending, ImportListOrder::PathAscending))
            .await,
        [
            "group Box",
            "candidate Box/1",
            "candidate Box/3",
            "group box",
            "candidate box/2"
        ]
    );
    assert_eq!(
        lib.items(ordered(TriageTab::Pending, ImportListOrder::PathDescending))
            .await,
        [
            "group box",
            "candidate box/2",
            "group Box",
            "candidate Box/3",
            "candidate Box/1"
        ]
    );
}

#[tokio::test]
async fn filtering_collapsing_and_paging_preserve_group_date_order() {
    let lib = Library::new().await;
    lib.run(&lib.dated(&[("A/Old", 10), ("B/Old", 20), ("A/New", 30)]))
        .await;
    let filtered = ImportListView {
        filter_text: "Old".into(),
        ..view(TriageTab::Pending)
    };
    let list = lib.open(filtered).await;
    assert_eq!(
        sequence(&lib.read(list).await),
        ["group A", "candidate A/Old", "group B", "candidate B/Old"]
    );
    assert_eq!(
        sequence(
            &lib.read_through(
                list,
                LibraryPageWindow {
                    offset: 1,
                    limit: 2
                }
            )
            .await
        ),
        ["candidate A/Old", "group B"]
    );
    lib.db
        .set_import_list_group_folded(list, lib.group_key("A"), true)
        .await
        .unwrap();
    assert_eq!(
        sequence(&lib.read(list).await),
        ["group A", "group B", "candidate B/Old"]
    );
}

/// A window past the end holds what is left of the list, and one wholly past
/// it holds nothing.
#[tokio::test]
async fn a_window_past_the_end_holds_what_is_left() {
    let lib = Library::new().await;
    lib.run(&format!(
        "{}{}{}",
        lib.scan("A"),
        lib.scan("B"),
        lib.scan("C")
    ))
    .await;
    let list = lib
        .open(ordered(TriageTab::Pending, ImportListOrder::PathAscending))
        .await;
    let read = lib
        .read_through(
            list,
            LibraryPageWindow {
                offset: 2,
                limit: 5,
            },
        )
        .await;
    assert_eq!(sequence(&read), ["candidate C"]);
    assert_eq!(read.total_count, 3);
    assert!(sequence(
        &lib.read_through(
            list,
            LibraryPageWindow {
                offset: 5,
                limit: 5
            }
        )
        .await
    )
    .is_empty());
}

/// A group header is where a folder read as several releases offers to be read
/// as one. A header that is only a path component the rows share offers
/// nothing: the folder that holds them — `Singles/Label`, the nearest one
/// nothing is stored for — is where that choice belongs, not every folder
/// above it.
#[tokio::test]
async fn only_a_group_over_a_folder_read_as_several_offers_to_combine() {
    let lib = Library::new().await;
    lib.run(&format!(
        "{}{}{}{}{}{}",
        lib.scan("Box/Disc 1"),
        lib.scan("Box/Disc 2"),
        lib.scan("Singles/Label/Series/One"),
        lib.scan("Singles/Label/Series/Two"),
        lib.kept_apart("Box"),
        lib.kept_apart("Singles/Label/Series"),
    ))
    .await;
    let read = lib.shown(view(TriageTab::Pending)).await;
    assert_eq!(
        combinable(&read),
        [("Box".to_string(), true), ("Singles".to_string(), false)]
    );
}

/// A folder nothing has settled either way, holding several releases below a
/// folder that keeps them apart, is combinable too — it is the nearest folder
/// above them to hold them all, and the header for it is where the choice
/// belongs.
#[tokio::test]
async fn a_folder_nothing_settled_over_several_releases_offers_to_combine() {
    let lib = Library::new().await;
    lib.run(&format!(
        "{}{}{}",
        lib.scan("Wrapper/Box/Disc 1"),
        lib.scan("Wrapper/Box/Disc 2"),
        lib.kept_apart("Wrapper/Box"),
    ))
    .await;
    let read = lib.shown(view(TriageTab::Pending)).await;
    assert_eq!(combinable(&read), [("Wrapper".to_string(), true)]);
}

/// Whether a header offers to combine follows every write that decides it: a
/// reading stored, changed or removed at its folder or at one inside it, and
/// a release of any tab placed below it or gone.
#[tokio::test]
async fn a_header_offers_to_combine_as_its_readings_and_rows_change() {
    let lib = Library::new().await;
    let offers = || async { combinable(&lib.shown(view(TriageTab::Pending)).await) };
    let singles = |offered: bool| [("Singles".to_string(), offered)];
    lib.run(&format!(
        "{}{}",
        lib.scan("Singles/Label/Series/One"),
        lib.scan("Singles/Label/Series/Two")
    ))
    .await;
    assert_eq!(offers().await, singles(false), "Series is nearer to both");
    lib.run(&format!(
        "{}{}",
        lib.kept_apart("Singles/Label"),
        lib.kept_apart("Singles/Label/Series")
    ))
    .await;
    assert_eq!(
        offers().await,
        singles(true),
        "every folder between is settled"
    );
    lib.run("DELETE FROM release_grouping WHERE key = 'reading Singles/Label';")
        .await;
    assert_eq!(offers().await, singles(false), "Label is nearer again");
    lib.run(&lib.kept_apart("Singles")).await;
    assert_eq!(offers().await, singles(true), "Singles keeps them apart");
    lib.run("UPDATE release_grouping SET combined = 1 WHERE key = 'reading Singles';")
        .await;
    assert_eq!(offers().await, singles(false), "Singles is read as one");
    lib.run("DELETE FROM release_grouping WHERE key = 'reading Singles';")
        .await;
    lib.run(&format!(
        "{}{}{}",
        lib.scan("Singles/Loose"),
        lib.state("Singles/Loose"),
        lib.imported("Singles/Loose", "2026-01-02T00:00:00Z")
    ))
    .await;
    assert_eq!(
        offers().await,
        singles(true),
        "a release on Done right below Singles is nearest to it"
    );
    lib.run(&format!(
        "DELETE FROM scan_candidate WHERE path = '{}';",
        lib.path("Singles/Loose")
    ))
    .await;
    assert_eq!(offers().await, singles(false), "and gone, it is not");
}

/// Each header's group name and whether it offers to combine, in order.
fn combinable(read: &ImportListProjection) -> Vec<(String, bool)> {
    read.windows[0]
        .items
        .iter()
        .filter_map(|item| match item {
            ImportListItem::GroupHeader { group, .. } => {
                Some((group.name.clone(), group.combinable))
            }
            _ => None,
        })
        .collect()
}

impl Library {
    /// The stored reading of the folder at `display_path` that keeps its
    /// releases apart.
    fn kept_apart(&self, display_path: &str) -> String {
        format!(
            "INSERT INTO release_grouping (key, anchor_folder, combined, author)
             VALUES ('reading {display_path}', '{}', 0, 'user');",
            self.path(display_path)
        )
    }
}

/// Every tab's window in every order is read off that order's index, in its
/// order, by its offset and from an item beside it: no sort of the tab's
/// rows, so a window costs about its offset and limit whatever the tab holds,
/// and one read from an item beside it about its limit.
#[tokio::test]
async fn every_order_reads_its_windows_off_its_index() {
    let lib = Library::new().await;
    lib.run(&format!("{}{}", lib.scan("Box/1"), lib.scan("Loose")))
        .await;
    for (order, index) in [
        (ImportListOrder::NewestFirst, "import_list_newest_first"),
        (ImportListOrder::OldestFirst, "import_list_oldest_first"),
        (ImportListOrder::PathAscending, "import_list_path_ascending"),
        (
            ImportListOrder::PathDescending,
            "import_list_path_descending",
        ),
    ] {
        for tab in [TriageTab::Pending, TriageTab::Done, TriageTab::Skipped] {
            let list = lib.open(ordered(tab, order)).await;
            let plans = lib.db.import_list_window_plans(list).await.unwrap();
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
                    !plan
                        .iter()
                        .any(|step| step.contains("TEMP B-TREE FOR ORDER BY")),
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
    lib.run(&format!("{}{}", lib.scan("Box/Wanted"), lib.scan("Loose")))
        .await;
    for tab in [TriageTab::Pending, TriageTab::Done, TriageTab::Skipped] {
        let list = lib
            .open(ImportListView {
                filter_text: "wanted".to_string(),
                ..view(tab)
            })
            .await;
        for plan in lib.db.import_list_search_plans(list).await.unwrap() {
            assert!(
                plan.iter()
                    .any(|step| step.contains("VIRTUAL TABLE INDEX") && step.contains(":G")),
                "{tab:?}: {plan:?}"
            );
            assert!(
                !plan.iter().any(|step| step.contains("(tab=?)")),
                "{tab:?} reads its tab: {plan:?}"
            );
        }
    }
}

/// A list folding groups shut reads each order in runs off that order's
/// indexes, in its order: the headers and the rows outside every group, and
/// each open group's members, with no sort and no folded member passed, so a
/// window costs about its offset and limit however many members are folded.
#[tokio::test]
async fn a_folded_list_reads_its_windows_off_its_runs() {
    let lib = Library::new().await;
    lib.run(&format!(
        "{}{}{}",
        lib.scan("Box/1"),
        lib.scan("Other/1"),
        lib.scan("Loose")
    ))
    .await;
    for (order, index) in [
        (ImportListOrder::NewestFirst, "import_list_newest_first"),
        (ImportListOrder::OldestFirst, "import_list_oldest_first"),
        (ImportListOrder::PathAscending, "import_list_path_ascending"),
        (
            ImportListOrder::PathDescending,
            "import_list_path_descending",
        ),
    ] {
        let list = lib.open(ordered(TriageTab::Pending, order)).await;
        lib.db
            .set_import_list_group_folded(list, lib.group_key("Box"), true)
            .await
            .unwrap();
        let plans = lib.db.import_list_folded_plans(list).await.unwrap();
        let plan = plans.concat();
        for run in ["_unfolded", "_headers", "_members"] {
            assert!(
                plan.iter()
                    .any(|step| step.contains(&format!("{index}{run} "))),
                "{order:?} reads {run}: {plan:?}"
            );
        }
        assert!(
            plan.iter().any(|step| step.contains("MERGE")),
            "{order:?}: {plan:?}"
        );
        assert!(
            !plan.iter().any(|step| step.contains("TEMP B-TREE")),
            "{order:?} sorts: {plan:?}"
        );
    }
}
