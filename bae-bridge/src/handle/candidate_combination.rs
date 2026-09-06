use super::*;
use crate::types::{BridgeCombinationPreview, BridgeCombinationTrackOrder};
use std::sync::Arc;

/// Retains the exact source revisions reviewed by the person combining them.
#[derive(uniffi::Object)]
pub struct CandidateCombinationReview {
    inner: bae_core::import::combination::CombinationReview,
    app: Arc<AppHandle>,
}

#[uniffi::export(async_runtime = "tokio", cancellable)]
impl AppHandle {
    pub async fn candidate_source_folders(
        self: Arc<Self>,
        key: String,
    ) -> Result<Vec<String>, BridgeError> {
        self.run_exported(move |this| async move {
            this.services
                .import_candidate_source_folders(&key)
                .await
                .map_err(BridgeError::import)
        })
        .await
    }

    pub async fn review_candidate_combination(
        self: Arc<Self>,
        keys: Vec<String>,
    ) -> Result<Arc<CandidateCombinationReview>, BridgeError> {
        self.run_exported(move |this| async move {
            let inner = this
                .services
                .import_review_combination(keys)
                .await
                .map_err(BridgeError::import)?;
            Ok(Arc::new(CandidateCombinationReview { inner, app: this }))
        })
        .await
    }

    pub async fn separate_combined_candidate(
        self: Arc<Self>,
        key: String,
    ) -> Result<(), BridgeError> {
        self.run_exported(move |this| async move {
            this.services
                .import_separate_combined_candidate(&key)
                .await
                .map_err(BridgeError::import)
        })
        .await
    }
}

#[uniffi::export(async_runtime = "tokio", cancellable)]
impl CandidateCombinationReview {
    pub fn candidate_keys(&self) -> Vec<String> {
        self.inner.candidate_keys()
    }

    pub fn preview(
        &self,
        keys: Vec<String>,
        order: BridgeCombinationTrackOrder,
    ) -> Result<BridgeCombinationPreview, BridgeError> {
        self.inner
            .preview(&keys, order.into_core())
            .map(BridgeCombinationPreview::from_core)
            .map_err(BridgeError::import)
    }

    pub async fn combine(
        self: Arc<Self>,
        keys: Vec<String>,
        order: BridgeCombinationTrackOrder,
        name: String,
    ) -> Result<String, BridgeError> {
        let app = self.app.clone();
        app.run_exported(move |this| async move {
            this.services
                .import_combine_reviewed_candidates(&self.inner, keys, order.into_core(), name)
                .await
                .map_err(BridgeError::import)
        })
        .await
    }
}
