use super::*;
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

    /// Make the folders at `keys` one release. They play in key order — for
    /// siblings, the order their names sort in — each folder a disc run of its
    /// own, and the release takes the first folder's name. All three are the
    /// combined candidate's own draft afterwards, which the pane edits and
    /// "Separate Folders" undoes.
    pub async fn combine_candidates(&self, keys: Vec<String>) -> Result<String, ImportError> {
        let this = self.clone();
        self.committed(async move { this.combine_candidates_write(keys).await })
            .await
    }

    async fn combine_candidates_write(
        &self,
        mut keys: Vec<String>,
    ) -> Result<String, ImportError> {
        keys.sort();
        let _commit = self.folder_state_commit.lock().await;
        let mut candidates = Vec::with_capacity(keys.len());
        for key in &keys {
            self.ensure_combination_idle(key)?;
            let crate::import::release_candidate::ReleaseCandidate::Folder(candidate) =
                self.editable_candidate_for_commit(key).await?
            else {
                return Err(ImportError::Internal {
                    detail: "separate an existing combination before combining its folders again"
                        .into(),
                });
            };
            candidates.push(candidate);
        }
        let name = candidates
            .first()
            .ok_or_else(|| ImportError::Internal {
                detail: "combining a release requires at least two folders".into(),
            })?
            .name
            .clone();
        let key = format!("combination:{}", self.library_manager.new_id());
        self.library_manager
            .combine_candidates(key.clone(), name, candidates)
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
        let this = self.clone();
        let key = key.to_string();
        self.committed(async move { this.separate_combined_candidate_write(&key).await })
            .await
    }

    async fn separate_combined_candidate_write(&self, key: &str) -> Result<(), ImportError> {
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
            if runtime.queued.is_some() || runtime.running.is_some() {
                return Err(ImportError::Internal {
                    detail: format!("identification is still running for {key}"),
                });
            }
        }
        Ok(())
    }
}
