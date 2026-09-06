use super::*;
use crate::import::combination::{CombinationReview, CombinationTrackOrder};
use crate::import::ImportError;

impl ImportServiceHandle {
    pub async fn candidate_source_folders(&self, key: &str) -> Result<Vec<String>, ImportError> {
        let detail = self
            .library_manager
            .load_import_candidate(key)
            .await?
            .ok_or_else(|| ImportError::Internal {
                detail: format!("{key} is no longer a candidate"),
            })?;
        Ok(detail
            .candidate
            .source_folders()
            .into_iter()
            .map(|path| path.to_string_lossy().into_owned())
            .collect())
    }

    pub async fn review_candidate_combination(
        &self,
        keys: Vec<String>,
    ) -> Result<CombinationReview, ImportError> {
        let _commit = self.folder_state_commit.lock().await;
        let mut candidates = Vec::with_capacity(keys.len());
        for key in keys {
            self.ensure_combination_idle(&key)?;
            let crate::import::release_candidate::ReleaseCandidate::Folder(candidate) =
                self.editable_candidate_for_commit(&key).await?
            else {
                return Err(ImportError::Internal {
                    detail: "separate an existing combination before combining its folders again"
                        .into(),
                });
            };
            candidates.push(candidate);
        }
        CombinationReview::new(candidates)
    }

    pub async fn combine_reviewed_candidates(
        &self,
        review: &CombinationReview,
        keys: Vec<String>,
        order: CombinationTrackOrder,
        name: String,
    ) -> Result<String, ImportError> {
        let candidates = review.ordered_candidates(&keys)?;
        let _commit = self.folder_state_commit.lock().await;
        for key in &keys {
            self.ensure_combination_idle(key)?;
        }
        let key = format!("combination:{}", self.library_manager.new_id());
        self.library_manager
            .combine_candidates(key.clone(), name, candidates, order)
            .await?;
        for candidate_key in keys {
            send_event(
                &self.event_tx,
                ImportEvent::Scan(ScanEvent::CandidateRemoved { candidate_key }),
            );
        }
        send_event(
            &self.event_tx,
            ImportEvent::Scan(ScanEvent::CandidateMetadataChanged {
                candidate_key: key.clone(),
            }),
        );
        Ok(key)
    }

    pub async fn separate_combined_candidate(&self, key: &str) -> Result<(), ImportError> {
        let _commit = self.folder_state_commit.lock().await;
        self.ensure_combination_idle(key)?;
        let detail = self
            .library_manager
            .load_import_candidate(key)
            .await?
            .ok_or_else(|| ImportError::Internal {
                detail: format!("{key} is no longer a candidate"),
            })?;
        if detail.is_added {
            return Err(ImportError::CandidateAlreadyImported);
        }
        self.library_manager
            .separate_combined_candidate(key)
            .await?;
        send_event(
            &self.event_tx,
            ImportEvent::Scan(ScanEvent::CandidateRemoved {
                candidate_key: key.into(),
            }),
        );
        send_event(&self.event_tx, ImportEvent::Scan(ScanEvent::Finished));
        Ok(())
    }

    fn ensure_combination_idle(&self, key: &str) -> Result<(), ImportError> {
        if let Some(runtime) = self.runtime.get(key) {
            if runtime.import.is_some() {
                return Err(ImportError::CandidateImportInProgress);
            }
            if runtime
                .identify
                .as_ref()
                .is_some_and(|identify| !identify.is_terminal())
            {
                return Err(ImportError::Internal {
                    detail: format!("identification is still running for {key}"),
                });
            }
        }
        Ok(())
    }
}
