//! One list a surface has open: its row in the tables, and the live queries
//! that read it.
//!
//! Opening the list writes its row (`import_list`); its view and the groups
//! it folds are rows too, so changing either is a write, and the live queries
//! that read them rerun on it. Each window of the list, its count, its
//! summary, Found's filter entries and what the selected rows hold are live
//! queries of their own, opened from here and ended when the list closes. The
//! surface keeps the keys of the rows the person selected, and passes them to
//! each call that reads or acts on them.
//! LiveImportList keeps nothing about the windows a surface holds: a window's
//! request is the range of the list it reads, and the surface moves each
//! window.
//!
//! Where the folder scans stand is not here: a scan moves its found count
//! with every folder it walks, and that count moves no row, so the scans are
//! their own subscription.

use super::{
    ImportCandidateListLocation, ImportListCount, ImportListId, ImportListView,
    ImportListViewChange, ImportQueueSummary, ImportWindow, PendingFilterEntry,
};
use crate::selection::{SelectedRows, SelectionSummary};
use crate::tables::{ImportListError, ImportTables};
use crate::{CandidateKey, FolderReleaseDecisionKey};
use bae_store::list_windows::OrderKey;
use bae_store::live_query::{CancellableLiveQuery, IntoLiveSource, LiveValues};
use bae_store::{DbError, ListWindow};
use std::collections::BTreeSet;
use tokio_util::sync::CancellationToken;

#[derive(Debug, thiserror::Error)]
pub enum ImportListFailure {
    #[error("the import list is closed")]
    Closed,
    #[error(transparent)]
    Database(#[from] DbError),
}

/// Only this list's `cancel` closes a list a surface holds open, so a list
/// the read finds closed means `cancel` ran.
impl From<ImportListError> for ImportListFailure {
    fn from(error: ImportListError) -> Self {
        match error {
            ImportListError::Closed(_) => Self::Closed,
            ImportListError::Database(error) => Self::Database(error),
        }
    }
}

/// One open list. Every live query opened from it ends when it closes, and
/// when the library does. Dropped without [`Self::cancel`], its row stays
/// until the library next opens, when every list closes. A clone names the
/// same list, so the list's view opens its window and position from one.
#[derive(Clone)]
pub struct LiveImportList {
    list: ImportListId,
    tables: ImportTables,
    /// Cancelled when the list closes; each live query takes a child.
    closing: CancellationToken,
    /// Receives each bug that a run of one of the list's live queries
    /// reports.
    bugs: coven::task::BugSinkRef,
}

impl LiveImportList {
    /// Open a list showing `view`, with nothing folded. `closing` is the
    /// library's cancellation: the list's live queries end with it.
    pub(crate) async fn open(
        tables: ImportTables,
        closing: CancellationToken,
        bugs: coven::task::BugSinkRef,
        view: ImportListView,
    ) -> Result<Self, DbError> {
        let list = tables.open_import_list(view).await?;
        Ok(Self { list, tables, closing, bugs })
    }

    /// Show a different tab, filter or order. `selected` holds the keys the
    /// surface keeps selected. On another tab, filter or search, the answer
    /// says where the new view opens, at the first selected row the view
    /// shows or at its top, and lists the selected keys the new view doesn't
    /// show, which leave the selection. For another order, or for the view
    /// the list already shows, the call answers nothing. A surface that gets
    /// an answer drops the windows it holds and opens new ones from the place
    /// the answer names.
    pub async fn set_view(
        &self,
        view: ImportListView,
        selected: Vec<CandidateKey>,
    ) -> Result<Option<ImportListViewChange>, ImportListFailure> {
        Ok(self.tables.set_import_list_view(self.list, view, selected).await?)
    }

    /// Fold one group shut or open it, and answer with the keys of
    /// `selected`, the rows the surface keeps selected, whose rows the fold
    /// hid. Its entries leave the list when it folds, so the windows holding
    /// the group change and those below it move.
    pub async fn set_group_folded(
        &self,
        group: FolderReleaseDecisionKey,
        folded: bool,
        selected: Vec<CandidateKey>,
    ) -> Result<Vec<CandidateKey>, ImportListFailure> {
        Ok(self.tables.set_import_list_group_folded(self.list, group, folded, selected).await?)
    }

    /// Fold every group shut, or open them all, as one change, and answer
    /// with the keys of `selected` whose rows the fold hid.
    pub async fn set_all_groups_folded(
        &self,
        folded: bool,
        selected: Vec<CandidateKey>,
    ) -> Result<Vec<CandidateKey>, ImportListFailure> {
        Ok(self.tables.set_import_list_groups_folded(self.list, folded, selected).await?)
    }

    /// The keys of every row the list shows, whether or not a surface loaded
    /// it: what Select All selects.
    pub async fn keys(&self) -> Result<Vec<CandidateKey>, ImportListFailure> {
        Ok(self.tables.import_list_keys(self.list).await?)
    }

    /// The keys of the rows the list shows on `tab` in `order`, the tab and
    /// order the surface shows it in, from the row `from` to the row `to`,
    /// both included, in that order: what a Shift gesture selects.
    pub async fn keys_between(
        &self,
        tab: crate::TriageTab,
        order: super::ImportListOrder,
        from: CandidateKey,
        to: CandidateKey,
    ) -> Result<Vec<CandidateKey>, ImportListFailure> {
        Ok(self.tables.import_list_keys_between(self.list, tab, order, from, to).await?)
    }

    /// The rows at `selected` the list shows, in its order: what a bulk
    /// action runs over.
    pub async fn selected(
        &self,
        selected: Vec<CandidateKey>,
    ) -> Result<SelectedRows, ImportListFailure> {
        Ok(self.tables.load_import_list_selected(self.list, selected, None).await?)
    }

    /// The rows at `selected` the list shows that offer `action`, in its
    /// order: what a bulk action of that kind runs over.
    pub async fn selected_offering(
        &self,
        selected: Vec<CandidateKey>,
        action: crate::CandidateAction,
    ) -> Result<SelectedRows, ImportListFailure> {
        Ok(self.tables.load_import_list_selected(self.list, selected, Some(action)).await?)
    }

    /// The view and place that reveal the candidate at `candidate_key` at its
    /// current placement: its tab, with the filters cleared and its group
    /// opened. `None` when the list lists no such candidate.
    pub async fn locate_candidate(
        &self,
        candidate_key: &crate::CandidateKey,
    ) -> Result<Option<ImportCandidateListLocation>, ImportListFailure> {
        Ok(self.tables.locate_import_candidate(self.list, candidate_key).await?)
    }

    /// Where the candidate at `candidate_key` sits in what the list shows
    /// under its view now, its filters and folds as they are; `None` when
    /// the view does not show it.
    pub async fn candidate_position(
        &self,
        candidate_key: &crate::CandidateKey,
    ) -> Result<Option<u64>, ImportListFailure> {
        Ok(self.tables.import_candidate_position(self.list, candidate_key).await?)
    }

    /// The first candidate the identification count is still waiting on —
    /// one queued, running or having its answer written — in the list's
    /// order. `None` when nothing is being identified, or nothing that is has
    /// a row.
    pub async fn first_identifying_candidate(
        &self,
    ) -> Result<Option<crate::CandidateKey>, ImportListFailure> {
        Ok(self.tables.first_identifying_import_candidate(self.list).await?)
    }

    /// The key of the item at `position`, or of the last item when the list
    /// ends before it; `None` for a list with no items. A jump reads it once
    /// and starts its windows there.
    pub async fn key_at(&self, position: u64) -> Result<Option<OrderKey>, ImportListFailure> {
        Ok(self.tables.import_key_at(self.list, position).await?)
    }

    /// One window of the list as a live query whose window the caller changes
    /// on the same query.
    pub fn subscribe_window(
        &self,
        initial: ListWindow,
    ) -> coven::ReconfigurableLiveQuery<ListWindow, ImportWindow, DbError> {
        self.tables.subscribe_import_window(self.list, initial)
    }

    /// How many items the list holds and how many its narrowing keeps.
    pub fn subscribe_count(&self) -> coven::LiveQuery<ImportListCount, DbError> {
        self.tables.subscribe_import_count(self.list)
    }

    /// How many items sit before the one at a key, as a live query whose key
    /// the caller changes on the same query.
    pub fn subscribe_position(
        &self,
        initial: OrderKey,
    ) -> coven::ReconfigurableLiveQuery<OrderKey, u64, DbError> {
        self.tables.subscribe_import_position(self.list, initial)
    }

    /// The watched folders, whether Found has groups, and the narrowing.
    pub fn subscribe_summary(&self) -> coven::LiveQuery<ImportQueueSummary, DbError> {
        self.tables.subscribe_import_summary(self.list)
    }

    /// Every entry of Found's filter, with how many of Found's rows it holds.
    pub fn subscribe_filter_entries(&self) -> coven::LiveQuery<Vec<PendingFilterEntry>, DbError> {
        self.tables.subscribe_import_filter_entries()
    }

    /// What the rows at the caller's keys hold and the actions the person
    /// can run on them, whether or not the list shows them, as a live query
    /// whose keys the caller changes on the same query.
    pub fn subscribe_selection(
        &self,
    ) -> coven::ReconfigurableLiveQuery<BTreeSet<CandidateKey>, SelectionSummary, DbError> {
        self.tables.subscribe_import_selection()
    }

    /// `query` as a subscription a surface pulls the values of, which this
    /// list's closing ends.
    pub fn until_closed<Request, Projection>(
        &self,
        query: coven::ReconfigurableLiveQuery<Request, Projection, DbError>,
    ) -> CancellableLiveQuery<Request, Projection, DbError>
    where
        Request: Clone + PartialEq + Send + Sync + 'static,
        Projection: Clone + PartialEq + Send + 'static,
    {
        CancellableLiveQuery::new(query, self.closing.child_token(), self.bugs.clone())
    }

    /// `query`'s values for a surface to pull, which this list's closing
    /// ends.
    pub fn values_until_closed<T: Clone + PartialEq + Send + 'static>(
        &self,
        query: impl IntoLiveSource<T>,
    ) -> LiveValues<T> {
        LiveValues::new(query, self.closing.child_token(), self.bugs.clone())
    }

    /// Everything the list shows, and what the rows at `selected` hold and
    /// offer, in one read.
    #[cfg(any(test, feature = "test-utils"))]
    pub async fn read_for_test(&self, selected: Vec<CandidateKey>) -> super::ImportListRead {
        self.tables.read_import_list_for_test(self.list, selected).await.expect("the list reads")
    }

    /// Close the list: every live query opened from it ends, and its row and
    /// folds go.
    pub async fn cancel(&self) -> Result<(), ImportListFailure> {
        self.closing.cancel();
        Ok(self.tables.close_import_list(self.list).await?)
    }
}
