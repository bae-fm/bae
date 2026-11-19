//! The counts kept beside the placement: each of the import's writes that
//! places, moves or removes a placement row moves its count in the same
//! write, and the counts always hold what a COUNT over the placement says.

use crate::tables::import_list::tests::*;

impl Library {
    /// The counts as they stand, one readable line each: the tab, the item,
    /// the filter entry when the rows have one, and the count.
    async fn counts(&self) -> Vec<String> {
        self.stored_counts()
            .await
            .into_iter()
            .map(|(tab, item, entry, count)| {
                let entry = entry.map(|entry| format!(" {entry}")).unwrap_or_default();
                format!("{tab} {item}{entry} {count}")
            })
            .collect()
    }

    /// Run `write` alone, check the counts against a COUNT over the
    /// placement ([`Library::run`]), and return them.
    async fn counts_after(&self, write: Write) -> Vec<String> {
        self.run(write).await;
        self.counts().await
    }

    /// Run `write` alone, check the counts against a COUNT over the
    /// placement ([`Library::run`]), and return the group counts, one
    /// readable line each: the group's folder, the filter entry and the
    /// count. Every group here is under the one watched folder.
    async fn group_counts_after(&self, write: Write) -> Vec<String> {
        self.run(write).await;
        self.stored_group_counts()
            .await
            .into_iter()
            .map(|(_root, group, entry, count)| format!("{group} {entry} {count}"))
            .collect()
    }
}

/// Every kind of write that moves a placement row moves its count with it:
/// a scan finding a release, a folder that fails validation and a nested
/// release with its group's header; a skip and its undoing; a lookup
/// starting and ending, and its answer; an import failing, finishing, and
/// its release deleted; releases combined into one and kept apart again; a
/// pass that no longer finds a folder; and the watched folder removed. After
/// each, the counts hold what a COUNT over the placement says.
#[tokio::test]
async fn each_write_moves_the_counts_of_the_rows_it_moves() {
    let lib = Library::new().await;
    assert!(lib.counts().await.is_empty(), "an empty list counts nothing");

    assert_eq!(
        lib.counts_after(lib.scan("A") + lib.scan("B")).await,
        ["pending candidate not_looked_up 2"]
    );
    assert_eq!(
        lib.counts_after(lib.scan_as("Broken", "invalid")).await,
        ["pending candidate not_looked_up 2", "skipped invalid 1"]
    );
    assert_eq!(
        lib.counts_after(lib.scan("Box/Disc 1") + lib.scan("Box/Disc 2")).await,
        ["pending candidate not_looked_up 4", "pending group 1", "skipped invalid 1"],
        "a group's header counts as its own item"
    );

    assert_eq!(
        lib.counts_after(lib.skip("A")).await,
        [
            "pending candidate not_looked_up 3",
            "pending group 1",
            "skipped candidate 1",
            "skipped invalid 1"
        ]
    );
    assert_eq!(
        lib.counts_after(lib.set_skipped("A", false)).await,
        ["pending candidate not_looked_up 4", "pending group 1", "skipped invalid 1"]
    );

    assert_eq!(
        lib.counts_after(lib.identifying("B", "running")).await,
        [
            "pending candidate in_progress 1",
            "pending candidate not_looked_up 3",
            "pending group 1",
            "skipped invalid 1"
        ]
    );
    assert_eq!(
        lib.counts_after(lib.reopened()).await,
        ["pending candidate not_looked_up 4", "pending group 1", "skipped invalid 1"]
    );
    assert_eq!(
        lib.counts_after(lib.verdict("B", "failed", Some(11))).await,
        [
            "pending candidate needs_you 1",
            "pending candidate not_looked_up 3",
            "pending group 1",
            "skipped invalid 1"
        ]
    );

    assert_eq!(
        lib.counts_after(lib.import_failed("A")).await,
        [
            "pending candidate needs_you 2",
            "pending candidate not_looked_up 2",
            "pending group 1",
            "skipped invalid 1"
        ]
    );
    assert_eq!(
        lib.counts_after(lib.imported("Box/Disc 1", "2026-01-02T00:00:00Z")).await,
        [
            "done candidate 1",
            "pending candidate needs_you 2",
            "pending candidate not_looked_up 1",
            "pending group 1",
            "skipped invalid 1"
        ]
    );
    assert_eq!(
        lib.counts_after(lib.release_deleted("Box/Disc 1")).await,
        [
            "pending candidate needs_you 2",
            "pending candidate not_looked_up 2",
            "pending group 1",
            "skipped invalid 1"
        ]
    );

    assert_eq!(
        lib.counts_after(lib.combine(&["Box/Disc 1", "Box/Disc 2"], "Box")).await,
        [
            "pending candidate needs_you 2",
            "pending candidate not_looked_up 1",
            "pending group 1",
            "skipped invalid 1"
        ],
        "the combined release takes its two members' place under their group"
    );
    assert_eq!(
        lib.counts_after(lib.separate("Box")).await,
        [
            "pending candidate needs_you 2",
            "pending candidate not_looked_up 2",
            "pending group 1",
            "skipped invalid 1"
        ]
    );

    assert_eq!(
        lib.counts_after(lib.unscan("Broken")).await,
        ["pending candidate needs_you 2", "pending candidate not_looked_up 2", "pending group 1"]
    );
    assert!(
        lib.counts_after(lib.unwatch()).await.is_empty(),
        "a list with no watched folder counts nothing"
    );
}

/// Every kind of write that moves a group's member moves its group count with
/// it: members found below a folder, and a release at a folder once another
/// is found below it; a member's lookup ending; a skip and its undoing;
/// members combined into one release and kept apart again; an import
/// finishing; a pass that no longer finds a member; and the watched folder
/// removed. After each, the group counts hold what a COUNT over the members
/// says.
#[tokio::test]
async fn each_write_moves_the_group_counts_of_the_members_it_moves() {
    let lib = Library::new().await;
    assert_eq!(
        lib.group_counts_after(lib.scan("Box/Disc 1") + lib.scan("Box/Disc 2") + lib.scan("Solo"))
            .await,
        ["Box not_looked_up 2"],
        "a release outside every group counts in no group"
    );
    assert_eq!(
        lib.group_counts_after(lib.scan("Solo/Bonus")).await,
        ["Box not_looked_up 2", "Solo not_looked_up 2"],
        "a release found below another puts both in the other's group"
    );
    assert_eq!(
        lib.group_counts_after(lib.verdict("Box/Disc 1", "failed", Some(11))).await,
        ["Box needs_you 1", "Box not_looked_up 1", "Solo not_looked_up 2"]
    );
    assert_eq!(
        lib.group_counts_after(lib.skip("Box/Disc 2")).await,
        ["Box needs_you 1", "Solo not_looked_up 2"],
        "a skipped release leaves Found and its group"
    );
    assert_eq!(
        lib.group_counts_after(lib.set_skipped("Box/Disc 2", false)).await,
        ["Box needs_you 1", "Box not_looked_up 1", "Solo not_looked_up 2"]
    );
    assert_eq!(
        lib.group_counts_after(lib.combine(&["Box/Disc 1", "Box/Disc 2"], "Box")).await,
        ["Box not_looked_up 1", "Solo not_looked_up 2"],
        "the combined release takes its two members' place in their group"
    );
    assert_eq!(
        lib.group_counts_after(lib.separate("Box")).await,
        ["Box needs_you 1", "Box not_looked_up 1", "Solo not_looked_up 2"]
    );
    assert_eq!(
        lib.group_counts_after(lib.imported("Box/Disc 2", "2026-01-02T00:00:00Z")).await,
        ["Box needs_you 1", "Solo not_looked_up 2"],
        "an imported release leaves Found and its group"
    );
    assert_eq!(
        lib.group_counts_after(lib.unscan("Solo/Bonus")).await,
        ["Box needs_you 1"],
        "a release with nothing found below it any more is in no group"
    );
    assert!(
        lib.group_counts_after(lib.unwatch()).await.is_empty(),
        "a list with no watched folder counts no group"
    );
}
