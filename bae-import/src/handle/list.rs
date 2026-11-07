//! The import list, the scans and the candidates as a surface reads them
//! over the store.

use super::*;

impl ImportServiceHandle {
    /// Where a bug a surface's live query carries goes as it arrives: the
    /// crash reporter.
    fn bugs(&self) -> coven::task::BugSinkRef {
        self.services.diagnostics.bug_sink()
    }

    /// Open an import list showing `view` through `windows`, with nothing
    /// folded and nothing selected. Closing the library ends it.
    pub async fn open_list(
        &self,
        view: crate::ImportListView,
        windows: bae_store::LibraryPageWindows,
    ) -> Result<crate::LiveImportList, crate::ImportError> {
        Ok(crate::LiveImportList::open(
            self.services.tables.clone(),
            self.services.closing.child_token(),
            self.bugs(),
            view,
            windows,
        )
        .await?)
    }

    /// One read of a list showing `view` through `windows`, for a caller
    /// with no list open: a list opened for the read and closed after it.
    pub async fn load_list(
        &self,
        view: crate::ImportListView,
        windows: bae_store::LibraryPageWindows,
    ) -> Result<crate::ImportListProjection, crate::ImportError> {
        let list = self.services.tables.open_import_list(view).await?;
        let projection =
            self.services.tables.load_import_list(crate::ImportListRequest { list, windows }).await;
        self.services.tables.close_import_list(list).await?;
        Ok(projection?)
    }

    /// Where each watched folder's scan stands: the first value at once, then
    /// a value only when it changed, by what a scan stored or by what the
    /// folder scan coordinator holds of the scans. Closing the library ends
    /// it.
    pub fn subscribe_folder_scans(
        &self,
    ) -> bae_store::live_query::LiveValues<crate::FolderScanProgress> {
        bae_store::live_query::LiveValues::new(
            crate::folder_scan_status::FolderScansLive::new(
                self.services.tables.clone(),
                self.scans_in_memory.clone(),
            ),
            self.services.closing.child_token(),
            self.bugs(),
        )
    }

    /// How many folders each scan under way has stored, by its root: the
    /// first value at once, then a value after each write that moves one.
    /// Closing the library ends it.
    pub fn subscribe_scan_found_counts(
        &self,
    ) -> bae_store::live_query::LiveValues<crate::ScanFoundCounts> {
        bae_store::live_query::LiveValues::new(
            self.services.tables.subscribe_scan_found_counts(),
            self.services.closing.child_token(),
            self.bugs(),
        )
    }

    /// How far the identifications and imports in flight have got: the
    /// first value at once, then a value after each write that moves them.
    /// Closing the library ends it.
    pub fn subscribe_counts(&self) -> bae_store::live_query::LiveValues<crate::ImportCounts> {
        bae_store::live_query::LiveValues::new(
            self.services.tables.subscribe_import_counts(),
            self.services.closing.child_token(),
            self.bugs(),
        )
    }

    /// What is in flight for `key`: the first value at once, then a value
    /// after each write that changes it; `None` while nothing is. Closing the
    /// library ends it.
    pub fn subscribe_candidate_runtime(
        &self,
        key: &crate::CandidateKey,
    ) -> bae_store::live_query::LiveValues<Option<crate::CandidateRuntimeSnapshot>> {
        bae_store::live_query::LiveValues::new(
            self.services.tables.subscribe_candidate_runtime(key),
            self.services.closing.child_token(),
            self.bugs(),
        )
    }

    /// One candidate as the pane reads it, once, with its standing among
    /// the candidates.
    pub async fn load_candidate(
        &self,
        key: &crate::CandidateKey,
    ) -> Result<Option<crate::ImportCandidateDetail>, crate::ImportError> {
        Ok(self
            .services
            .tables
            .load_import_candidate(key)
            .await?
            .map(crate::ImportCandidateDetailProjection::resolve))
    }

    /// The pane's live read of the candidate the request names; `None` for
    /// no key or a key naming no release the list lists. Setting a new key
    /// points the same query at another candidate.
    pub fn subscribe_candidate(
        &self,
        initial: Option<crate::CandidateKey>,
    ) -> coven::ReconfigurableLiveQuery<
        Option<crate::CandidateKey>,
        Option<crate::ImportCandidateDetailProjection>,
        crate::ImportError,
    > {
        self.services.tables.subscribe_import_candidate(initial)
    }

    /// The live read of the folder that failed validation the request names;
    /// `None` for no key or a key naming no such folder. Setting a new key
    /// points the same query at another folder.
    pub fn subscribe_invalid_candidate(
        &self,
        initial: Option<crate::CandidateKey>,
    ) -> coven::ReconfigurableLiveQuery<
        Option<crate::CandidateKey>,
        Option<crate::InvalidCandidateDetail>,
        bae_store::DbError,
    > {
        self.services.tables.subscribe_invalid_candidate(initial)
    }

    /// The import list for `view` as one window over every item, for a test
    /// that watches it change.
    #[cfg(any(test, feature = "test-utils"))]
    pub async fn subscribe_whole_list(&self, view: crate::ImportListView) -> crate::LiveImportList {
        self.subscribe_list_window(
            view,
            bae_store::LibraryPageWindow { offset: 0, limit: u64::MAX },
        )
        .await
    }

    /// The import list for `view` through one window, for a test that
    /// watches what changes off the page.
    #[cfg(any(test, feature = "test-utils"))]
    pub async fn subscribe_list_window(
        &self,
        view: crate::ImportListView,
        window: bae_store::LibraryPageWindow,
    ) -> crate::LiveImportList {
        self.open_list(view, std::iter::once(window).collect())
            .await
            .expect("the import list opens")
    }

    /// The first import list `accept` admits, waiting through the query's
    /// values until one does. The list lands after the commit it reflects, so
    /// a test that just observed a scan event waits here for the read.
    #[cfg(any(test, feature = "test-utils"))]
    pub async fn wait_for_list(
        &self,
        view: crate::ImportListView,
        mut accept: impl FnMut(&crate::ImportListSnapshot) -> bool,
    ) -> crate::ImportListSnapshot {
        let subscription = self.subscribe_whole_list(view).await;
        loop {
            let snapshot = subscription.next().await.expect("the import list query stays open");
            if accept(&snapshot) {
                return snapshot;
            }
        }
    }
}

#[cfg(test)]
#[path = "list_tests.rs"]
mod tests;
