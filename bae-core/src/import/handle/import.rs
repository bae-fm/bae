use super::*;

impl ImportServiceHandle {
    /// Project an Unknown import candidate into a
    /// `ReleaseUserEdit` shape so the edit-metadata form can seed itself
    /// from what's on disk. CUE-backed candidates use the parsed CUE track
    /// layout; per-track-file candidates use embedded tags. Used by the
    /// "Add as Unknown" affordance: the user clicks the link, the UI calls this
    /// to preview, then shows the editor for verification before commit.
    ///
    /// The commit-side worker re-scans the same folder and runs the same
    /// Unknown mapper at commit time, so the user's edits — applied via the
    /// `user_edit` overlay on the import command — are the source of truth for
    /// fields they touched. This preview is the seed only.
    pub async fn preview_file_tags_for_folder(
        &self,
        folder: std::path::PathBuf,
    ) -> Result<crate::import::ReleaseUserEdit, String> {
        // Captured before `folder` moves into the scan — the album-title
        // fallback when no file carries an ALBUM tag.
        let folder_name = folder
            .file_name()
            .and_then(|n| n.to_str())
            .map(|s| s.to_string());

        let categorized = tokio::task::spawn_blocking(move || {
            crate::import::folder_scanner::collect_release_candidate_files(&folder)
        })
        .await
        .map_err(|e| format!("folder scan task failed: {e}"))??;

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
        .map_err(|e| format!("unknown preview projection task failed: {e}"))?
    }

    /// Build an import command and enqueue it. For Exact / Approximate
    /// the worker calls `prepare_release` itself to fetch and map the
    /// release — reading from the same LRU caches the UI's prefetch
    /// warmed up. For Unknown the worker reads the candidate's local evidence:
    /// CUE sheets for CUE-backed candidates, embedded tags for per-track-file
    /// candidates. Remote cover bytes are not threaded through the command;
    /// `download_cover_art_bytes` consults the URL cache when the worker writes
    /// the cover.
    ///
    /// `identity_choice` carries both the user's claim shape and the
    /// release reference (when applicable): Exact preserves the
    /// mapper's `source_release_id`, Approximate NULLs it, Unknown
    /// writes zero `release_identities` rows.
    ///
    /// `user_edit` is an optional overlay from the confirmation page;
    /// when present, fields override the seeded metadata.
    pub fn start_import(
        &self,
        candidate_key: &str,
        folder: std::path::PathBuf,
        selected_cover: Option<crate::import::types::CoverSelection>,
        storage_mode: StorageMode,
        pin: bool,
        identity_choice: crate::import::types::IdentityChoice,
        user_edit: Option<crate::import::types::ReleaseUserEdit>,
    ) -> Result<String, String> {
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
    pub async fn save_discogs_token(&self, token: &str) -> Result<DiscogsSaveOutcome, String> {
        use crate::config::DiscogsValidation;

        let client = DiscogsClient::new(token.to_string());
        match validation_from_validate_result(client.validate_token().await) {
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

    /// Write the key to the keyring and record its validation in config. The
    /// shared persist path for the two outcomes that keep the key.
    fn persist_discogs_key(
        &self,
        token: &str,
        validation: crate::config::DiscogsValidation,
    ) -> Result<(), String> {
        self.library_manager
            .save_discogs_key(token)
            .map_err(|e| e.to_string())?;
        self.library_manager
            .set_discogs_key_stored(validation)
            .map_err(|e| e.to_string())?;
        Ok(())
    }

    /// Re-check a stored `Unvalidated` key when possible (app launch,
    /// settings-tab open). No-op when no key is stored or the key is already
    /// settled `Valid`/`Rejected`. A 401 marks it `Rejected`; success confirms
    /// it `Valid`; network/rate-limit leaves it `Unvalidated` to retry later.
    pub async fn revalidate_discogs_token(&self) -> Result<(), String> {
        use crate::config::DiscogsValidation;

        if self.library_manager.discogs_validation() != Some(DiscogsValidation::Unvalidated) {
            return Ok(());
        }
        let Some(client) = self.library_manager.discogs_client()? else {
            // A stored `Unvalidated` key (the guard above) with no client means
            // the keyring entry and the config disagree — surface it rather than
            // silently leaving the key stuck unvalidated.
            warn!(
                "revalidate skipped: config says a Discogs key is stored but the keyring has none"
            );
            return Ok(());
        };
        match validation_from_validate_result(client.validate_token().await) {
            settled @ (DiscogsValidation::Valid | DiscogsValidation::Rejected) => self
                .library_manager
                .set_discogs_validation(settled)
                .map_err(|e| e.to_string()),
            DiscogsValidation::Unvalidated => Ok(()),
        }
    }

    /// Remove the Discogs API token from the OS keyring and clear the
    /// stored-key hint.
    pub fn remove_discogs_token(&self) -> Result<(), String> {
        self.library_manager
            .delete_discogs_key()
            .map_err(|e| e.to_string())?;
        self.library_manager
            .clear_discogs_key_stored()
            .map_err(|e| e.to_string())?;
        Ok(())
    }

    /// Queue an import command and return the import_id for progress tracking.
    ///
    /// All heavy work (metadata resolution, file discovery, track mapping,
    /// DB insertion) happens in the service worker. This returns immediately.
    pub fn send_command(&self, command: ImportCommand) -> Result<String, String> {
        let import_id = command.import_id.clone();
        self.requests_tx
            .send(command)
            .map_err(|_| "Failed to queue import command".to_string())?;
        Ok(import_id)
    }

    /// Subscribe to progress updates for a specific release
    /// Returns a filtered receiver that yields only updates for the specified release
    pub fn subscribe_release(
        &self,
        release_id: String,
    ) -> tokio::sync::mpsc::UnboundedReceiver<ImportProgress> {
        self.progress_handle.subscribe_release(release_id)
    }

    /// Subscribe to progress updates for a specific import operation
    /// Returns Preparing events and any event with matching import_id
    pub fn subscribe_import(
        &self,
        import_id: String,
    ) -> tokio::sync::mpsc::UnboundedReceiver<ImportProgress> {
        self.progress_handle.subscribe_import(import_id)
    }

    /// Subscribe to progress updates for ALL import operations
    /// Returns any event that has an import_id (for toolbar dropdown)
    pub fn subscribe_all_imports(&self) -> tokio::sync::mpsc::UnboundedReceiver<ImportProgress> {
        self.progress_handle.subscribe_all_imports()
    }

    /// Subscribe to the unified event channel.
    pub fn subscribe_events(&self) -> broadcast::Receiver<ImportEvent> {
        self.event_tx.subscribe()
    }
}
