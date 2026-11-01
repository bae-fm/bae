use super::*;

forward! { async this => {
    /// Record the person's answer to the candidate's artist conflict.
    fn choose_candidate_artist_identity(
        candidate_key: Vec<u8>,
        metadata_revision: u64,
        choice: crate::types::BridgeArtistIdentityConflictChoice,
    ) -> crate::types::BridgePaneOutcome {
        Ok(crate::types::BridgePaneOutcome::from_core(
            this.services
                .pane_choose_artist_identity(
                    &candidate_key.into(),
                    metadata_revision,
                    choice.into_core(),
                )
                .await?,
        ))
    }

    /// Record the person's answer to the candidate's label conflict.
    fn choose_candidate_label_identity(
        candidate_key: Vec<u8>,
        metadata_revision: u64,
        choice: crate::types::BridgeLabelIdentityConflictChoice,
    ) -> crate::types::BridgePaneOutcome {
        Ok(crate::types::BridgePaneOutcome::from_core(
            this.services
                .pane_choose_label_identity(
                    &candidate_key.into(),
                    metadata_revision,
                    choice.into_core(),
                )
                .await?,
        ))
    }
} }

#[uniffi::export(async_runtime = "tokio")]
impl AppHandle {
    /// Open an import list showing `view`, with nothing folded and nothing
    /// selected.
    pub async fn subscribe_import_list(
        &self,
        view: crate::types::BridgeImportListView,
    ) -> Result<std::sync::Arc<crate::ImportListSubscription>, BridgeError> {
        let runtime = self.runtime.clone();
        let services = self.services.clone();
        let diagnostics = self.diagnostics.clone();
        runtime
            .clone()
            .run("AppHandle.subscribe_import_list", move || async move {
                let inner = services
                    .subscribe_import_list(view.into_core())
                    .await
                    .map_err(BridgeError::from)?;
                Ok(crate::screens::import_list(
                    inner,
                    services,
                    runtime,
                    diagnostics,
                ))
            })
            .await
    }
}

#[uniffi::export]
impl AppHandle {
    /// Where each watched folder's scan stands, now and as it changes.
    pub fn subscribe_folder_scans(&self) -> std::sync::Arc<crate::FolderScansSubscription> {
        crate::screens::folder_scans(self.services.subscribe_folder_scans(), self.runtime.clone())
    }

    /// How far the identifications and imports in flight have got, now and
    /// as they change.
    pub fn subscribe_import_counts(&self) -> std::sync::Arc<crate::ImportCountsSubscription> {
        crate::screens::import_counts(
            self.services.subscribe_import_counts(),
            self.runtime.clone(),
        )
    }

    /// What one candidate has in flight — a run's identify state and the
    /// signals it last read, a running import's step, its typed search — now
    /// and as it changes; `None` while nothing is.
    pub fn subscribe_candidate_runtime(
        &self,
        candidate_key: Vec<u8>,
    ) -> std::sync::Arc<crate::CandidateRuntimeSubscription> {
        crate::screens::candidate_runtime(
            self.services
                .subscribe_candidate_runtime(&candidate_key.into()),
            self.runtime.clone(),
        )
    }
}

#[cfg(test)]
#[path = "import_list_tests.rs"]
mod tests;
