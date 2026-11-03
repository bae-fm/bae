use super::*;
use bae_catalogs::rate_limiter::CallPriority;

/// How an import was asked for, which decides whether a running
/// identification refuses it.
#[derive(Clone, Copy)]
enum ImportRequest {
    /// This candidate itself, by a person or by its just-settled automatic run;
    /// the claim ends whatever identification it had.
    Candidate,
    /// One row of a bulk import of a selection; refused while the row is being
    /// identified.
    Selection,
}

/// What asking to import a candidate did.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ImportStart {
    /// The import is queued under this id.
    Queued { import_id: String },
    /// The release the candidate's run picked unattended is still being read
    /// in full. Nothing is queued yet: the candidate imports once that read
    /// stores the pick, and not at all if it stores none.
    WhenPickStored,
}

impl ImportStart {
    /// The queued import's id, for a test that asked while nothing was being
    /// read.
    #[cfg(any(test, feature = "test-utils"))]
    pub fn expect_queued(self) -> String {
        match self {
            Self::Queued { import_id } => import_id,
            Self::WhenPickStored => panic!("the import waits for a pick being read"),
        }
    }
}

impl ImportServiceHandle {
    /// Queue an import of the candidate as stored, ending its identification
    /// — unless that identification is reading the release it picked
    /// unattended, which the import then waits for.
    pub async fn start_import(
        &self,
        candidate_key: &crate::CandidateKey,
    ) -> Result<ImportStart, crate::ImportError> {
        let this = self.clone();
        let candidate_key = candidate_key.clone();
        self.committed(async move {
            this.start_import_write(&candidate_key, ImportRequest::Candidate).await
        })
        .await
    }

    /// [`Self::start_import`] for one row of a bulk import, refused while that
    /// row is being identified, other than reading the release its run
    /// picked unattended.
    pub async fn import_selected(
        &self,
        candidate_key: &crate::CandidateKey,
    ) -> Result<ImportStart, crate::ImportError> {
        let this = self.clone();
        let candidate_key = candidate_key.clone();
        self.committed(async move {
            this.start_import_write(&candidate_key, ImportRequest::Selection).await
        })
        .await
    }

    /// [`Self::start_import`] for a candidate an identification run just
    /// settled as auto-importable. Nobody sees a refusal, so it is recorded as
    /// the candidate's failed import.
    pub(crate) async fn import_identified(
        &self,
        candidate_key: &crate::CandidateKey,
    ) -> Result<ImportStart, crate::ImportError> {
        let this = self.clone();
        let candidate_key = candidate_key.clone();
        self.committed(async move {
            let started = this.start_import_write(&candidate_key, ImportRequest::Candidate).await;
            match &started {
                Ok(_) => {}
                // Another import owns the candidate and reports how it went.
                Err(crate::ImportError::CandidateImportInProgress) => {}
                Err(error) => this.record_failed_start(&candidate_key, error).await,
            }
            started
        })
        .await
    }

    /// Record a refused start as `candidate_key`'s failed import.
    async fn record_failed_start(
        &self,
        candidate_key: &crate::CandidateKey,
        error: &crate::ImportError,
    ) {
        let candidate = match self.get_release_candidate(candidate_key).await {
            Ok(Some(candidate)) => candidate,
            Ok(None) => {
                warn!("{candidate_key} is gone; its refused import has no row to show on");
                return;
            }
            Err(read) => {
                tracing::error!(
                    "could not read {candidate_key} to record its refused import: {read}"
                );
                return;
            }
        };
        let failure =
            crate::service::ImportService::terminal_failure(error, self.clock.now().into());
        if let Err(write) = self
            .tables
            .save_import_candidate_failure(
                &candidate.files.content_hash(),
                candidate.file_edit_revision,
                &failure,
            )
            .await
        {
            tracing::error!("could not record the refused import of {candidate_key}: {write}");
        }
    }

    async fn start_import_write(
        &self,
        candidate_key: &crate::CandidateKey,
        request: ImportRequest,
    ) -> Result<ImportStart, crate::ImportError> {
        let commit = self.folder_state_commit.lock("start an import").await;
        let Some(candidate) = self.get_release_candidate(candidate_key).await? else {
            return Err(crate::ImportError::Internal {
                detail: format!("{candidate_key} is not a scanned folder candidate"),
            });
        };
        // Refused for every request kind: an import owns it, or this device
        // already imported its files, and one folder hash is one release here.
        self.candidate_standing(candidate_key, &candidate).await?.editable()?;
        if let Some(release_id) =
            self.tables.release_imported_from(&candidate.files.content_hash()).await?
        {
            return Err(crate::ImportError::FilesAlreadyImported { release_id });
        }
        // Asked under the lock the verdict write ends its answer under, so the
        // ask is on the answer that write carries out, or the write has ended
        // it and what follows imports the stored pick. Every request kind
        // waits: the read is not cancelled, and the draft it replaces is not
        // imported.
        if self.runtime.ask_import_once_stored(candidate_key).await? {
            info!("{candidate_key} imports once the release its run picked is read and stored");
            return Ok(ImportStart::WhenPickStored);
        }
        match request {
            ImportRequest::Candidate => {}
            ImportRequest::Selection => {
                if self.runtime.standing_facts(candidate_key).await?.identifying() {
                    return Err(crate::ImportError::CandidateBeingIdentified);
                }
            }
        }
        let content_hash = candidate.files.content_hash();
        let preparation = crate::preparation::import_preparation(&self.tables, &content_hash)
            .await?
            .filter(|preparation| preparation.file_edit_revision == candidate.file_edit_revision)
            .ok_or_else(|| crate::ImportError::Internal {
                detail: format!(
                    "{candidate_key} has no complete preparation for its current files"
                ),
            })?;
        let metadata_provenance = preparation.metadata_provenance.clone();
        let needs_file_tag_snapshot =
            matches!(metadata_provenance, Some(crate::MetadataProvenance::FileMetadata))
                || matches!(preparation.cover, Some(crate::CoverSelection::Embedded(_)));
        let file_tag_snapshot = if needs_file_tag_snapshot {
            let Some(stored) = self
                .tables
                .load_candidate_file_tag_snapshot(&candidate.watched_folder_path, candidate_key)
                .await?
            else {
                return Err(crate::ImportError::Internal {
                    detail: format!("{candidate_key} is not an actionable folder candidate"),
                });
            };
            let crate::tables::DbCandidateFileTagSnapshot {
                scan_generation,
                candidate: snapshot_candidate,
                snapshot,
            } = stored;
            if snapshot_candidate.files.content_hash() != content_hash
                || snapshot_candidate.file_edit_revision != candidate.file_edit_revision
            {
                return Err(crate::ImportError::FileTags {
                    detail: format!(
                        "{candidate_key} changed after its file tags were read; open it again"
                    ),
                });
            }
            let Some(snapshot) = snapshot else {
                return Err(crate::ImportError::FileTags {
                    detail: format!(
                        "{candidate_key}'s file tags have not been read; open it again"
                    ),
                });
            };
            if snapshot.scan_generation != scan_generation
                || snapshot.file_edit_revision != snapshot_candidate.file_edit_revision
            {
                return Err(crate::ImportError::FileTags {
                    detail: format!(
                        "{candidate_key} changed after its file tags were read; open it again"
                    ),
                });
            }
            Some(snapshot)
        } else {
            None
        };
        let import_id = self.ids.new_id().to_string();
        let expectation = crate::service::ImportExpectation {
            candidate: crate::CandidateAsRead {
                content_hash: content_hash.clone(),
                file_edit_revision: candidate.file_edit_revision,
                metadata_revision: preparation.metadata_revision,
            },
            file_tag_snapshot,
        };
        let command = ImportCommand {
            import_id: import_id.clone(),
            candidate_key: candidate_key.clone(),
            source: candidate.source(),
            #[cfg(any(test, feature = "test-utils"))]
            selected_cover: None,
            destination: crate::ImportDestination::of(
                bae_preferences::preferences(&self.database).await?.import_storage,
                self.database.storage_config().await?.is_some(),
            ),
            #[cfg(any(test, feature = "test-utils"))]
            pick: None,
            #[cfg(any(test, feature = "test-utils"))]
            read_file_tags: false,
            #[cfg(any(test, feature = "test-utils"))]
            user_edit: None,
        };

        // Claimed under the lock the standing was read under, so no second
        // import can claim these files in between. Once claimed, the
        // candidate is not identification's to answer: the claim ends its
        // run in the same write, and its work stops.
        self.runtime.claim_for_import(candidate_key, &import_id, true).await?;
        self.identify.cancel(candidate_key);
        self.extraction.cancel(candidate_key);
        drop(commit);
        self.send_claimed_command(command, expectation).await?;
        Ok(ImportStart::Queued { import_id })
    }

    /// Store the person's artist answer while this candidate is editable.
    pub async fn choose_candidate_artist_identity(
        &self,
        candidate_key: &crate::CandidateKey,
        metadata_revision: u64,
        choice: crate::ArtistIdentityConflictChoice,
    ) -> Result<(), crate::ImportError> {
        let this = self.clone();
        let key = candidate_key.clone();
        self.committed(async move {
            let _commit = this.folder_state_commit.lock("choose an import artist").await;
            let candidate = this.get_release_candidate(&key).await?.ok_or_else(|| {
                crate::ImportError::Internal {
                    detail: format!("{key} is not a scanned folder candidate"),
                }
            })?;
            this.candidate_standing(&key, &candidate).await?.editable()?;
            this.tables
                .choose_import_artist_identity(
                    &candidate.files.content_hash(),
                    metadata_revision,
                    choice,
                    this.ids.as_ref(),
                )
                .await?;
            Ok(())
        })
        .await
    }

    /// Store the person's label answer while this candidate is editable.
    pub async fn choose_candidate_label_identity(
        &self,
        candidate_key: &crate::CandidateKey,
        metadata_revision: u64,
        choice: crate::LabelIdentityConflictChoice,
    ) -> Result<(), crate::ImportError> {
        let this = self.clone();
        let key = candidate_key.clone();
        self.committed(async move {
            let _commit = this.folder_state_commit.lock("choose an import label").await;
            let candidate = this.get_release_candidate(&key).await?.ok_or_else(|| {
                crate::ImportError::Internal {
                    detail: format!("{key} is not a scanned folder candidate"),
                }
            })?;
            this.candidate_standing(&key, &candidate).await?.editable()?;
            this.tables
                .choose_import_label_identity(
                    &candidate.files.content_hash(),
                    metadata_revision,
                    choice,
                    this.ids.as_ref(),
                )
                .await?;
            Ok(())
        })
        .await
    }

    /// Check a submitted Discogs key against Discogs and store it unless
    /// Discogs rejects it; a key that could not be checked is stored as
    /// unvalidated.
    pub async fn save_discogs_token(
        &self,
        token: &str,
    ) -> Result<DiscogsSaveOutcome, crate::ImportError> {
        let check =
            DiscogsCheck::of(self.catalogs.try_discogs_key(token, CallPriority::Interactive).await);
        let validation = bae_preferences::DiscogsValidation::of(&check);
        match check {
            DiscogsCheck::Valid => {
                self.persist_discogs_key(token, validation).await?;
                Ok(DiscogsSaveOutcome::Valid)
            }
            DiscogsCheck::Unchecked(error) => {
                self.persist_discogs_key(token, validation).await?;
                Ok(DiscogsSaveOutcome::Unvalidated(error))
            }
            DiscogsCheck::Rejected => Ok(DiscogsSaveOutcome::Rejected),
        }
    }

    /// Store the key and record its validation.
    async fn persist_discogs_key(
        &self,
        token: &str,
        validation: bae_preferences::DiscogsValidation,
    ) -> Result<(), crate::ImportError> {
        Ok(self.catalogs.store_discogs_key(token, validation).await?)
    }

    /// Check a stored `Unvalidated` key against Discogs again; any other
    /// state, or no key, is left alone. A check Discogs could not answer
    /// leaves the key unvalidated and fails with why, so the person who asked
    /// is told rather than shown the same state again.
    pub async fn revalidate_discogs_token(&self) -> Result<(), crate::ImportError> {
        use bae_catalogs::providers::DiscogsCheck;
        use bae_preferences::DiscogsValidation;

        if bae_preferences::preferences(&self.database).await?.discogs
            != Some(DiscogsValidation::Unvalidated)
        {
            return Ok(());
        }
        let check = self.catalogs.session().await?.validate().await.ok_or_else(|| {
            crate::ImportError::Internal {
                detail: "config says a Discogs key is stored but the keyring has none".to_string(),
            }
        })?;
        let validation = DiscogsValidation::of(&check);
        bae_preferences::edit_preferences(&self.database, move |prefs| {
            if prefs.discogs.is_some() {
                prefs.discogs = Some(validation);
            }
            Ok(())
        })
        .await
        .map_err(crate::DiscogsKeyError::from)?;
        match check {
            DiscogsCheck::Valid | DiscogsCheck::Rejected => Ok(()),
            DiscogsCheck::Unchecked(error) => Err(bae_catalogs::CatalogError::from(error).into()),
        }
    }

    /// Remove the stored Discogs key and its validation.
    /// The library's stored Discogs key, as the settings screen shows it.
    pub async fn discogs_token(&self) -> Result<Option<String>, crate::ImportError> {
        Ok(self.catalogs.stored_discogs_key().await?)
    }

    pub async fn remove_discogs_token(&self) -> Result<(), crate::ImportError> {
        Ok(self.catalogs.clear_discogs_key().await?)
    }

    /// Store `command`'s folder as a scanned candidate carrying the command's
    /// metadata and cover, then queue its import and return the import ID.
    #[cfg(all(any(test, feature = "test-utils"), desktop))]
    pub async fn send_command(
        &self,
        mut command: ImportCommand,
    ) -> Result<String, crate::ImportError> {
        let crate::release_candidate::CandidateSource { path: folder, scope, .. } =
            command.source.clone();
        let categorized = crate::folder_scanner::collect_release_candidate_files_with_scope(
            &folder,
            scope,
            &crate::folder_scanner::StoredCandidateEdits::none(),
        )?;
        let watched_folder = bae_db::watched_path::canonical_absolute_root(&folder)?;
        let candidate_key = crate::CandidateKey::folder(&watched_folder);
        let candidate_name =
            coven::path_text(std::path::Path::new(folder.file_name().ok_or_else(|| {
                crate::ImportError::Internal {
                    detail: format!(
                        "test import folder has no directory name: {}",
                        coven::path_text(&folder)
                    ),
                }
            })?));
        self.tables.add_watched_import_folder(&watched_folder).await?;
        let volume = crate::volume::volume_kind(&watched_folder, &self.diagnostics).await;
        let generation = self.tables.begin_folder_scan(&watched_folder, volume).await?;
        let candidate = crate::folder_scanner::FolderCandidate {
            path: watched_folder.clone(),
            file_root: watched_folder.clone(),
            name: candidate_name,
            files: categorized.clone(),
            watched_folder_path: watched_folder.clone(),
            scope,
            file_edit_revision: 0,
            relative_folder: bae_db::RelativeFolder::root(),
            grouping: None,
        };
        let item = crate::folder_scanner::ScanItem::Valid(candidate.clone());
        let seed = crate::file_metadata_seed::scan_item_seed(
            &self.tables,
            &self.diagnostics,
            &item,
            generation,
            Arc::new(crate::folder_scanner::FileReads::new(self.file_tags.clone())),
        )
        .await?;
        if self
            .tables
            .save_folder_scan_items(
                &watched_folder,
                generation,
                vec![crate::tables::ScanItemToWrite { item, seed, folder_date: None }],
            )
            .await?
            .is_none()
        {
            return Err(crate::ImportError::Internal {
                detail: format!("test import scan for {candidate_key} was superseded"),
            });
        }
        if self.tables.finish_folder_scan(&watched_folder, generation, None).await?.is_none() {
            return Err(crate::ImportError::Internal {
                detail: format!("test import scan for {candidate_key} was superseded"),
            });
        }
        command.candidate_key = candidate_key;
        let content_hash = categorized.content_hash();
        if let Some(link) = command.pick.clone() {
            self.pick_candidate_release_write(
                command.candidate_key.clone(),
                link,
                &crate::CatalogApplySteps::unread(),
            )
            .await?;
        }
        if command.read_file_tags {
            self.read_candidate_file_tags_write(command.candidate_key.clone()).await?;
        }
        if let Some(edit) = command.user_edit.clone() {
            let state =
                self.tables.load_import_candidate_state(&content_hash).await?.ok_or_else(|| {
                    crate::ImportError::Internal {
                        detail: "test import has no candidate state".into(),
                    }
                })?;
            let rows = self.tables.load_import_candidate_pane_rows(&content_hash).await?;
            let assets = self.tables.load_import_candidate_prepared_assets(&content_hash).await?;
            let draft = crate::pane::edit_over_audio(
                crate::RawReleaseEdit::from_user_edit(edit, "test-import-track"),
                &rows.draft,
            )?;
            self.preparations
                .apply_source(
                    &candidate.watched_folder_path,
                    &crate::CandidateAsRead {
                        content_hash: content_hash.clone(),
                        file_edit_revision: candidate.file_edit_revision,
                        metadata_revision: state.metadata_revision,
                    },
                    &command.candidate_key,
                    &crate::CandidateMetadataDraft {
                        draft,
                        // An edit of the draft the command applied, which
                        // stays read from where it was; with none applied,
                        // the draft is typed in, read from nowhere.
                        provenance: state
                            .metadata_provenance
                            .filter(|_| command.pick.is_some() || command.read_file_tags),
                        cover: rows.cover,
                        assets,
                    },
                )
                .await?;
        }
        if let Some(cover) = command.selected_cover.clone() {
            self.set_candidate_cover(&command.candidate_key, cover).await?;
        }
        let metadata_revision = self
            .tables
            .load_import_candidate_state(&content_hash)
            .await?
            .ok_or_else(|| crate::ImportError::Internal {
                detail: "test import has no candidate state".into(),
            })?
            .metadata_revision;
        let file_tag_snapshot = if command.read_file_tags
            || matches!(command.selected_cover, Some(crate::CoverSelection::Embedded(_)))
        {
            let snapshot = self
                .tables
                .load_candidate_file_tag_snapshot(
                    &candidate.watched_folder_path,
                    &command.candidate_key,
                )
                .await?
                .and_then(|stored| stored.snapshot)
                .ok_or_else(|| crate::ImportError::FileTags {
                    detail: "test import has no prepared file-tag snapshot".into(),
                })?;
            Some(snapshot)
        } else {
            None
        };
        let expectation = crate::service::ImportExpectation {
            candidate: crate::CandidateAsRead {
                content_hash,
                file_edit_revision: candidate.file_edit_revision,
                metadata_revision,
            },
            file_tag_snapshot,
        };
        self.send_command_with_expectation(command, expectation).await
    }

    /// Claim the candidate under the folder-state commit lock, so an
    /// identification verdict for it either landed first or is refused, then
    /// queue the import.
    #[cfg(any(test, feature = "test-utils"))]
    async fn send_command_with_expectation(
        &self,
        command: ImportCommand,
        expectation: crate::service::ImportExpectation,
    ) -> Result<String, crate::ImportError> {
        let import_id = command.import_id.clone();
        let candidate_key = command.candidate_key.clone();
        let commit = self.folder_state_commit.lock("queue a test import").await;
        self.runtime.claim_for_import(&candidate_key, &import_id, false).await?;
        drop(commit);
        self.send_claimed_command(command, expectation).await?;
        Ok(import_id)
    }

    /// A command the worker cannot take releases its claim, so no candidate is
    /// left owned by an import that does not exist.
    async fn send_claimed_command(
        &self,
        command: ImportCommand,
        expectation: crate::service::ImportExpectation,
    ) -> Result<(), crate::ImportError> {
        let candidate_key = command.candidate_key.clone();
        let import_id = command.import_id.clone();
        self.import_cancels.register(&candidate_key, &import_id);
        if self
            .worker
            .send(crate::service::ImportWorkerMessage::Import { command, expectation })
            .is_err()
        {
            self.import_cancels.forget(&candidate_key, &import_id);
            self.release_import_claim(&candidate_key, &import_id).await?;
            return Err(crate::ImportError::Internal {
                detail: "Failed to queue import command".to_string(),
            });
        }
        Ok(())
    }

    /// Every `ImportProgress` of `import_id` from its start, none dropped.
    #[cfg(any(test, feature = "test-utils"))]
    pub fn subscribe_import(
        &self,
        import_id: String,
    ) -> tokio::sync::mpsc::UnboundedReceiver<ImportProgress> {
        let mut events = self.event_tx.every_event_from_start();
        let (tx, rx) = tokio::sync::mpsc::unbounded_channel();
        self.runtime_handle.spawn(async move {
            while let Some(event) = events.recv().await {
                if tx.is_closed() {
                    break;
                }
                if let ImportEvent::ImportProgress { progress, .. } = event {
                    if progress.import_id() == import_id && tx.send(progress).is_err() {
                        break;
                    }
                }
            }
        });
        rx
    }

    /// Every import event since the service started, for the one
    /// identification queue that takes it (see
    /// [`super::ImportEventBus::take_feed`]).
    pub(crate) fn take_event_feed(&self) -> Option<mpsc::UnboundedReceiver<ImportEvent>> {
        self.event_tx.take_feed()
    }

    /// Every import event from now on, none dropped, for a test to wait on.
    #[cfg(any(test, feature = "test-utils"))]
    pub fn every_event(&self) -> mpsc::UnboundedReceiver<ImportEvent> {
        self.event_tx.every_event()
    }

    /// Every scan event from now on, none dropped, for a test to wait on.
    #[cfg(any(test, feature = "test-utils"))]
    pub fn every_scan_event_for_test(&self) -> mpsc::UnboundedReceiver<ScanEvent> {
        let mut events = self.event_tx.every_event();
        let (reader, scan_events) = mpsc::unbounded_channel();
        self.runtime_handle.spawn(async move {
            while let Some(event) = events.recv().await {
                if let ImportEvent::Scan(event) = event {
                    if reader.send(event).is_err() {
                        break;
                    }
                }
            }
        });
        scan_events
    }

    async fn release_import_claim(
        &self,
        candidate_key: &crate::CandidateKey,
        import_id: &str,
    ) -> Result<(), crate::ImportError> {
        let _commit = self.folder_state_commit.lock("release an import claim").await;
        Ok(self.runtime.move_import(candidate_key, import_id, None).await?)
    }
}

#[cfg(test)]
#[path = "import_tests.rs"]
mod tests;
