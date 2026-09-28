use super::*;
use crate::import::import_cancel::CancelOutcome;
use crate::util::rate_limiter::CallPriority;

#[derive(PartialEq, Eq)]
enum FileTagSnapshotMatch {
    Current,
    CandidateChanged,
    AudioChanged,
}

fn file_tag_snapshot_match(
    snapshot: &crate::import::file_tag_snapshot::FileTagSnapshot,
    scan_generation: u64,
    file_edit_revision: u64,
    observations: &[crate::import::file_tag_snapshot::FileObservation],
) -> FileTagSnapshotMatch {
    if snapshot.scan_generation != scan_generation
        || snapshot.file_edit_revision != file_edit_revision
    {
        return FileTagSnapshotMatch::CandidateChanged;
    }
    if snapshot
        .files
        .iter()
        .map(|fact| &fact.observation)
        .eq(observations)
    {
        FileTagSnapshotMatch::Current
    } else {
        FileTagSnapshotMatch::AudioChanged
    }
}

/// A candidate's file tags as [`ImportServiceHandle::read_file_tag_snapshot`]
/// found them, not yet stored.
pub(super) struct FileTagSnapshotRead {
    /// The candidate as stored when its reading was looked up.
    pub(super) candidate: crate::import::folder_scanner::FolderCandidate,
    pub(super) snapshot: crate::import::file_tag_snapshot::FileTagSnapshot,
    /// Whether the files were read, rather than the stored reading kept.
    pub(super) extracted: bool,
}

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

impl ImportServiceHandle {
    pub(super) async fn file_tag_snapshot(
        &self,
        candidate_key: &str,
    ) -> Result<
        (
            crate::import::folder_scanner::FolderCandidate,
            crate::import::file_tag_snapshot::FileTagSnapshot,
        ),
        crate::import::ImportError,
    > {
        self.file_tag_snapshot_with_reader(candidate_key, self.file_tags.clone())
            .await
    }

    pub(super) async fn file_tag_snapshot_with_reader(
        &self,
        candidate_key: &str,
        reader: std::sync::Arc<dyn crate::import::file_tag_snapshot::FileTagReader>,
    ) -> Result<
        (
            crate::import::folder_scanner::FolderCandidate,
            crate::import::file_tag_snapshot::FileTagSnapshot,
        ),
        crate::import::ImportError,
    > {
        let read = self.read_file_tag_snapshot(candidate_key, reader).await?;
        if read.extracted
            && !self
                .library_manager
                .replace_candidate_file_tag_snapshot(
                    &read.candidate.watched_folder_path,
                    candidate_key,
                    &read.snapshot,
                )
                .await?
        {
            return Err(crate::import::ImportError::FileTags {
                detail: format!(
                    "{candidate_key} changed while its file tags were being read; open it again"
                ),
            });
        }
        Ok((read.candidate, read.snapshot))
    }

    /// The stored reading while the candidate and its audio files are unchanged,
    /// a new reading otherwise. Stores nothing; the caller decides whether the
    /// reading is kept.
    pub(super) async fn read_file_tag_snapshot(
        &self,
        candidate_key: &str,
        reader: std::sync::Arc<dyn crate::import::file_tag_snapshot::FileTagReader>,
    ) -> Result<FileTagSnapshotRead, crate::import::ImportError> {
        let Some(candidate) = self.get_release_candidate(candidate_key).await? else {
            return Err(crate::import::ImportError::Internal {
                detail: format!("{candidate_key} is not an actionable folder candidate"),
            });
        };
        let watched_folder_path = candidate.watched_folder_path.to_string();
        let Some(stored) = self
            .library_manager
            .load_candidate_file_tag_snapshot(&watched_folder_path, candidate_key)
            .await?
        else {
            return Err(crate::import::ImportError::Internal {
                detail: format!("{candidate_key} is not an actionable folder candidate"),
            });
        };
        let crate::db::DbCandidateFileTagSnapshot {
            scan_generation,
            candidate,
            snapshot: stored_snapshot,
        } = stored;
        let audio_files = candidate.files.audio().cloned().collect::<Vec<_>>();
        let file_edit_revision = candidate.file_edit_revision;
        let (snapshot, extracted) = tokio::task::spawn_blocking(move || {
            let observations = crate::import::file_tag_snapshot::observe_audio_files(&audio_files)?;
            if let Some(snapshot) = stored_snapshot.filter(|snapshot| {
                file_tag_snapshot_match(
                    snapshot,
                    scan_generation,
                    file_edit_revision,
                    &observations,
                ) == FileTagSnapshotMatch::Current
            }) {
                return Ok::<_, crate::import::ImportError>((snapshot, false));
            }
            Ok((
                crate::import::file_tag_snapshot::extract_file_tag_snapshot(
                    &audio_files,
                    scan_generation,
                    file_edit_revision,
                    reader.as_ref(),
                )?,
                true,
            ))
        })
        .await
        .map_err(|error| crate::import::ImportError::Internal {
            detail: format!("file-tag snapshot task failed: {error}"),
        })??;
        Ok(FileTagSnapshotRead {
            candidate,
            snapshot,
            extracted,
        })
    }

    /// Queue an import of the candidate as stored, ending its identification.
    pub async fn start_import(
        &self,
        candidate_key: &str,
    ) -> Result<String, crate::import::ImportError> {
        let this = self.clone();
        let candidate_key = candidate_key.to_string();
        self.committed(async move {
            this.start_import_write(&candidate_key, ImportRequest::Candidate)
                .await
        })
        .await
    }

    /// [`Self::start_import`] for one row of a bulk import, refused while that
    /// row is being identified.
    pub async fn import_selected(
        &self,
        candidate_key: &str,
    ) -> Result<String, crate::import::ImportError> {
        let this = self.clone();
        let candidate_key = candidate_key.to_string();
        self.committed(async move {
            this.start_import_write(&candidate_key, ImportRequest::Selection)
                .await
        })
        .await
    }

    /// [`Self::start_import`] for a candidate its automatic run just settled as
    /// auto-importable. Nobody sees a refusal, so it is recorded as the
    /// candidate's failed import.
    pub(crate) async fn import_identified(
        &self,
        candidate_key: &str,
    ) -> Result<String, crate::import::ImportError> {
        let this = self.clone();
        let candidate_key = candidate_key.to_string();
        self.committed(async move {
            let started = this
                .start_import_write(&candidate_key, ImportRequest::Candidate)
                .await;
            match &started {
                Ok(_) => {}
                // Another import owns the candidate or already made it a
                // release; that import reports how it went.
                Err(
                    crate::import::ImportError::CandidateImportInProgress
                    | crate::import::ImportError::CandidateAlreadyImported,
                ) => {}
                Err(error) => this.record_failed_start(&candidate_key, error).await,
            }
            started
        })
        .await
    }

    /// Record a refused start as `candidate_key`'s failed import.
    async fn record_failed_start(&self, candidate_key: &str, error: &crate::import::ImportError) {
        let candidate = match self.get_release_candidate(candidate_key).await {
            Ok(Some(candidate)) => candidate,
            Ok(None) => {
                warn!("{candidate_key} is gone; its refused import has no row to show on");
                return;
            }
            Err(read) => {
                tracing::error!("could not read {candidate_key} to record its refused import: {read}");
                return;
            }
        };
        let failure = crate::import::service::ImportService::terminal_failure(
            error,
            self.library_manager.now(),
        );
        if let Err(write) = self
            .library_manager
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
        candidate_key: &str,
        request: ImportRequest,
    ) -> Result<String, crate::import::ImportError> {
        let commit = self.folder_state_commit.lock("start an import").await;
        match request {
            ImportRequest::Candidate => {}
            ImportRequest::Selection => {
                let facts = self
                    .runtime
                    .get(candidate_key)
                    .as_ref()
                    .map(crate::import::triage::TriageRuntimeFacts::of)
                    .unwrap_or_default();
                if facts.identifying() {
                    return Err(crate::import::ImportError::CandidateBeingIdentified);
                }
            }
        }
        let Some(candidate) = self.get_release_candidate(candidate_key).await? else {
            return Err(crate::import::ImportError::Internal {
                detail: format!("{candidate_key} is not a scanned folder candidate"),
            });
        };
        // Refused for every request kind: an import owns it or its files are
        // already a release.
        self.candidate_standing(candidate_key, &candidate)
            .await?
            .editable()?;
        let content_hash = candidate.files.content_hash();
        let preparation = self
            .library_manager
            .load_import_candidate_preparation(&content_hash)
            .await?
            .filter(|preparation| preparation.file_edit_revision == candidate.file_edit_revision)
            .ok_or_else(|| crate::import::ImportError::Internal {
                detail: format!(
                    "{candidate_key} has no complete preparation for its current files"
                ),
            })?;
        let metadata_provenance = preparation.metadata_provenance.clone();
        let needs_file_tag_snapshot = matches!(
            metadata_provenance,
            Some(crate::import::MetadataProvenance::FileMetadata)
        ) || matches!(
            preparation.cover,
            Some(crate::import::CoverSelection::Embedded(_))
        );
        let file_tag_snapshot = if needs_file_tag_snapshot {
            let Some(stored) = self
                .library_manager
                .load_candidate_file_tag_snapshot(&candidate.watched_folder_path, candidate_key)
                .await?
            else {
                return Err(crate::import::ImportError::Internal {
                    detail: format!("{candidate_key} is not an actionable folder candidate"),
                });
            };
            let crate::db::DbCandidateFileTagSnapshot {
                scan_generation,
                candidate: snapshot_candidate,
                snapshot,
            } = stored;
            if snapshot_candidate.files.content_hash() != content_hash
                || snapshot_candidate.file_edit_revision != candidate.file_edit_revision
            {
                return Err(crate::import::ImportError::FileTags {
                    detail: format!(
                        "{candidate_key} changed after its file tags were read; open it again"
                    ),
                });
            }
            let Some(snapshot) = snapshot else {
                return Err(crate::import::ImportError::FileTags {
                    detail: format!(
                        "{candidate_key}'s file tags have not been read; open it again"
                    ),
                });
            };
            if snapshot.scan_generation != scan_generation
                || snapshot.file_edit_revision != snapshot_candidate.file_edit_revision
            {
                return Err(crate::import::ImportError::FileTags {
                    detail: format!(
                        "{candidate_key} changed after its file tags were read; open it again"
                    ),
                });
            }
            Some(snapshot)
        } else {
            None
        };
        let import_id = self.library_manager.new_id();
        let expectation = crate::import::service::ImportExpectation {
            candidate: crate::import::CandidateAsRead {
                content_hash: content_hash.clone(),
                file_edit_revision: candidate.file_edit_revision,
                metadata_revision: preparation.metadata_revision,
            },
            file_tag_snapshot,
        };
        let command = ImportCommand {
            import_id: import_id.clone(),
            candidate_key: candidate_key.to_string(),
            source: candidate.source(),
            #[cfg(any(test, feature = "test-utils"))]
            selected_cover: None,
            destination: self.library_manager.get_config().import_destination(),
            #[cfg(any(test, feature = "test-utils"))]
            metadata_provenance: None,
            #[cfg(any(test, feature = "test-utils"))]
            user_edit: None,
        };

        // Claimed under the lock the standing was read under, so no second
        // import can claim these files in between.
        self.runtime.claim_for_import(candidate_key, &import_id)?;
        // Once claimed, the candidate is not identification's to answer.
        self.cancel_identification(candidate_key);
        drop(commit);
        self.send_claimed_command(command, expectation).await?;
        Ok(import_id)
    }

    /// Resolve the recoverable artist conflict stored for this candidate by
    /// keeping the selected library row and absorbing the other one.
    pub async fn merge_candidate_artist_identity_conflict(
        &self,
        candidate_key: &str,
        surviving_artist_id: &str,
    ) -> Result<(), crate::import::ImportError> {
        let Some(candidate) = self.get_release_candidate(candidate_key).await? else {
            return Err(crate::import::ImportError::Internal {
                detail: format!("{candidate_key} is not a scanned folder candidate"),
            });
        };
        self.library_manager
            .merge_import_artist_identity_conflict(
                &candidate.files.content_hash(),
                surviving_artist_id,
            )
            .await?;
        Ok(())
    }

    /// Check a submitted Discogs key against Discogs and store it unless
    /// Discogs rejects it; a key that could not be checked is stored as
    /// unvalidated.
    pub async fn save_discogs_token(
        &self,
        token: &str,
    ) -> Result<DiscogsSaveOutcome, crate::import::ImportError> {
        use crate::config::DiscogsValidation;

        match validation_from_validate_result(
            self.library_manager
                .try_discogs_key(token, CallPriority::Interactive)
                .await,
        ) {
            DiscogsValidation::Valid => {
                self.persist_discogs_key(token, DiscogsValidation::Valid).await?;
                Ok(DiscogsSaveOutcome::Valid)
            }
            DiscogsValidation::Unvalidated => {
                self.persist_discogs_key(token, DiscogsValidation::Unvalidated).await?;
                Ok(DiscogsSaveOutcome::Unvalidated)
            }
            DiscogsValidation::Rejected => Ok(DiscogsSaveOutcome::Rejected),
        }
    }

    /// Store the key and record its validation.
    async fn persist_discogs_key(
        &self,
        token: &str,
        validation: crate::config::DiscogsValidation,
    ) -> Result<(), crate::import::ImportError> {
        self.library_manager
            .set_discogs_key(token, validation)
            .await
            .map_err(|e| crate::import::ImportError::Config {
                detail: e.to_string(),
            })
    }

    /// Check a stored `Unvalidated` key against Discogs again; any other
    /// state, or no key, is left alone.
    pub async fn revalidate_discogs_token(&self) -> Result<(), crate::import::ImportError> {
        use crate::config::DiscogsValidation;

        if self.library_manager.discogs_validation() != Some(DiscogsValidation::Unvalidated) {
            return Ok(());
        }
        self.library_manager
            .revalidate_discogs_token()
            .await
            .map_err(Into::into)
    }

    /// Remove the stored Discogs key and its validation.
    pub async fn remove_discogs_token(&self) -> Result<(), crate::import::ImportError> {
        self.library_manager
            .clear_discogs_key()
            .await
            .map_err(|e| crate::import::ImportError::Config {
                detail: e.to_string(),
            })
    }

    /// Store `command`'s folder as a scanned candidate carrying the command's
    /// metadata and cover, then queue its import and return the import ID.
    #[cfg(all(
        any(test, feature = "test-utils"),
        not(any(target_os = "ios", target_os = "android"))
    ))]
    pub async fn send_command(
        &self,
        mut command: ImportCommand,
    ) -> Result<String, crate::import::ImportError> {
        let crate::import::release_candidate::CandidateSource {
            path: folder,
            scope,
            ..
        } = command.source.clone();
        let categorized =
            crate::import::folder_scanner::collect_release_candidate_files_with_scope(
                &folder,
                scope,
                &crate::import::folder_scanner::StoredCandidateEdits::none(),
            )?;
        let candidate_key =
            crate::import::watched_folder::canonical_absolute_root(&folder.to_string_lossy())?;
        let candidate_name = folder
            .file_name()
            .and_then(|name| name.to_str())
            .ok_or_else(|| crate::import::ImportError::Internal {
                detail: format!(
                    "test import folder has no UTF-8 directory name: {}",
                    folder.display()
                ),
            })?
            .to_string();
        self.library_manager
            .add_watched_import_folder(&candidate_key)
            .await?;
        let generation = self
            .library_manager
            .begin_folder_scan(&candidate_key)
            .await?;
        let candidate = crate::import::folder_scanner::FolderCandidate {
            path: folder.clone(),
            file_root: folder,
            name: candidate_name,
            files: categorized.clone(),
            watched_folder_path: candidate_key.clone(),
            scope,
            file_edit_revision: 0,
            display_path: String::new(),
            grouping: None,
        };
        if self
            .library_manager
            .save_folder_scan_item(
                &candidate_key,
                generation,
                &crate::import::folder_scanner::ScanItem::Valid(candidate.clone()),
            )
            .await?
            .is_none()
        {
            return Err(crate::import::ImportError::Internal {
                detail: format!("test import scan for {candidate_key} was superseded"),
            });
        }
        if self
            .library_manager
            .finish_folder_scan(&candidate_key, generation, None)
            .await?
            .is_none()
        {
            return Err(crate::import::ImportError::Internal {
                detail: format!("test import scan for {candidate_key} was superseded"),
            });
        }
        command.candidate_key = candidate_key;
        let content_hash = categorized.content_hash();
        if let Some(provenance) = command.metadata_provenance.clone() {
            self.set_candidate_metadata_provenance(command.candidate_key.clone(), provenance)
                .await?;
        }
        if let Some(edit) = command.user_edit.clone() {
            let rows = self
                .library_manager
                .load_import_candidate_pane_rows(&content_hash)
                .await?;
            let mut assets = self
                .library_manager
                .load_import_candidate_prepared_assets(&content_hash)
                .await?;
            let source_draft = crate::import::pane::candidate_draft_from_edit(
                crate::import::RawReleaseEdit::from_user_edit(edit, "test-import-track"),
            )?;
            assets.artist_images = self
                .library_manager
                .prepare_discogs_artist_images(source_draft.mapped_credit_discogs_artist_ids.clone())
                .await?;
            self.preparations
                .apply_source(
                    &candidate.watched_folder_path,
                    &crate::import::CandidateAsRead {
                        content_hash: content_hash.clone(),
                        file_edit_revision: candidate.file_edit_revision,
                        metadata_revision: self
                            .library_manager
                            .load_import_candidate_state(&content_hash)
                            .await?
                            .ok_or_else(|| crate::import::ImportError::Internal {
                                detail: "test import has no candidate state".into(),
                            })?
                            .metadata_revision,
                    },
                    &command.candidate_key,
                    &crate::import::CandidateMetadataDraft {
                        draft: source_draft.draft,
                        source_discogs_artist_ids: Default::default(),
                        provenance: command.metadata_provenance.clone(),
                        cover: rows.cover,
                        assets,
                    },
                )
                .await?;
        }
        if let Some(cover) = command.selected_cover.clone() {
            self.set_candidate_cover(&command.candidate_key, cover)
                .await?;
        }
        let metadata_revision = self
            .library_manager
            .load_import_candidate_state(&content_hash)
            .await?
            .ok_or_else(|| crate::import::ImportError::Internal {
                detail: "test import has no candidate state".into(),
            })?
            .metadata_revision;
        let file_tag_snapshot = if matches!(
            command.metadata_provenance,
            Some(crate::import::MetadataProvenance::FileMetadata)
        ) || matches!(
            command.selected_cover,
            Some(crate::import::CoverSelection::Embedded(_))
        ) {
            let snapshot = self
                .library_manager
                .load_candidate_file_tag_snapshot(
                    &candidate.watched_folder_path,
                    &command.candidate_key,
                )
                .await?
                .and_then(|stored| stored.snapshot)
                .ok_or_else(|| crate::import::ImportError::FileTags {
                    detail: "test import has no prepared file-tag snapshot".into(),
                })?;
            Some(snapshot)
        } else {
            None
        };
        let expectation = crate::import::service::ImportExpectation {
            candidate: crate::import::CandidateAsRead {
                content_hash,
                file_edit_revision: candidate.file_edit_revision,
                metadata_revision,
            },
            file_tag_snapshot,
        };
        self.send_command_with_expectation(command, expectation)
            .await
    }

    /// Claim the candidate under the folder-state commit lock, so an
    /// identification verdict for it either landed first or is refused, then
    /// queue the import.
    #[cfg(any(test, feature = "test-utils"))]
    async fn send_command_with_expectation(
        &self,
        command: ImportCommand,
        expectation: crate::import::service::ImportExpectation,
    ) -> Result<String, crate::import::ImportError> {
        let import_id = command.import_id.clone();
        let candidate_key = command.candidate_key.clone();
        let commit = self.folder_state_commit.lock("queue a test import").await;
        self.runtime.claim_for_import(&candidate_key, &import_id)?;
        drop(commit);
        self.send_claimed_command(command, expectation).await?;
        Ok(import_id)
    }

    /// A command the worker cannot take releases its claim, so no candidate is
    /// left owned by an import that does not exist.
    async fn send_claimed_command(
        &self,
        command: ImportCommand,
        expectation: crate::import::service::ImportExpectation,
    ) -> Result<(), crate::import::ImportError> {
        let candidate_key = command.candidate_key.clone();
        let import_id = command.import_id.clone();
        self.import_cancels.register(&candidate_key, &import_id);
        if self
            .worker
            .send(crate::import::service::ImportWorkerMessage::Import {
                command,
                expectation,
            })
            .is_err()
        {
            self.import_cancels.forget(&candidate_key, &import_id);
            self.release_import_claim(&candidate_key, &import_id).await;
            return Err(crate::import::ImportError::Internal {
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

    /// Cancel the waiting or running import of `candidate_key`, leaving the
    /// candidate as it stood before the import was asked for. An import already
    /// writing its release finishes, and that is the error.
    pub fn cancel_import(&self, candidate_key: &str) -> Result<(), crate::import::ImportError> {
        match self.import_cancels.cancel(candidate_key) {
            CancelOutcome::NotImporting | CancelOutcome::CancelledRunning => Ok(()),
            CancelOutcome::CancelledWaiting { import_id } => {
                self.announce_cancelled_import(candidate_key, import_id);
                Ok(())
            }
            CancelOutcome::Writing => Err(crate::import::ImportError::ImportWriting),
        }
    }

    /// Cancel every import that has not begun writing its release.
    pub fn cancel_all_imports(&self) {
        for (candidate_key, import_id) in self.import_cancels.cancel_all() {
            self.announce_cancelled_import(&candidate_key, import_id);
        }
    }

    /// Announce the end of an import cancelled while waiting: the worker skips
    /// it, so it never will.
    fn announce_cancelled_import(&self, candidate_key: &str, import_id: String) {
        info!("Import of {candidate_key} was cancelled before it started");
        self.event_tx.send(ImportEvent::ImportProgress {
            candidate_key: candidate_key.to_string(),
            progress: ImportProgress::Cancelled { import_id },
        });
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
}
