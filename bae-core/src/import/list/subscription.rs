//! One list a surface has open: its row in the tables, and the live query
//! that reads it.
//!
//! Opening the list writes its row (`import_list`); its view, the groups it
//! folds and its selection are rows too, so changing any of them is a write,
//! and the live query, which reads them with the placement, reruns on it. The
//! windows a surface has loaded are the query's request, the one thing held
//! here. A write answers with the revision the read that reflects it carries,
//! so a surface can wait for that read.
//!
//! Where the folder scans stand is not here: a scan moves its found count
//! with every folder it walks, and that count moves no row, so the scans are
//! their own subscription and never resend the list.

use super::{
    ImportCandidateListLocation, ImportListId, ImportListProjection, ImportListRequest,
    ImportListSnapshot, ImportListView,
};
use crate::db::SelectionWrite;
use crate::import::selection::{SelectedRows, SelectionChange};
use crate::import::FolderReleaseDecisionKey;
use crate::library::{LibraryError, LibraryManager, LibraryPageWindows};
use crate::live_query::CancellableLiveQuery;

#[derive(Debug, thiserror::Error)]
pub enum ImportListSubscriptionError {
    #[error("import list subscription cancelled")]
    Cancelled,
    #[error("the list does not show both ends of the run of rows to select")]
    RunNotShown,
    #[error(transparent)]
    Query(#[from] coven::CovenError),
    #[error(transparent)]
    Library(#[from] LibraryError),
}

/// The live query ending is this subscription's cancellation.
impl From<crate::live_query::LiveQueryCancelled> for ImportListSubscriptionError {
    fn from(_: crate::live_query::LiveQueryCancelled) -> Self {
        Self::Cancelled
    }
}

/// One open list. Closing the library ends its live query before the store
/// closes. Dropped without [`Self::cancel`], its live query ends and its row
/// stays until the library next opens, when every list closes.
pub struct ImportListSubscription {
    list: ImportListId,
    manager: LibraryManager,
    query: CancellableLiveQuery<ImportListRequest, ImportListProjection>,
}

impl ImportListSubscription {
    /// Open a list showing `view` through `windows`, with nothing folded and
    /// nothing selected, and the live query that reads it.
    pub(crate) async fn open(
        manager: LibraryManager,
        view: ImportListView,
        windows: LibraryPageWindows,
    ) -> Result<Self, LibraryError> {
        let list = manager.open_import_list(view).await?;
        let query = manager
            .until_closed(manager.subscribe_import_list(ImportListRequest { list, windows }));
        Ok(Self {
            list,
            manager,
            query,
        })
    }

    /// Show a different tab, filter or order. The windows are kept: the query
    /// reruns and the surface re-reads them. Answers with the view revision.
    pub async fn set_view(&self, view: ImportListView) -> Result<u64, ImportListSubscriptionError> {
        Ok(self.manager.set_import_list_view(self.list, view).await?)
    }

    pub fn set_windows(
        &self,
        windows: LibraryPageWindows,
    ) -> Result<(), ImportListSubscriptionError> {
        self.query.set(ImportListRequest {
            list: self.list,
            windows,
        })?;
        Ok(())
    }

    /// Fold one group shut or open it. Its entries leave the list when it
    /// folds, so this changes what every later offset holds; answers with the
    /// view revision.
    pub async fn set_group_folded(
        &self,
        group: FolderReleaseDecisionKey,
        folded: bool,
    ) -> Result<u64, ImportListSubscriptionError> {
        Ok(self
            .manager
            .set_import_list_group_folded(self.list, group, folded)
            .await?)
    }

    /// Fold every group shut, or open them all, as one change.
    pub async fn set_all_groups_folded(
        &self,
        folded: bool,
    ) -> Result<u64, ImportListSubscriptionError> {
        Ok(self
            .manager
            .set_import_list_groups_folded(self.list, folded)
            .await?)
    }

    /// Apply a change a person made by pointing at rows of the list, and
    /// answer with the selection revision of the snapshot that reflects it.
    pub async fn change_selection(
        &self,
        change: SelectionChange,
    ) -> Result<u64, ImportListSubscriptionError> {
        match self
            .manager
            .change_import_list_selection(self.list, change)
            .await?
        {
            SelectionWrite::Changed { revision } => Ok(revision),
            SelectionWrite::RunNotShown => Err(ImportListSubscriptionError::RunNotShown),
        }
    }

    /// Select every row the list shows, whether or not a surface loaded it.
    pub async fn select_all(&self) -> Result<u64, ImportListSubscriptionError> {
        Ok(self.manager.select_all_in_import_list(self.list).await?)
    }

    /// The selected rows, in the order the list shows them: what a bulk
    /// action runs over.
    pub async fn selected(&self) -> Result<SelectedRows, ImportListSubscriptionError> {
        Ok(self.manager.load_import_list_selected(self.list).await?)
    }

    /// The view and position that reveal the candidate at `candidate_key`
    /// at its current placement: its tab, with the filters cleared and its
    /// group opened. `None` when the list lists no such candidate.
    pub async fn locate_candidate(
        &self,
        candidate_key: &str,
    ) -> Result<Option<ImportCandidateListLocation>, ImportListSubscriptionError> {
        Ok(self
            .manager
            .locate_import_candidate(self.list, candidate_key)
            .await?)
    }

    /// The first candidate the identification count is still waiting on —
    /// one queued, running or having its answer written — in the list's
    /// order. `None` when nothing is being identified, or nothing that is has
    /// a row.
    pub async fn first_identifying_candidate(
        &self,
    ) -> Result<Option<String>, ImportListSubscriptionError> {
        Ok(self
            .manager
            .first_identifying_import_candidate(self.list)
            .await?)
    }

    /// The list's next value.
    pub async fn next(&self) -> Result<ImportListSnapshot, ImportListSubscriptionError> {
        let event = self
            .query
            .next()
            .await
            .map_err(ImportListSubscriptionError::from)?;
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

    /// A handle that counts the list's live query runs, for a test that
    /// measures which writes rerun it.
    #[cfg(feature = "test-utils")]
    pub fn runs_for_test(&self) -> coven::LiveQueryRuns {
        self.query.runs()
    }

    /// End the live query and close the list: its row, folds and selection
    /// go.
    pub async fn cancel(&self) -> Result<(), ImportListSubscriptionError> {
        self.query.close().await;
        Ok(self.manager.close_import_list(self.list).await?)
    }
}
