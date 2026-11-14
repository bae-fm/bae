//! One list a surface has open: its row in the tables, and the live query
//! that reads it.
//!
//! Opening the list writes its row (`import_list`); its view, the groups it
//! folds and its selection are rows too, so changing any of them is a write,
//! and the live query, which reads them with the placement, reruns on it. The
//! windows a surface has loaded are the query's request, the one thing held
//! here, and every value carries each of them, read fresh. A write answers with the revision the read that reflects it carries,
//! so a surface can wait for that read.
//!
//! Where the folder scans stand is not here: a scan moves its found count
//! with every folder it walks, and that count moves no row, so the scans are
//! their own subscription and never resend the list.

use super::{
    ImportCandidateListLocation, ImportListId, ImportListProjection, ImportListRequest,
    ImportListSnapshot, ImportListView,
};
use crate::selection::{SelectedRows, SelectionChange};
use crate::tables::{ImportListError, ImportTables, SelectionWrite};
use crate::FolderReleaseDecisionKey;
use bae_store::live_query::CancellableLiveQuery;
use bae_store::DbError;
use bae_store::LibraryPageWindows;
use tokio_util::sync::CancellationToken;

#[derive(Debug, thiserror::Error)]
pub enum LiveImportListError {
    #[error("import list subscription cancelled")]
    Cancelled,
    #[error("the list does not show both ends of the run of rows to select")]
    RunNotShown,
    #[error(transparent)]
    Query(#[from] coven::CovenError),
    #[error(transparent)]
    Database(#[from] DbError),
}

/// A list found closed was closed by this subscription's own cancel, the one
/// write that closes a list a surface holds open.
impl From<ImportListError> for LiveImportListError {
    fn from(error: ImportListError) -> Self {
        match error {
            ImportListError::Closed(_) => Self::Cancelled,
            ImportListError::Database(error) => Self::Database(error),
        }
    }
}

/// The live query ending is this subscription's cancellation.
impl From<bae_store::live_query::LiveQueryCancelled> for LiveImportListError {
    fn from(_: bae_store::live_query::LiveQueryCancelled) -> Self {
        Self::Cancelled
    }
}

/// One open list. `closing` ends its live query when the library closes,
/// before the store does. Dropped without [`Self::cancel`], its live query
/// ends and its row stays until the library next opens, when every list
/// closes.
pub struct LiveImportList {
    list: ImportListId,
    tables: ImportTables,
    query: CancellableLiveQuery<ImportListRequest, ImportListProjection, ImportListError>,
}

impl LiveImportList {
    /// Open a list showing `view` through `windows`, with nothing folded and
    /// nothing selected, and the live query that reads it, which reports a
    /// bug a run carries to `bugs`.
    pub(crate) async fn open(
        tables: ImportTables,
        closing: CancellationToken,
        bugs: coven::task::BugSinkRef,
        view: ImportListView,
        windows: LibraryPageWindows,
    ) -> Result<Self, DbError> {
        let list = tables.open_import_list(view).await?;
        let query = CancellableLiveQuery::new(
            tables.subscribe_import_list(ImportListRequest { list, windows }),
            closing,
            bugs,
        );
        Ok(Self { list, tables, query })
    }

    /// Show a different tab, filter or order. The windows are kept: the query
    /// reruns and the surface re-reads them. Answers with the view revision.
    pub async fn set_view(&self, view: ImportListView) -> Result<u64, LiveImportListError> {
        Ok(self.tables.set_import_list_view(self.list, view).await?)
    }

    pub fn set_windows(&self, windows: LibraryPageWindows) -> Result<(), LiveImportListError> {
        self.query.set(ImportListRequest { list: self.list, windows })?;
        Ok(())
    }

    /// Fold one group shut or open it. Its entries leave the list when it
    /// folds, so this changes what every later offset holds; answers with the
    /// view revision.
    pub async fn set_group_folded(
        &self,
        group: FolderReleaseDecisionKey,
        folded: bool,
    ) -> Result<u64, LiveImportListError> {
        Ok(self.tables.set_import_list_group_folded(self.list, group, folded).await?)
    }

    /// Fold every group shut, or open them all, as one change.
    pub async fn set_all_groups_folded(&self, folded: bool) -> Result<u64, LiveImportListError> {
        Ok(self.tables.set_import_list_groups_folded(self.list, folded).await?)
    }

    /// Apply a change a person made by pointing at rows of the list, and
    /// answer with the selection revision of the snapshot that reflects it.
    pub async fn change_selection(
        &self,
        change: SelectionChange,
    ) -> Result<u64, LiveImportListError> {
        match self.tables.change_import_list_selection(self.list, change).await? {
            SelectionWrite::Changed { revision } => Ok(revision),
            SelectionWrite::RunNotShown => Err(LiveImportListError::RunNotShown),
        }
    }

    /// Select every row the list shows, whether or not a surface loaded it.
    pub async fn select_all(&self) -> Result<u64, LiveImportListError> {
        Ok(self.tables.select_all_in_import_list(self.list).await?)
    }

    /// The selected rows, in the order the list shows them: what a bulk
    /// action runs over.
    pub async fn selected(&self) -> Result<SelectedRows, LiveImportListError> {
        Ok(self.tables.load_import_list_selected(self.list).await?)
    }

    /// The view and position that reveal the candidate at `candidate_key`
    /// at its current placement: its tab, with the filters cleared and its
    /// group opened. `None` when the list lists no such candidate.
    pub async fn locate_candidate(
        &self,
        candidate_key: &crate::CandidateKey,
    ) -> Result<Option<ImportCandidateListLocation>, LiveImportListError> {
        Ok(self.tables.locate_import_candidate(self.list, candidate_key).await?)
    }

    /// The first candidate the identification count is still waiting on —
    /// one queued, running or having its answer written — in the list's
    /// order. `None` when nothing is being identified, or nothing that is has
    /// a row.
    pub async fn first_identifying_candidate(
        &self,
    ) -> Result<Option<crate::CandidateKey>, LiveImportListError> {
        Ok(self.tables.first_identifying_import_candidate(self.list).await?)
    }

    /// The list's next value.
    pub async fn next(&self) -> Result<ImportListSnapshot, LiveImportListError> {
        let event = self.query.next().await.map_err(LiveImportListError::from)?;
        let cause = event.cause;
        // A list read that fails takes the whole import tab with it — no
        // rows, no watched folders — so the reason is worth a line whether or
        // not anyone is on screen to be shown it.
        let projection = match event.result {
            Ok(projection) => projection,
            Err(error) => {
                tracing::error!("import list query failed ({cause:?}): {error}");
                return Err(error.into());
            }
        };
        let ImportListProjection {
            windows,
            total_count,
            summary,
            pending_filter_entries,
            selection,
            view_revision,
            selection_revision,
        } = projection;
        Ok(ImportListSnapshot {
            windows,
            total_count,
            summary,
            pending_filter_entries,
            selection_revision,
            selection,
            view_revision,
            cause,
        })
    }

    /// How often the list's live query had run when it last returned a
    /// value, for a test that measures which writes rerun it.
    #[cfg(any(test, feature = "test-utils"))]
    pub fn runs_for_test(&self) -> coven::LiveQueryRuns {
        self.query.runs()
    }

    /// End the live query and close the list: its row, folds and selection
    /// go.
    pub async fn cancel(&self) -> Result<(), LiveImportListError> {
        self.query.close().await;
        Ok(self.tables.close_import_list(self.list).await?)
    }
}
