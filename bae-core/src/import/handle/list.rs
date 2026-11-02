//! The import list, the scans and the candidates as a surface reads them
//! over the store.

use super::*;

impl ImportServiceHandle {
    /// Open an import list showing `view` through `windows`, with nothing
    /// folded and nothing selected. Closing the library ends it.
    pub async fn open_list(
        &self,
        view: crate::import::ImportListView,
        windows: crate::db::LibraryPageWindows,
    ) -> Result<crate::import::LiveImportList, crate::import::ImportError> {
        Ok(crate::import::LiveImportList::open(
            self.tables.clone(),
            self.closing.child_token(),
            view,
            windows,
        )
        .await?)
    }

    /// One read of a list showing `view` through `windows`, for a caller
    /// with no list open: a list opened for the read and closed after it.
    pub async fn load_list(
        &self,
        view: crate::import::ImportListView,
        windows: crate::db::LibraryPageWindows,
    ) -> Result<crate::import::ImportListProjection, crate::import::ImportError> {
        let list = self.tables.open_import_list(view).await?;
        let projection =
            self.tables.load_import_list(crate::import::ImportListRequest { list, windows }).await;
        self.tables.close_import_list(list).await?;
        Ok(projection?)
    }

    /// Where each watched folder's scan stands: the first value at once, then
    /// a value only when the scans changed, which the live query tests on the
    /// processed progress. Closing the library ends it.
    pub fn subscribe_folder_scans(
        &self,
    ) -> crate::live_query::LiveValues<crate::import::FolderScanProgress> {
        crate::live_query::LiveValues::new(
            self.tables.subscribe_folder_scan_progress(),
            self.closing.child_token(),
        )
    }

    /// How far the identifications and imports in flight have got: the
    /// first value at once, then a value after each write that moves them.
    /// Closing the library ends it.
    pub fn subscribe_counts(&self) -> crate::live_query::LiveValues<crate::import::ImportCounts> {
        crate::live_query::LiveValues::new(
            self.tables.subscribe_import_counts(),
            self.closing.child_token(),
        )
    }

    /// What is in flight for `key`: the first value at once, then a value
    /// after each write that changes it; `None` while nothing is. Closing the
    /// library ends it.
    pub fn subscribe_candidate_runtime(
        &self,
        key: &crate::import::CandidateKey,
    ) -> crate::live_query::LiveValues<Option<crate::import::CandidateRuntimeSnapshot>> {
        crate::live_query::LiveValues::new(
            self.tables.subscribe_candidate_runtime(key),
            self.closing.child_token(),
        )
    }

    /// One candidate as the pane reads it, once, with its standing among
    /// the candidates.
    pub async fn load_candidate(
        &self,
        key: &crate::import::CandidateKey,
    ) -> Result<Option<crate::import::ImportCandidateDetail>, crate::import::ImportError> {
        Ok(self
            .tables
            .load_import_candidate(key)
            .await?
            .map(crate::import::ImportCandidateDetailProjection::resolve))
    }

    /// The pane's live read of the candidate the request names; `None` for
    /// no key or a key naming no release the list lists. Setting a new key
    /// points the same query at another candidate.
    pub fn subscribe_candidate(
        &self,
        initial: Option<crate::import::CandidateKey>,
    ) -> coven::ReconfigurableLiveQuery<
        Option<crate::import::CandidateKey>,
        Option<crate::import::ImportCandidateDetailProjection>,
    > {
        self.tables.subscribe_import_candidate(initial)
    }

    /// The live read of the folder that failed validation the request names;
    /// `None` for no key or a key naming no such folder. Setting a new key
    /// points the same query at another folder.
    pub fn subscribe_invalid_candidate(
        &self,
        initial: Option<crate::import::CandidateKey>,
    ) -> coven::ReconfigurableLiveQuery<
        Option<crate::import::CandidateKey>,
        Option<crate::import::InvalidCandidateDetail>,
    > {
        self.tables.subscribe_invalid_candidate(initial)
    }
}
