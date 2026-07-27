use super::*;
use crate::util::rate_limiter::CallPriority;

impl ImportServiceHandle {
    /// Project an Unknown import candidate into a `ReleaseUserEdit` so the
    /// edit-metadata form can seed itself from what's on disk: the parsed CUE
    /// track layout for CUE-backed candidates, embedded tags for per-track-file
    /// ones. This backs "Add as Unknown" — the UI previews, then shows the editor
    /// for verification before commit.
    ///
    /// This is only the seed. The worker re-scans the folder and re-runs the same
    /// Unknown mapper at commit time, so for any field the user touched, their
    /// edits (carried by the command's `user_edit` overlay) are what wins.
    pub async fn preview_file_tags_for_folder(
        &self,
        folder: std::path::PathBuf,
    ) -> Result<crate::import::ReleaseUserEdit, crate::import::ImportError> {
        // Captured before `folder` moves into the scan — the album-title
        // fallback when no file carries an ALBUM tag.
        let folder_name = folder
            .file_name()
            .and_then(|n| n.to_str())
            .map(|s| s.to_string());

        let stored_bindings = self.library_manager.load_stored_sheet_bindings().await?;
        let categorized = tokio::task::spawn_blocking(move || {
            crate::import::folder_scanner::collect_release_candidate_files(
                &folder,
                &stored_bindings,
            )
        })
        .await
        .map_err(|e| crate::import::ImportError::Internal {
            detail: format!("folder scan task failed: {e}"),
        })??;

        let clock = self.library_manager.clock().clone();
        let ids = self.library_manager.ids().clone();
        tokio::task::spawn_blocking(move || {
            let parsed = crate::import::file_tag_mapper::map_unknown_candidate_to_db(
                &categorized,
                folder_name.as_deref(),
                clock.as_ref(),
                ids.as_ref(),
            )?;
            Ok(parsed_album_to_user_edit(&parsed))
        })
        .await
        .map_err(|e| crate::import::ImportError::Internal {
            detail: format!("unknown preview projection task failed: {e}"),
        })?
    }

    /// Build an import command and enqueue it. The worker sources the release
    /// itself: `prepare_release` for Exact / Approximate (reading the same LRU
    /// caches the UI's prefetch warmed), the candidate's local evidence for
    /// Unknown. Remote cover bytes don't ride the command either —
    /// `download_cover_art_bytes` consults the URL cache when the worker writes
    /// the cover.
    ///
    /// `identity_choice` carries the user's claim: Exact preserves the mapper's
    /// `source_release_id`, Approximate NULLs it, Unknown writes zero
    /// `release_identities` rows. `user_edit` is the confirmation page's optional
    /// overlay, whose fields override the seeded metadata.
    pub fn start_import(
        &self,
        candidate_key: &str,
        folder: std::path::PathBuf,
        selected_cover: Option<crate::import::types::CoverSelection>,
        storage_mode: StorageMode,
        pin: bool,
        identity_choice: crate::import::types::IdentityChoice,
        user_edit: Option<crate::import::types::ReleaseUserEdit>,
    ) -> Result<String, crate::import::ImportError> {
        let import_id = self.library_manager.ids().new_id();
        let command = ImportCommand {
            import_id: import_id.clone(),
            candidate_key: candidate_key.to_string(),
            folder,
            selected_cover,
            storage_mode,
            pin,
            identity_choice,
            user_edit,
        };

        self.send_command(command)?;
        Ok(import_id)
    }

    /// Validate a submitted Discogs key against Discogs, then persist it only if
    /// it isn't outright rejected. Validating first means a typo (401) never
    /// stores a bad key, while an offline/rate-limited save still stores the key
    /// optimistically so the user isn't blocked. See `DiscogsSaveOutcome`.
    pub async fn save_discogs_token(
        &self,
        token: &str,
    ) -> Result<DiscogsSaveOutcome, crate::import::ImportError> {
        use crate::config::DiscogsValidation;

        let client = DiscogsClient::new(token.to_string());
        match validation_from_validate_result(
            client.validate_token(CallPriority::Interactive).await,
        ) {
            DiscogsValidation::Valid => {
                self.persist_discogs_key(token, DiscogsValidation::Valid)?;
                Ok(DiscogsSaveOutcome::Valid)
            }
            DiscogsValidation::Unvalidated => {
                self.persist_discogs_key(token, DiscogsValidation::Unvalidated)?;
                Ok(DiscogsSaveOutcome::Unvalidated)
            }
            DiscogsValidation::Rejected => Ok(DiscogsSaveOutcome::Rejected),
        }
    }

    /// Write the key to the keyring and record its validation in config, as one
    /// atomic operation. The shared persist path for the two outcomes that keep
    /// the key.
    fn persist_discogs_key(
        &self,
        token: &str,
        validation: crate::config::DiscogsValidation,
    ) -> Result<(), crate::import::ImportError> {
        self.library_manager
            .set_discogs_key(token, validation)
            .map_err(|e| crate::import::ImportError::Config {
                detail: e.to_string(),
            })
    }

    /// Re-check a stored `Unvalidated` key when possible (app launch,
    /// settings-tab open). No-op when no key is stored or the key is already
    /// settled `Valid`/`Rejected`. A 401 marks it `Rejected`; success confirms
    /// it `Valid`; network/rate-limit leaves it `Unvalidated` to retry later.
    pub async fn revalidate_discogs_token(&self) -> Result<(), crate::import::ImportError> {
        use crate::config::DiscogsValidation;

        if self.library_manager.discogs_validation() != Some(DiscogsValidation::Unvalidated) {
            return Ok(());
        }
        let Some(client) = self.library_manager.discogs_client()? else {
            // The guard above established config says a key is stored and
            // Unvalidated, so no client here means the keyring has no matching
            // bytes — the two durable stores disagree. Our own writes cannot
            // produce this (see `set_discogs_key`/`clear_discogs_key`), so it is
            // external tampering or a torn write from before those existed. Fail
            // loudly rather than leave the key stuck Unvalidated behind a log line.
            return Err(crate::import::ImportError::Config {
                detail: "config says a Discogs key is stored but the keyring has none".to_string(),
            });
        };
        match validation_from_validate_result(
            client.validate_token(CallPriority::Interactive).await,
        ) {
            settled @ (DiscogsValidation::Valid | DiscogsValidation::Rejected) => self
                .library_manager
                .set_discogs_validation(settled)
                .map_err(|e| crate::import::ImportError::Config {
                    detail: e.to_string(),
                }),
            DiscogsValidation::Unvalidated => Ok(()),
        }
    }

    /// Remove the Discogs API token from the OS keyring and clear the
    /// stored-key hint.
    pub fn remove_discogs_token(&self) -> Result<(), crate::import::ImportError> {
        self.library_manager
            .clear_discogs_key()
            .map_err(|e| crate::import::ImportError::Config {
                detail: e.to_string(),
            })
    }

    /// Queue an import command and return its import_id for progress tracking.
    /// Returns immediately — all the work (metadata resolution, file discovery,
    /// track mapping, DB insertion) happens in the service worker.
    pub fn send_command(
        &self,
        command: ImportCommand,
    ) -> Result<String, crate::import::ImportError> {
        let import_id = command.import_id.clone();
        self.requests_tx
            .send(crate::import::service::ImportWorkerMessage::Import(command))
            .map_err(|_| crate::import::ImportError::Internal {
                detail: "Failed to queue import command".to_string(),
            })?;
        Ok(import_id)
    }

    /// Test helper: yield every `ImportProgress` whose `import_id` matches, for a
    /// test that drives one import and asserts on its progress sequence.
    /// Production consumers read the unified stream via `subscribe_events`.
    #[cfg(any(test, feature = "test-utils"))]
    pub fn subscribe_import(
        &self,
        import_id: String,
    ) -> tokio::sync::mpsc::UnboundedReceiver<ImportProgress> {
        let mut event_rx = self.event_tx.subscribe();
        let (tx, rx) = tokio::sync::mpsc::unbounded_channel();
        self.runtime_handle.spawn(async move {
            loop {
                match event_rx.recv().await {
                    Ok(ImportEvent::ImportProgress { progress, .. }) => {
                        if tx.is_closed() {
                            break;
                        }
                        let matches = match &progress {
                            ImportProgress::Preparing { import_id: iid, .. }
                            | ImportProgress::Started { import_id: iid, .. }
                            | ImportProgress::Progress { import_id: iid, .. }
                            | ImportProgress::Complete { import_id: iid, .. }
                            | ImportProgress::RemoteUploadQueued { import_id: iid, .. }
                            | ImportProgress::Failed { import_id: iid, .. } => *iid == import_id,
                        };
                        if matches && tx.send(progress).is_err() {
                            break;
                        }
                    }
                    Ok(_) => {
                        if tx.is_closed() {
                            break;
                        }
                    }
                    Err(tokio::sync::broadcast::error::RecvError::Lagged(n)) => {
                        warn!("Import progress lagged by {n} events");
                        continue;
                    }
                    Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
                }
            }
        });
        rx
    }

    /// Subscribe to the unified event channel.
    pub fn subscribe_events(&self) -> broadcast::Receiver<ImportEvent> {
        self.event_tx.subscribe()
    }
}
