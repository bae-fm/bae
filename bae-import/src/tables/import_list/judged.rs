//! The rows a request keeps, as SQL fragments the list's statements embed.
//!
//! The request holds the view's tab, filter entry and filter text, and the
//! groups it folds shut. Each fragment is a subquery over the placement
//! (`import_list_placement`) built from the request: it tests the filter entry
//! with the bound `:filter`, the text with `bae_search_pattern(:filter_text)`,
//! and the folds with `:folded`, one JSON array tested only as
//! `(root, key) IN (SELECT ... FROM json_each(:folded))`, which keeps Found's
//! merge of its indexes. A request that narrows nothing and folds nothing
//! leaves the tests out of the statement, so no row is tested.

use super::*;

/// The statement selecting the keys of the folded groups that the parameter
/// `folded` holds, as the placement holds them.
fn folded_keys(folded: &str) -> String {
    format!(
        "SELECT unhex(json_extract(f.value, '$[0]')), unhex(json_extract(f.value, '$[1]')) \
         FROM json_each({folded}) AS f"
    )
}

/// The request's fragments with its folds read from the parameter `folded`,
/// so one statement can name two requests' folds.
pub(super) struct Fragments<'a> {
    request: &'a ImportListRequest,
    folded: &'a str,
}

impl ImportListRequest {
    /// The folded groups as the JSON array `:folded` binds: each group's
    /// watched folder path and folder, as hex of the bytes the placement
    /// holds.
    pub(super) fn folded_json(&self) -> String {
        let groups: Vec<[String; 2]> = self
            .folded
            .iter()
            .map(|group| {
                [
                    hex::encode(coven::path_bytes(&group.watched_folder_path)),
                    hex::encode(group.relative_folder.as_bytes()),
                ]
            })
            .collect();
        serde_json::to_string(&groups).expect("a list of text pairs serializes")
    }

    /// The test that Found's filter entry keeps the row aliased `p`; `None`
    /// on another tab, or while the entry keeps every row.
    pub(super) fn entry_sql(&self, p: &str) -> Option<String> {
        self.entry().map(|_entry| format!("{p}.pending_entry = :filter"))
    }

    /// The test that the row aliased `p`'s shown text matches the search's
    /// pattern, the test `import_list_search` finds the matching rows by;
    /// `None` while the list searches for nothing.
    fn text_sql(&self, p: &str) -> Option<String> {
        self.filters_text().then(|| {
            format!(
                "({p}.filter_title GLOB bae_search_pattern(:filter_text) \
                  OR {p}.filter_artists GLOB bae_search_pattern(:filter_text))"
            )
        })
    }

    /// The test that the filters keep the row aliased `p`: on Found, its
    /// filter entry; and its shown text matching the search's pattern. `1`
    /// when nothing narrows the list.
    pub(super) fn kept_sql(&self, p: &str) -> String {
        match (self.entry_sql(p), self.text_sql(p)) {
            (None, None) => "1".to_string(),
            (Some(test), None) | (None, Some(test)) => test,
            (Some(entry), Some(text)) => format!("{entry} AND {text}"),
        }
    }

    /// The request's fragments, its folds read from `:folded`.
    pub(super) fn fragments(&self) -> Fragments<'_> {
        Fragments { request: self, folded: ":folded" }
    }

    /// The request's fragments, its folds read from the parameter `folded`.
    pub(super) fn fragments_folding<'a>(&'a self, folded: &'a str) -> Fragments<'a> {
        Fragments { request: self, folded }
    }

    /// The test that the list folds the group at `root` and `folder` shut;
    /// `0` when it folds none.
    pub(super) fn folds_sql(&self, root: &str, folder: &str) -> String {
        self.fragments().folds_sql(root, folder)
    }

    /// The rows of the request's tab with their `kept` and `folded` tests.
    pub(super) fn judged(&self) -> String {
        self.fragments().judged()
    }

    /// The releases the filters keep, folded groups' members included.
    pub(super) fn matching(&self) -> String {
        self.fragments().matching()
    }

    /// The list's items, in no order.
    pub(super) fn items(&self) -> String {
        self.fragments().items()
    }

    /// The releases the list shows.
    pub(super) fn shown(&self) -> String {
        self.fragments().shown()
    }

    /// The entries of the search index the search's pattern matches.
    pub(super) fn found(&self) -> String {
        Fragments::found()
    }
}

impl Fragments<'_> {
    /// The test that the list folds the group at `root` and `folder` shut;
    /// `0` when it folds none.
    pub(super) fn folds_sql(&self, root: &str, folder: &str) -> String {
        match self.request.folded.is_empty() {
            true => "0".to_string(),
            false => format!("(({root}, {folder}) IN ({}))", folded_keys(self.folded)),
        }
    }

    /// The rows of the request's tab, each with whether its filters keep it
    /// (`kept`) and whether the list folds the row's group shut (`folded`).
    /// The tab is matched by value rather than by index (`+p.tab`), so a
    /// lookup of a group's members reaches them by their group.
    pub(super) fn judged(&self) -> String {
        format!(
            "(SELECT p.*, {kept} AS kept, \
                    (p.group_folder IS NOT NULL AND {folded}) AS folded \
             FROM import_list_placement AS p WHERE +p.tab = :tab)",
            kept = self.request.kept_sql("p"),
            folded = self.folds_sql("p.watched_folder_path", "p.group_folder"),
        )
    }

    /// The releases the filters keep, folded groups' members included.
    pub(super) fn matching(&self) -> String {
        format!("(SELECT * FROM {} WHERE item <> 'group' AND kept)", self.judged())
    }

    /// The list's items, in no order: the releases its filters keep outside
    /// the groups it folds shut, and the header of each group holding a
    /// release its filters keep.
    pub(super) fn items(&self) -> String {
        if self.request.keeps_all() {
            return self.judged();
        }
        format!(
            "(SELECT j.* FROM {judged} AS j \
              WHERE CASE WHEN j.item = 'group' \
                         THEN EXISTS (SELECT 1 FROM {matching} AS m \
                                      WHERE m.watched_folder_path = j.watched_folder_path \
                                        AND m.group_folder = j.key) \
                         ELSE j.kept AND NOT j.folded END)",
            judged = self.judged(),
            matching = self.matching(),
        )
    }

    /// The releases the list shows.
    pub(super) fn shown(&self) -> String {
        format!("(SELECT * FROM {} WHERE item <> 'group')", self.items())
    }

    /// The entries of the search index whose shown text matches the search's
    /// pattern, every tab's, as the index finds them: what a search's counts
    /// read instead of testing every row of the tab.
    pub(super) fn found() -> String {
        "(SELECT s.rowid AS entry FROM import_list_search AS s \
          WHERE s.title GLOB bae_search_pattern(:filter_text) \
             OR s.artists GLOB bae_search_pattern(:filter_text))"
            .to_string()
    }
}
