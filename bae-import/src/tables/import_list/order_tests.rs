//! Which items a list shows and in what order: its roots, its natural path
//! order, its dates, its groups and their headers, folds, windows, what its
//! text filter finds, and the indexes each order reads off.

use super::*;
use crate::list::ImportListRead;
use crate::tables::import_list::tests::*;
use crate::ImportListItem;
use crate::ImportListView;
use crate::{FolderReleaseDecisionKey, ImportListFolds};
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
    lib.run(lib.scan("Group/Release 1") + lib.scan("Group/Release 2")).await;
    let list = ImportListRequest::of(view(TriageTab::Pending));
    let list = list.folding(lib.group_key("Group"));
    let read = lib.read(&list).await;
    assert_eq!(sequence(&read), ["group Group"]);
    assert_eq!(read.count.rows, 1);
    let ImportListItem::GroupHeader(header) = &read.windows[0].rows[0].0 else {
        panic!("the header is the one item");
    };
    assert!(!header.expanded);
    assert_eq!(header.entry_count, 2, "the header still says how much is folded away");
    assert!(header.shows_no_member(), "the folded header is its group's last row");

    let list = list.unfolding(&lib.group_key("Group"));
    assert_eq!(
        sequence(&lib.read(&list).await),
        ["group Group", "candidate Group/Release 1", "candidate Group/Release 2"]
    );
}

/// A group folded in the request that a rescan takes out of the list and
/// brings back under the same folder comes back folded: the request names
/// the group by its folder, not by a row the rescan replaced.
#[tokio::test]
async fn a_folded_group_a_rescan_brings_back_comes_back_folded() {
    let lib = Library::new().await;
    lib.run(lib.scan("Group/Release 1") + lib.scan("Group/Release 2") + lib.scan("Loose")).await;
    let list = ImportListRequest::of(view(TriageTab::Pending)).folding(lib.group_key("Group"));
    assert_eq!(sequence(&lib.read(&list).await), ["group Group", "candidate Loose"]);

    lib.run(lib.unscan("Group/Release 1")).await;
    lib.run(lib.unscan("Group/Release 2")).await;
    assert_eq!(sequence(&lib.read(&list).await), ["candidate Loose"], "the group left the list");

    lib.run(lib.scan("Group/Release 1") + lib.scan("Group/Release 2")).await;
    assert_eq!(
        sequence(&lib.read(&list).await),
        ["group Group", "candidate Loose"],
        "the group came back folded"
    );
}

/// Folding every group folds the groups Found gains later too, and a group
/// the person opens after that stays open.
#[tokio::test]
async fn folding_every_group_folds_the_groups_found_gains_later() {
    let lib = Library::new().await;
    lib.run(lib.scan("Box/Release 1") + lib.scan("Box/Release 2") + lib.scan("Loose")).await;
    let list = ImportListRequest::of(view(TriageTab::Pending)).folding_all();
    assert_eq!(sequence(&lib.read(&list).await), ["group Box", "candidate Loose"]);

    lib.run(lib.scan("Crate/Release 1") + lib.scan("Crate/Release 2")).await;
    assert_eq!(
        sequence(&lib.read(&list).await),
        ["group Box", "group Crate", "candidate Loose"],
        "the new group arrives folded"
    );

    let list = list.unfolding(&lib.group_key("Crate"));
    assert_eq!(
        sequence(&lib.read(&list).await),
        [
            "group Box",
            "group Crate",
            "candidate Crate/Release 1",
            "candidate Crate/Release 2",
            "candidate Loose"
        ]
    );
}

/// The key of the header each release is listed under, in a read's order:
/// `None` for a release in no group, and for each header.
fn header_keys(items: &[ImportListItem]) -> Vec<Option<String>> {
    items
        .iter()
        .map(|item| match item {
            ImportListItem::Candidate { group, .. } => {
                group.as_ref().map(|group| group.header.stable_key())
            }
            ImportListItem::GroupHeader(_) | ImportListItem::Invalid { .. } => None,
        })
        .collect()
}

/// The header each release is listed under, in a read's order: `None` for a
/// release in no group, and for each header.
fn headers(items: &[ImportListItem]) -> Vec<Option<crate::ImportGroupHeader>> {
    items
        .iter()
        .map(|item| match item {
            ImportListItem::Candidate { group, .. } => {
                group.as_ref().map(|group| group.header.clone())
            }
            ImportListItem::GroupHeader(_) | ImportListItem::Invalid { .. } => None,
        })
        .collect()
}

/// The stable key of the header of the group at `name`.
fn header_of(lib: &Library, name: &str) -> Option<String> {
    Some(crate::list::group_stable_key(&lib.group_key(name)))
}

fn items_of(read: &ImportListRead) -> Vec<ImportListItem> {
    read.windows.iter().flat_map(|window| unkeyed(&window.rows)).collect()
}

/// A release in a group carries its group's header, the header's own row as
/// the list shows it, and one in no group carries none.
#[tokio::test]
async fn a_group_member_carries_its_header() {
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
    let (a, b) = (header_of(&lib, "A"), header_of(&lib, "B"));
    assert_eq!(header_keys(&items_of(&read)), [None, a.clone(), a, None, b, None]);
    let ImportListItem::GroupHeader(header) = &items_of(&read)[0] else {
        panic!("a header leads");
    };
    assert_eq!(
        headers(&items_of(&read))[1].as_ref(),
        Some(header),
        "a member carries its header as the header's own row shows it ({})",
        header.group.name
    );
}

/// A window that starts among a group's members carries the header with
/// each of them, though it does not hold the header's own row: the header
/// the rows on top are listed under, whichever window a surface holds.
#[tokio::test]
async fn a_window_starting_inside_a_group_carries_its_header() {
    let lib = Library::new().await;
    lib.run(lib.scan("A/Disc 1") + lib.scan("A/Disc 2") + lib.scan("A/Disc 3") + lib.scan("Loose"))
        .await;
    let list = ImportListRequest::of(ordered(TriageTab::Pending, ImportListOrder::PathAscending));
    let window = lib.window_at(&list, 2, 3).await;
    assert_eq!(
        items_sequence(&unkeyed(&window.rows)),
        ["candidate A/Disc 2", "candidate A/Disc 3", "candidate Loose"]
    );
    let a = header_of(&lib, "A");
    assert_eq!(header_keys(&unkeyed(&window.rows)), [a.clone(), a.clone(), None]);
    let header = headers(&unkeyed(&window.rows))[0].clone().expect("a member carries its header");
    assert_eq!(header.group.name, "A");
    assert_eq!(header.entry_count, 3, "the header counts the group's members");

    let top = lib.window_at(&list, 0, 2).await;
    let ImportListItem::GroupHeader(own) = &unkeyed(&top.rows)[0] else {
        panic!("the list's top is the group's header");
    };
    assert_eq!(own, &header, "a member's header is the header's own row");
}

/// A list that folds a group shut reads its windows past the folded members,
/// and an open group's members still name their header.
#[tokio::test]
async fn a_folded_lists_members_name_their_header() {
    let lib = Library::new().await;
    lib.run(
        lib.scan("A/Disc 1")
            + lib.scan("A/Disc 2")
            + lib.scan("B/Disc 1")
            + lib.scan("B/Disc 2")
            + lib.scan("B/Disc 3")
            + lib.scan("Loose"),
    )
    .await;
    let list = ImportListRequest::of(ordered(TriageTab::Pending, ImportListOrder::PathAscending));
    let list = list.folding(lib.group_key("A"));
    let read = lib.read(&list).await;
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
    let b = header_of(&lib, "B");
    assert_eq!(header_keys(&items_of(&read)), [None, None, b.clone(), b.clone(), b.clone(), None]);
    let window = lib.window_at(&list, 3, 3).await;
    assert_eq!(
        items_sequence(&unkeyed(&window.rows)),
        ["candidate B/Disc 2", "candidate B/Disc 3", "candidate Loose"]
    );
    assert_eq!(header_keys(&unkeyed(&window.rows)), [b.clone(), b, None]);
}

/// Whether each release ends the group it is listed under, in a read's
/// order: `None` for a release in no group, and for each header.
fn group_ends(items: &[ImportListItem]) -> Vec<Option<bool>> {
    items
        .iter()
        .map(|item| match item {
            ImportListItem::Candidate { group, .. } => group.as_ref().map(|group| group.last),
            ImportListItem::GroupHeader(_) | ImportListItem::Invalid { .. } => None,
        })
        .collect()
}

/// A group member says it ends its group when the list shows no member of
/// the group after it, in every order, and so does a window that holds the
/// member alone.
#[tokio::test]
async fn a_groups_last_member_says_it_ends_the_group() {
    let lib = Library::new().await;
    lib.run(
        lib.scan("A/One")
            + lib.scan("A/Two")
            + lib.scan("A/Three")
            + lib.scan("B/Four")
            + lib.scan("Loose"),
    )
    .await;
    for order in [
        ImportListOrder::NewestFirst,
        ImportListOrder::OldestFirst,
        ImportListOrder::PathAscending,
        ImportListOrder::PathDescending,
    ] {
        let list = ImportListRequest::of(ordered(TriageTab::Pending, order));
        let items = items_of(&lib.read(&list).await);
        let keys = header_keys(&items);
        // A member ends its group when the row after it is not a member of
        // the same group.
        let expected = (0..items.len())
            .map(|at| keys[at].as_ref().map(|key| keys.get(at + 1) != Some(&Some(key.clone()))))
            .collect::<Vec<_>>();
        assert_eq!(group_ends(&items), expected, "{order:?}");
        assert_eq!(expected.iter().filter(|ends| **ends == Some(true)).count(), 2, "{order:?}");
        for (at, ends) in (0..).zip(&expected) {
            let window = lib.window_at(&list, at, 1).await;
            assert_eq!(group_ends(&unkeyed(&window.rows)), [*ends], "{order:?} at {at}");
        }
    }
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
    let list = ImportListRequest::of(view(TriageTab::Pending));
    assert_eq!(
        tables.import_candidate_position(list.clone(), &lib.key("Single")).await.unwrap(),
        Some(4)
    );
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
    lib.run(lib.dated(&[("A/Old", 10), ("B/Old", 20), ("A/New", 30)])).await;
    let filtered =
        ImportListView { search: crate::SearchText::of("Old"), ..view(TriageTab::Pending) };
    let list = ImportListRequest::of(filtered);
    assert_eq!(
        sequence(&lib.read(&list).await),
        ["group A", "candidate A/Old", "group B", "candidate B/Old"]
    );
    assert_eq!(
        items_sequence(&unkeyed(&lib.window_at(&list, 1, 2).await.rows)),
        ["candidate A/Old", "group B"]
    );
    let list = list.folding(lib.group_key("A"));
    assert_eq!(sequence(&lib.read(&list).await), ["group A", "group B", "candidate B/Old"]);
}

/// Done shows two parts, the uploading releases and the rest, but bae still
/// reads the key at a position, the count before a key and a release's
/// position with one statement each.
#[tokio::test]
async fn positions_in_done_are_read_in_one_statement() {
    let lib = Library::new().await;
    lib.run(
        scanned_with_states(&lib, &["A", "B", "C"])
            + lib.imported("A", "2026-01-01T00:00:00Z")
            + lib.imported("B", "2026-01-02T00:00:00Z")
            + lib.imported("C", "2026-01-03T00:00:00Z"),
    )
    .await;
    let tables = lib.tables();
    let list = ImportListRequest::of(view(TriageTab::Done));
    let log = lib.db.record_statements_for_test().await.unwrap();
    let key = tables.import_key_at(list.clone(), 2).await.unwrap().expect("the list has items");
    assert_eq!(log.take().len(), 1, "the key at the position");
    let rows_before = tables.import_rows_before_for_test(list.clone(), key).await.unwrap();
    assert_eq!(rows_before, 2);
    assert_eq!(log.take().len(), 1, "the count before the key");
}

/// read_import_window reads a window in one statement, whatever the window's
/// rows hold: each member's group header
/// and each release's rows come in the same statement as the window's keys.
#[tokio::test]
async fn a_window_is_read_in_one_statement() {
    let lib = Library::new().await;
    lib.run(lib.scan("Group/One") + lib.scan("Group/Two") + lib.scan("Solo")).await;
    let tables = lib.tables();
    let list = ImportListRequest::of(ordered(TriageTab::Pending, ImportListOrder::PathAscending));
    let inside = tables.import_key_at(list.clone(), 1).await.unwrap().expect("the list has items");
    let log = lib.db.record_statements_for_test().await.unwrap();
    let window = tables
        .read_import_window(list.clone(), bae_store::ListWindow { from: Some(inside), limit: 1 })
        .await
        .unwrap();
    assert_eq!(log.take().len(), 1, "the window");
    assert_eq!(items_sequence(&unkeyed(&window.rows)), ["candidate Group/One"]);
    assert!(headers(&unkeyed(&window.rows))[0].is_some(), "the member carries its header");
}

/// A jump past the end starts at the last item, and a window from its key
/// holds that item alone.
#[tokio::test]
async fn a_jump_past_the_end_starts_at_the_last_item() {
    let lib = Library::new().await;
    lib.run(lib.scan("A") + lib.scan("B") + lib.scan("C")).await;
    let list = ImportListRequest::of(ordered(TriageTab::Pending, ImportListOrder::PathAscending));
    let window = lib.window_at(&list, 5, 5).await;
    assert_eq!(items_sequence(&unkeyed(&window.rows)), ["candidate C"]);
    assert_eq!(lib.read(&list).await.count.rows, 3);
    let empty = Library::new().await;
    let none = ImportListRequest::of(view(TriageTab::Pending));
    assert!(empty.tables().import_key_at(none, 0).await.unwrap().is_none());
}

impl Library {
    /// The items a list on `tab` shows with `needle` typed into its filter.
    async fn found_by(&self, tab: TriageTab, needle: &str) -> Vec<String> {
        self.items(ImportListView { search: crate::SearchText::of(needle), ..view(tab) }).await
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
    /// SQLite's plan for the statement revealing a candidate from `list`,
    /// one line per step.
    async fn import_reveal_plan(
        &self,
        list: ImportListRequest,
        key: crate::CandidateKey,
    ) -> Result<Vec<String>, DbError> {
        self.store
            .read(move |sql| {
                Items::new(list.clone()).query(
                    &sql,
                    &format!("EXPLAIN QUERY PLAN {}", summary::reveal_statement(&list)),
                    &[(":key", &key)],
                    |row| row.get::<_, String>(3),
                )
            })
            .await
    }

    /// The plans each part of `list`'s windows is read by, by offset and by
    /// each seek from a row, for a test that holds every order to its index.
    async fn import_list_window_plans(
        &self,
        list: ImportListRequest,
    ) -> Result<Vec<Vec<Vec<String>>>, DbError> {
        self.store.read(move |sql| Items::new(list).window_plans(&sql)).await
    }

    /// Returns SQLite's plan for the search counts query, one line per step,
    /// so a test can check that the query uses the search index.
    async fn import_list_search_plan(
        &self,
        list: ImportListRequest,
    ) -> Result<Vec<String>, DbError> {
        self.store
            .read(move |sql| {
                Items::new(list.clone()).query(
                    &sql,
                    &format!("EXPLAIN QUERY PLAN {}", summary::search_counts(&list)),
                    &[],
                    |row| row.get::<_, String>(3),
                )
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
            let list = ImportListRequest::of(ordered(tab, order));
            let plans = tables.import_list_window_plans(list.clone()).await.unwrap();
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

/// A reveal reads the candidate's placement row by its key first, then
/// counts the items before it off its tab's order indexes, with no temporary
/// B-tree: one read whose cost is the index ranges before the candidate,
/// however its list is folded.
#[tokio::test]
async fn a_reveal_reads_the_candidate_by_key_then_its_tabs_order_index() {
    let lib = Library::new().await;
    let tables = crate::tables::ImportTables::new(lib.db.clone());
    lib.run(lib.scan("Box/1") + lib.scan("Box/2") + lib.scan("Loose")).await;
    for order in [ImportListOrder::NewestFirst, ImportListOrder::PathAscending] {
        let list =
            ImportListRequest::of(ordered(TriageTab::Pending, order)).folding(lib.group_key("Box"));
        let plan = tables.import_reveal_plan(list, lib.key("Box/1")).await.unwrap();
        let first = plan
            .iter()
            .position(|step| step.starts_with("SEARCH") || step.starts_with("SCAN"))
            .expect("the reveal reads a table");
        assert!(
            plan[first].contains("import_list_placement_by_key (key=?)"),
            "{order:?} reads the candidate by its key first: {plan:?}"
        );
        // A seek that holds every term up to the folder equal may take any
        // order's index, since each holds those terms.
        assert!(
            plan.iter().any(|step| step.starts_with("SEARCH")
                && step.contains("USING INDEX import_list_")
                && step.contains("_members ")),
            "{order:?} counts a group's members off an order index: {plan:?}"
        );
        assert!(!plan.iter().any(|step| step.contains("TEMP B-TREE")), "{order:?} sorts: {plan:?}");
    }
}

/// The search counts query finds its rows through the search index by row
/// number, and does not read every row of the tab to test it.
#[tokio::test]
async fn a_search_reads_the_rows_its_index_finds() {
    let lib = Library::new().await;
    let tables = crate::tables::ImportTables::new(lib.db.clone());
    lib.run(lib.scan("Box/Wanted") + lib.scan("Loose")).await;
    for tab in [TriageTab::Pending, TriageTab::Done, TriageTab::Skipped] {
        let list = ImportListRequest::of(ImportListView {
            search: crate::SearchText::of("wanted"),
            ..view(tab)
        });
        let plan = tables.import_list_search_plan(list).await.unwrap();
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

/// The groups a request folds or leaves open, `count` of them, none of them
/// a group the library holds but the first, `Box`.
fn many_groups(
    lib: &Library,
    count: usize,
) -> std::collections::BTreeSet<FolderReleaseDecisionKey> {
    std::iter::once(lib.group_key("Box"))
        .chain((1..count).map(|n| lib.group_key(&format!("Folder {n}"))))
        .collect()
}

/// A Found window under folds naming 4,000 groups reads them as one list
/// subquery off the JSON array, beside the members index, whether the folds
/// name the groups folded or the groups left open: never a scan of the array
/// for each row, and no sort.
#[tokio::test]
async fn a_window_reads_four_thousand_folds_as_one_list() {
    let lib = Library::new().await;
    let tables = crate::tables::ImportTables::new(lib.db.clone());
    lib.run(lib.scan("Box/1") + lib.scan("Other/1") + lib.scan("Loose")).await;
    let groups = many_groups(&lib, 4_000);
    for folds in [ImportListFolds::Only(groups.clone()), ImportListFolds::AllBut(groups)] {
        let list = ImportListRequest {
            folds: folds.clone(),
            ..ImportListRequest::of(ordered(TriageTab::Pending, ImportListOrder::NewestFirst))
        };
        let plans = tables.import_list_window_plans(list).await.unwrap().concat();
        let (by_offset, _) = plans.split_first().expect("a part is read by its offset");
        let kind = match folds {
            ImportListFolds::Only(_) => "only",
            ImportListFolds::AllBut(_) => "all but",
        };
        assert!(
            by_offset.iter().any(|step| step.contains("import_list_newest_first_members ")),
            "{kind}: {by_offset:?}"
        );
        for plan in &plans {
            assert!(plan.iter().any(|step| step.contains("LIST SUBQUERY")), "{kind}: {plan:?}");
            assert!(
                !plan.iter().any(|step| step.contains("CORRELATED")),
                "{kind} reads the folds for each row: {plan:?}"
            );
            assert!(!plan.iter().any(|step| step.contains("TEMP B-TREE")), "{kind}: {plan:?}");
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
            let mut list = ImportListRequest::of(ordered(TriageTab::Pending, order));
            if folded {
                list = list.folding(lib.group_key("Box"));
            }
            // Found runs in one part, read by its offset off the three runs'
            // indexes, and by each seek from its first item, which searches
            // them, or reaches the row it holds every term up to the folder
            // and its number equal to by that number.
            let plans = tables.import_list_window_plans(list.clone()).await.unwrap().concat();
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
                    // The folded groups are one JSON parameter read once as a
                    // list subquery; no other table is scanned.
                    !plan
                        .iter()
                        .any(|step| step.starts_with("SCAN") && !step.starts_with("SCAN f VIRTUAL")),
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
