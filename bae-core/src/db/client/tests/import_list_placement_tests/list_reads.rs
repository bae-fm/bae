//! The import list's read over the placement: which items a list shows, in
//! what order and under which header, the counts and filter entries beside
//! them, where each row stands and what it offers, what a selection holds and
//! offers, and where a candidate is revealed. The rows are written as the
//! placement tests write them, and every answer is the list's own read.

use super::{hash, Library};
use crate::import::selection::SelectionChange;
use crate::import::{
    ImportListId, ImportListItem, ImportListOrder, ImportListProjection, ImportListRequest,
    ImportListView, PendingFilter, TriageTab,
};
use crate::library::LibraryPageWindow;

mod counts;
mod order;
mod rows;
mod selection;
mod standings;

/// `tab` in the default order, unfiltered.
fn view(tab: TriageTab) -> ImportListView {
    ImportListView {
        tab,
        ..ImportListView::default()
    }
}

/// The items one read holds, one readable line each: a header by its
/// group's name, a release or a folder that failed validation by its display
/// path.
fn sequence(projection: &ImportListProjection) -> Vec<String> {
    projection
        .windows
        .iter()
        .flat_map(|window| &window.items)
        .map(|item| match item {
            ImportListItem::GroupHeader { group, .. } => format!("group {}", group.name),
            ImportListItem::Candidate { row, .. } => format!("candidate {}", row.display_path),
            ImportListItem::Invalid { candidate, .. } => {
                format!("invalid {}", candidate.display_path)
            }
        })
        .collect()
}

/// The release row one read holds at `display_path`.
fn row_at<'a>(
    projection: &'a ImportListProjection,
    display_path: &str,
) -> (
    &'a crate::import::TriageRow,
    &'a crate::import::CandidateLiveState,
) {
    projection
        .windows
        .iter()
        .flat_map(|window| &window.items)
        .find_map(|item| match item {
            ImportListItem::Candidate { row, live, .. } if row.display_path == display_path => {
                Some((row, live))
            }
            _ => None,
        })
        .unwrap_or_else(|| panic!("{display_path} is a row of the read"))
}

impl Library {
    /// Open a list showing `view`.
    async fn open(&self, view: ImportListView) -> ImportListId {
        self.db.open_import_list(view).await.unwrap()
    }

    /// One read of `list` through `window`. A searching or folded list's
    /// items read from its tab's rows and by its other read must agree, since
    /// its windows are read by either.
    async fn read_through(
        &self,
        list: ImportListId,
        window: LibraryPageWindow,
    ) -> ImportListProjection {
        let read = self
            .db
            .load_import_list(ImportListRequest {
                list,
                windows: std::iter::once(window).collect(),
            })
            .await
            .unwrap();
        if let Some((walked, other)) = self.db.import_list_read_both_ways(list).await.unwrap() {
            assert_eq!(
                walked, other,
                "{:?}: the tab's rows in order against its other read",
                read.summary.narrowing
            );
        }
        read
    }

    /// One read of `list` through one window over every item.
    async fn read(&self, list: ImportListId) -> ImportListProjection {
        self.read_through(
            list,
            LibraryPageWindow {
                offset: 0,
                limit: u64::MAX,
            },
        )
        .await
    }

    /// One read of a list opened on `view`.
    async fn shown(&self, view: ImportListView) -> ImportListProjection {
        let list = self.open(view).await;
        self.read(list).await
    }

    /// The items a list opened on `view` shows.
    async fn items(&self, view: ImportListView) -> Vec<String> {
        sequence(&self.shown(view).await)
    }

    /// Change `list`'s selection, and answer with the revision.
    async fn select(&self, list: ImportListId, change: SelectionChange) -> u64 {
        match self
            .db
            .change_import_list_selection(list, change)
            .await
            .unwrap()
        {
            crate::db::SelectionWrite::Changed { revision } => revision,
            crate::db::SelectionWrite::RunNotShown => panic!("the run's ends are shown"),
        }
    }

    /// The release at `display_path` as this device imported it, its release
    /// written at `created_at` (RFC 3339): what puts it on Done.
    fn imported(&self, display_path: &str, created_at: &str) -> String {
        let album = bae_test_support::test_uuid(&format!("album {display_path}"));
        let release = self.release_id(display_path);
        format!(
            "INSERT INTO albums (id, title, is_compilation, created_at)
             VALUES ('{album}', 'Album Title', 0, '{created_at}');
             INSERT INTO releases (id, album_id, created_at, album_order)
             VALUES ('{release}', '{album}', '{created_at}', 0);
             INSERT INTO local_release_origin (release_id, content_hash)
             VALUES ('{release}', '{hash}');",
            hash = hash(display_path),
        )
    }

    /// The id of the release [`Self::imported`] writes for `display_path`.
    fn release_id(&self, display_path: &str) -> String {
        bae_test_support::test_uuid(&format!("release {display_path}"))
    }

    /// The person's skip of the release at `display_path`.
    fn skip(&self, display_path: &str) -> String {
        format!(
            "INSERT INTO skipped_import_candidates (candidate_path) VALUES ('{}');",
            self.path(display_path)
        )
    }

    /// A run of identification `stage` (`queued`, `running`, `finalizing`)
    /// for the release at `display_path`.
    fn identifying(&self, display_path: &str, stage: &str) -> String {
        let (admission, run) = match stage {
            "queued" => ("'automatic'", "NULL"),
            _ => ("NULL", "1"),
        };
        format!(
            "INSERT INTO import_candidate_standing
                 (candidate_key, identification, content_hash, file_edit_revision, admission,
                  identify_run)
             VALUES ('{}', '{stage}', '{}', 0, {admission}, {run});",
            self.path(display_path),
            hash(display_path),
        )
    }

    /// An import `stage` (`queued`, `running`, `writing`) owning the release
    /// at `display_path`.
    fn importing(&self, display_path: &str, stage: &str) -> String {
        format!(
            "INSERT INTO import_candidate_standing (candidate_key, import, import_id)
             VALUES ('{}', '{stage}', 'import-{display_path}');",
            self.path(display_path),
        )
    }

    /// The failure the last import of the release at `display_path` left.
    fn import_failed(&self, display_path: &str) -> String {
        format!(
            "INSERT INTO import_candidate_failure (content_hash, error, failed_at)
             VALUES ('{}', 'it broke', '2026-01-01T00:00:00Z');",
            hash(display_path)
        )
    }

    /// The release at `display_path` linked to an album, its pressing
    /// unknown.
    fn linked(&self, display_path: &str) -> String {
        format!(
            "INSERT INTO import_candidate_release_link (content_hash, kind) VALUES ('{h}', 'album');
             INSERT INTO import_candidate_release_link_album (content_hash, kind, catalog, album_id)
             VALUES ('{h}', 'album', 'musicbrainz', 'group-{display_path}');",
            h = hash(display_path),
        )
    }

    /// Mark the draft of the release at `display_path` one that would import.
    fn valid_draft(&self, display_path: &str) -> String {
        format!(
            "UPDATE import_candidate_edit SET draft_valid = 1 WHERE content_hash = '{}';",
            hash(display_path)
        )
    }
}

/// The candidate releases of `display_paths`, each scanned with a state
/// whose blank draft a person owns.
fn scanned_with_states(lib: &Library, display_paths: &[&str]) -> String {
    display_paths
        .iter()
        .map(|display_path| format!("{}{}", lib.scan(display_path), lib.state(display_path)))
        .collect()
}

/// `view` narrowed to `filter`.
fn under(tab: TriageTab, filter: PendingFilter) -> ImportListView {
    ImportListView {
        pending_filter: filter,
        ..view(tab)
    }
}

/// `view` ordered by `order`.
fn ordered(tab: TriageTab, order: ImportListOrder) -> ImportListView {
    ImportListView { order, ..view(tab) }
}
