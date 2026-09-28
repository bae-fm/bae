//! The import list's selection: the candidate keys a person has selected, one
//! row each, so a selection of any size is a table rather than a set some
//! surface holds. The list reads it into each row, and what the selection can
//! be told to do is read from it.

use super::import_list::{load_import_queue_on, DoneRowText};
use super::*;
use crate::import::selection::{SelectedCandidate, SelectionChange};
use crate::import::ImportListRequest;

impl Database {
    /// Empty the selection. Its lifetime is one app session, so the library
    /// empties it as it opens.
    pub(crate) async fn clear_candidate_selection(&self) -> Result<(), DbError> {
        let selected = self.selected_keys().await?;
        self.write_selection(selected.into_iter().collect(), Vec::new())
            .await
            .map(|_| ())
    }

    /// The selected candidates as the tables place them, now and on every
    /// write that changes them.
    pub(crate) fn subscribe_selected_candidates(
        &self,
    ) -> coven::LiveQuery<Vec<SelectedCandidate>> {
        self.inner
            .handle
            .subscribe(|sql| {
                load_import_queue_on(&sql, DoneRowText::Skip).map_err(CovenError::from)
            })
            .process(|rows| {
                crate::import::list::selected_candidates(&rows)
                    .map_err(|error| CovenError::from(DbError::Message(error.to_string())))
            })
    }

    /// The selected candidates as the tables place them now.
    pub(crate) async fn load_selected_candidates(
        &self,
    ) -> Result<Vec<SelectedCandidate>, DbError> {
        self.read(|sql| load_import_queue_on(&sql, DoneRowText::Skip))
            .process(|rows| {
                crate::import::list::selected_candidates(&rows)
                    .map_err(|error| DbError::Message(error.to_string()))
            })
            .await
    }

    /// The selected candidates in the order the list places them under
    /// `request`, which a bulk action runs over them in.
    pub(crate) async fn load_selected_candidates_in_view_order(
        &self,
        request: ImportListRequest,
    ) -> Result<Vec<SelectedCandidate>, DbError> {
        let done_row_text = DoneRowText::of(&request.view);
        self.read(move |sql| load_import_queue_on(&sql, done_row_text))
            .process(move |rows| {
                crate::import::list::selected_candidates_in_view_order(&rows, &request)
                    .map_err(|error| DbError::Message(error.to_string()))
            })
            .await
    }

    /// Apply one change a person made by pointing at rows of the list
    /// `request` shows, and say the selection revision a list read that
    /// reflects it carries.
    pub(crate) async fn change_candidate_selection(
        &self,
        request: ImportListRequest,
        change: SelectionChange,
    ) -> Result<u64, DbError> {
        let selected = self.selected_keys().await?;
        let (remove, add) = match change {
            SelectionChange::Replace { keys } => {
                let keys: HashSet<String> = keys.into_iter().collect();
                (
                    selected.difference(&keys).cloned().collect(),
                    keys.difference(&selected).cloned().collect(),
                )
            }
            SelectionChange::Toggle { add, remove } => (
                remove.into_iter().filter(|key| selected.contains(key)).collect(),
                add.into_iter().filter(|key| !selected.contains(key)).collect(),
            ),
            SelectionChange::Extend { from, to } => {
                let shown = self.shown_candidate_keys(request).await?;
                let position = |key: &str| shown.iter().position(|shown| shown == key);
                let (Some(from), Some(to)) = (position(&from), position(&to)) else {
                    return Err(DbError::Message(format!(
                        "the list does not show {from} and {to}"
                    )));
                };
                let (start, end) = (from.min(to), from.max(to));
                (
                    Vec::new(),
                    shown[start..=end]
                        .iter()
                        .filter(|key| !selected.contains(*key))
                        .cloned()
                        .collect(),
                )
            }
        };
        self.write_selection(remove, add).await
    }

    /// Select every candidate the list shows under `request`, loaded by a
    /// surface or not: that key set, as the tables hold it now.
    pub(crate) async fn select_shown_candidates(
        &self,
        request: ImportListRequest,
    ) -> Result<(), DbError> {
        let selected = self.selected_keys().await?;
        let add = self
            .shown_candidate_keys(request)
            .await?
            .into_iter()
            .filter(|key| !selected.contains(key))
            .collect();
        self.write_selection(Vec::new(), add).await.map(|_| ())
    }

    /// Keep only the selected candidates the list shows under `request`: a
    /// row a new view hides leaves the selection, and none it shows joins it.
    pub(crate) async fn keep_shown_candidate_selection(
        &self,
        request: ImportListRequest,
    ) -> Result<(), DbError> {
        let shown: HashSet<String> =
            self.shown_candidate_keys(request).await?.into_iter().collect();
        let remove = self
            .selected_keys()
            .await?
            .into_iter()
            .filter(|key| !shown.contains(key))
            .collect();
        self.write_selection(remove, Vec::new()).await.map(|_| ())
    }

    /// Take `remove` out of the selection and put `add` in, as one write that
    /// also counts the selection's revision up, and return that revision. No
    /// write at all when both are empty, since the store refuses a write that
    /// changes nothing: the revision the last change left is returned.
    async fn write_selection(&self, remove: Vec<String>, add: Vec<String>) -> Result<u64, DbError> {
        if remove.is_empty() && add.is_empty() {
            return self.read(|sql| selection_revision_on(&sql)).await;
        }
        self.call(move |sql| {
            for key in &remove {
                sql.execute(
                    "DELETE FROM candidate_selection WHERE candidate_key = ?",
                    [key],
                )?;
            }
            for key in &add {
                select_on(sql, key)?;
            }
            Ok(sql.query_row(
                "UPDATE candidate_selection_revision SET revision = revision + 1 \
                 RETURNING revision",
                [],
                |row| row.get::<_, i64>(0),
            )? as u64)
        })
        .await
    }

    async fn selected_keys(&self) -> Result<HashSet<String>, DbError> {
        self.read(|sql| {
            Ok(sql
                .query("SELECT candidate_key FROM candidate_selection", [], |row| {
                    row.get::<_, String>(0)
                })?
                .into_iter()
                .collect())
        })
        .await
    }

    /// The keys of the candidates the list shows under `request`, in its
    /// order.
    async fn shown_candidate_keys(&self, request: ImportListRequest) -> Result<Vec<String>, DbError> {
        let done_row_text = DoneRowText::of(&request.view);
        self.read(move |sql| load_import_queue_on(&sql, done_row_text))
            .process(move |rows| {
                crate::import::list::shown_candidate_keys(&rows, &request)
                    .map_err(|error| DbError::Message(error.to_string()))
            })
            .await
    }
}

/// How many changes a person has made to the selection, as the caller's read
/// sees it.
pub(super) fn selection_revision_on(sql: &SqlReadContext<'_>) -> Result<u64, DbError> {
    Ok(sql.query_row(
        "SELECT revision FROM candidate_selection_revision",
        [],
        |row| row.get::<_, i64>(0),
    )? as u64)
}

/// Whether `key` is selected, inside the caller's transaction.
pub(super) fn is_selected_on(sql: &SqlContext<'_, '_>, key: &str) -> Result<bool, DbError> {
    Ok(sql
        .query_row(
            "SELECT 1 FROM candidate_selection WHERE candidate_key = ?",
            [key],
            |_| Ok(()),
        )
        .optional()?
        .is_some())
}

/// Select `key`, inside the caller's transaction.
pub(super) fn select_on(sql: &SqlContext<'_, '_>, key: &str) -> Result<(), DbError> {
    sql.execute(
        "INSERT INTO candidate_selection (candidate_key) VALUES (?) ON CONFLICT DO NOTHING",
        [key],
    )?;
    Ok(())
}
