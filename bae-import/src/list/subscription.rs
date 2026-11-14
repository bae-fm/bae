//! One list a surface has open: its row in the tables, and the live queries
//! that read it.
//!
//! Opening the list writes its row (`import_list`); its view, the groups it
//! folds and its selection are rows too, so changing any of them is a write,
//! and the live queries that read them rerun on it. Each window of the list,
//! its count, its summary, Found's filter entries and its selection are live
//! queries of their own, opened from here and ended when the list closes.
//! Nothing about the windows a surface holds is kept here: a window's request
//! is the range of the list it reads, and moving it is the surface's.
//!
//! Where the folder scans stand is not here: a scan moves its found count
//! with every folder it walks, and that count moves no row, so the scans are
//! their own subscription.

use super::{
    ImportCandidateListLocation, ImportListCount, ImportListId, ImportListOpening, ImportListView,
    ImportQueueSummary, ImportWindow, PendingFilterEntry,
};
use crate::selection::{SelectedRows, SelectionChange, SelectionSummary};
use crate::tables::{ImportListError, ImportTables, SelectionWrite};
use crate::FolderReleaseDecisionKey;
use bae_store::list_windows::OrderKey;
use bae_store::live_query::{CancellableLiveQuery, LiveSource, LiveValues};
use bae_store::{DbError, ListWindow};
use tokio_util::sync::CancellationToken;

#[derive(Debug, thiserror::Error)]
pub enum ImportListFailure {
    #[error("import list closed")]
    Closed,
    #[error("the list does not show both ends of the run of rows to select")]
    RunNotShown,
    #[error(transparent)]
    Database(#[from] DbError),
}

/// A list found closed was closed by this list's own cancel, the one write
/// that closes a list a surface holds open.
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
/// until the library next opens, when every list closes.
pub struct LiveImportList {
    list: ImportListId,
    tables: ImportTables,
    /// Cancelled when the list closes; each live query takes a child.
    closing: CancellationToken,
    /// Where a bug a run of one of its live queries carries goes.
    bugs: coven::task::BugSinkRef,
}

impl LiveImportList {
    /// Open a list showing `view`, with nothing folded and nothing selected.
    /// `closing` is the library's cancellation: the list's live queries end
    /// with it.
    pub(crate) async fn open(
        tables: ImportTables,
        closing: CancellationToken,
        bugs: coven::task::BugSinkRef,
        view: ImportListView,
    ) -> Result<Self, DbError> {
        let list = tables.open_import_list(view).await?;
        Ok(Self { list, tables, closing, bugs })
    }

    /// Show a different tab, filter or order, and answer where the new view
    /// opens: its first selected row, or its top. The view it already shows
    /// changes nothing, and answers nothing. A surface that gets an answer
    /// drops the windows it holds and opens new ones from that place.
    pub async fn set_view(
        &self,
        view: ImportListView,
    ) -> Result<Option<ImportListOpening>, ImportListFailure> {
        Ok(self.tables.set_import_list_view(self.list, view).await?)
    }

    /// Fold one group shut or open it. Its entries leave the list when it
    /// folds, so the windows holding the group change and those below it move.
    pub async fn set_group_folded(
        &self,
        group: FolderReleaseDecisionKey,
        folded: bool,
    ) -> Result<(), ImportListFailure> {
        Ok(self.tables.set_import_list_group_folded(self.list, group, folded).await?)
    }

    /// Fold every group shut, or open them all, as one change.
    pub async fn set_all_groups_folded(&self, folded: bool) -> Result<(), ImportListFailure> {
        Ok(self.tables.set_import_list_groups_folded(self.list, folded).await?)
    }

    /// Apply a change a person made by pointing at rows of the list. The
    /// windows holding the rows it changed show it.
    pub async fn change_selection(&self, change: SelectionChange) -> Result<(), ImportListFailure> {
        match self.tables.change_import_list_selection(self.list, change).await? {
            SelectionWrite::Changed => Ok(()),
            SelectionWrite::RunNotShown => Err(ImportListFailure::RunNotShown),
        }
    }

    /// Select every row the list shows, whether or not a surface loaded it.
    pub async fn select_all(&self) -> Result<(), ImportListFailure> {
        Ok(self.tables.select_all_in_import_list(self.list).await?)
    }

    /// The selected rows, in the order the list shows them: what a bulk
    /// action runs over.
    pub async fn selected(&self) -> Result<SelectedRows, ImportListFailure> {
        Ok(self.tables.load_import_list_selected(self.list).await?)
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

    /// One window of the list as a live query whose request moves in place.
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

    /// How many items sit before the one at a key, whose key moves in place.
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

    /// What the selection holds and can be told to do.
    pub fn subscribe_selection(&self) -> coven::LiveQuery<SelectionSummary, DbError> {
        self.tables.subscribe_import_selection(self.list)
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
        query: impl LiveSource<T> + 'static,
    ) -> LiveValues<T> {
        LiveValues::new(query, self.closing.child_token(), self.bugs.clone())
    }

    /// Everything the list shows, in one read.
    #[cfg(any(test, feature = "test-utils"))]
    pub async fn read_for_test(&self) -> super::ImportListRead {
        self.tables.read_import_list_for_test(self.list).await.expect("the list reads")
    }

    /// Close the list: every live query opened from it ends, and its row,
    /// folds and selection go.
    pub async fn cancel(&self) -> Result<(), ImportListFailure> {
        self.closing.cancel();
        Ok(self.tables.close_import_list(self.list).await?)
    }
}
