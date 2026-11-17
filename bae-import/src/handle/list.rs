//! The import list, the scans and the candidates as a surface reads them
//! over the store.

use super::*;

impl ImportServiceHandle {
    /// Where a bug a surface's live query carries goes as it arrives: the
    /// crash reporter.
    fn bugs(&self) -> coven::task::BugSinkRef {
        self.services.diagnostics.bug_sink()
    }

    /// Open an import list showing `view`, with nothing folded and nothing
    /// selected. Closing the library ends it.
    pub async fn open_list(
        &self,
        view: crate::ImportListView,
    ) -> Result<crate::LiveImportList, crate::ImportError> {
        Ok(crate::LiveImportList::open(
            self.services.tables.clone(),
            self.services.closing.child_token(),
            self.bugs(),
            view,
        )
        .await?)
    }

    /// Every item of the list showing `view`, once, for a caller with no
    /// list open: the function opens a list for the read and closes it after.
    pub async fn load_list(
        &self,
        view: crate::ImportListView,
    ) -> Result<Vec<crate::ImportListItem>, crate::ImportError> {
        let list = self.services.tables.open_import_list(view).await?;
        let window = bae_store::ListWindow::Forward { from: None, until: None, limit: u64::MAX };
        let read = self.services.tables.read_import_window(list, window).await;
        self.services.tables.close_import_list(list).await?;
        Ok(read?.read.rows)
    }

    /// Each watched folder's scan status: the first value at once, then a
    /// value after each write that changes it, whether a scan stored it, the
    /// folder scan coordinator wrote what it keeps for the scans, or the app
    /// marked an alert shown. Closing the library ends the subscription.
    pub fn subscribe_folder_scans(
        &self,
    ) -> bae_store::live_query::LiveValues<crate::FolderScanProgress> {
        bae_store::live_query::LiveValues::new(
            self.services.tables.subscribe_folder_scan_progress(),
            self.services.closing.child_token(),
            self.bugs(),
        )
    }

    /// The folders being scanned, each with how many folders its scan has
    /// stored: the first value at once, then a value after each write that
    /// changes one. Closing the library ends the subscription.
    pub fn subscribe_scanning_folders(
        &self,
    ) -> bae_store::live_query::LiveValues<Vec<crate::ScanningFolder>> {
        bae_store::live_query::LiveValues::new(
            self.services.tables.subscribe_scanning_folders(),
            self.services.closing.child_token(),
            self.bugs(),
        )
    }

    /// Record that the app raised the alert for `watched_folder_path`'s
    /// problem of `kind` with `detail`, so the folder scans list no alert
    /// for it until the folder's problem ends.
    pub async fn mark_folder_scan_alert_shown(
        &self,
        watched_folder_path: &std::path::Path,
        kind: crate::FolderScanAlertKind,
        detail: String,
    ) -> Result<(), crate::ImportError> {
        Ok(self
            .services
            .tables
            .mark_folder_scan_alert_shown(watched_folder_path, kind, detail)
            .await?)
    }

    /// How many entries each tab holds, for a test that waits on a scan: the
    /// first value at once, then a value after each write that moves one.
    /// Closing the library ends it. A screen reads each tab's count from its
    /// open import list's count instead of this stream.
    #[cfg(any(test, feature = "test-utils"))]
    pub fn subscribe_tab_counts(
        &self,
    ) -> bae_store::live_query::LiveValues<crate::TriageTabCounts> {
        bae_store::live_query::LiveValues::new(
            self.services.tables.subscribe_tab_counts(),
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

    /// The identify state `key`'s pane shows and what is in flight for it:
    /// the first value at once, then a value after each write that changes
    /// it. Closing the library ends it.
    pub fn subscribe_candidate_runtime(
        &self,
        key: &crate::CandidateKey,
    ) -> bae_store::live_query::LiveValues<crate::CandidateRuntimeView> {
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

    /// The live read of the images the lightbox of a candidate's folder
    /// shows; see `ImportTables::subscribe_folder_images`.
    #[cfg(desktop)]
    pub fn subscribe_folder_images(
        &self,
        initial: Option<crate::FolderImagesRequest>,
    ) -> coven::ReconfigurableLiveQuery<
        Option<crate::FolderImagesRequest>,
        Option<crate::FolderImages>,
        bae_store::DbError,
    > {
        self.services.tables.subscribe_folder_images(initial)
    }

    /// The first tab counts `accept` admits, waiting through the query's
    /// values until one does. The counts land after the commit they reflect,
    /// so a test that just observed a scan event waits here for the read.
    #[cfg(any(test, feature = "test-utils"))]
    pub async fn wait_for_tab_counts(
        &self,
        mut accept: impl FnMut(&crate::TriageTabCounts) -> bool,
    ) -> crate::TriageTabCounts {
        let counts = self.subscribe_tab_counts();
        loop {
            let counts = counts.next().await.expect("the tab counts query stays open");
            if accept(&counts) {
                return counts;
            }
        }
    }

    /// The list for `view` followed through one window over every item, for
    /// a test that watches it change.
    #[cfg(any(test, feature = "test-utils"))]
    pub async fn subscribe_whole_list(
        &self,
        view: crate::ImportListView,
    ) -> crate::list::FollowedImportList {
        self.follow_list(
            view,
            bae_store::ListWindow::Forward { from: None, until: None, limit: u64::MAX },
        )
        .await
    }

    /// The list for `view` followed through `window`, for a test that
    /// watches what changes off the window.
    #[cfg(any(test, feature = "test-utils"))]
    pub async fn follow_list(
        &self,
        view: crate::ImportListView,
        window: bae_store::ListWindow,
    ) -> crate::list::FollowedImportList {
        let list = self.open_list(view).await.expect("the import list opens");
        crate::list::FollowedImportList::follow(list, window)
    }

    /// The list for `view`, read through every row, once `accept` admits
    /// it. The query delivers the new list after the commit it reflects, so
    /// a test that just observed a scan event waits here for the read.
    #[cfg(any(test, feature = "test-utils"))]
    pub async fn wait_for_list(
        &self,
        view: crate::ImportListView,
        mut accept: impl FnMut(&crate::list::ImportListRead) -> bool,
    ) -> crate::list::ImportListRead {
        let list = self.subscribe_whole_list(view).await;
        loop {
            let read = list.next(Vec::new()).await;
            if accept(&read) {
                return read;
            }
        }
    }
}

#[cfg(test)]
#[path = "list_tests.rs"]
mod tests;
