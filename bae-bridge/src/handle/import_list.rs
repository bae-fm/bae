use super::*;

#[uniffi::export]
impl AppHandle {
    /// The import list the app reads and acts on: its reads take the view and
    /// folds the app shows in their request, and it hands out the progress of
    /// the bulk actions over the app's selection.
    pub fn import_list(&self) -> std::sync::Arc<crate::ImportList> {
        crate::screens::import_list(self.services.clone(), self.runtime.clone())
    }

    /// Where each watched folder's scan stands, now and as it changes.
    pub fn subscribe_folder_scans(&self) -> std::sync::Arc<crate::FolderScansSubscription> {
        crate::screens::folder_scans(self.services.subscribe_folder_scans(), self.runtime.clone())
    }

    /// The folders bae is scanning, each with how many folders its scan has
    /// stored, now and as they change.
    pub fn subscribe_scanning_folders(&self) -> std::sync::Arc<crate::ScanningFoldersSubscription> {
        crate::screens::scanning_folders(
            self.services.subscribe_scanning_folders(),
            self.runtime.clone(),
        )
    }

    /// How far the identifications and imports in flight have got, now and
    /// as they change.
    pub fn subscribe_import_counts(&self) -> std::sync::Arc<crate::ImportCountsSubscription> {
        crate::screens::import_counts(self.services.subscribe_import_counts(), self.runtime.clone())
    }

    /// A new object that follows one catalog apply the app shows: a pick, or
    /// the release editor applying a catalog release. The app subscribes to
    /// its step, then passes it as `apply` to `select_candidate_release` or
    /// `apply_release_edit_catalog`.
    pub fn catalog_apply(&self) -> std::sync::Arc<crate::BridgeCatalogApply> {
        crate::screens::catalog_apply(self.services.clone(), self.runtime.clone())
    }

    /// What one candidate has in flight — a run's identify state and the
    /// signals it last read, a running import's step, its typed search — now
    /// and as it changes; `None` while nothing is.
    pub fn subscribe_candidate_runtime(
        &self,
        candidate_key: Vec<u8>,
    ) -> std::sync::Arc<crate::CandidateRuntimeSubscription> {
        crate::screens::candidate_runtime(
            self.services.subscribe_candidate_runtime(&candidate_key.into()),
            self.runtime.clone(),
        )
    }
}

#[cfg(test)]
#[path = "import_list_tests.rs"]
mod tests;
