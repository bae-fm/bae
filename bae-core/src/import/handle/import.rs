use super::*;
use crate::discogs::DiscogsClient;
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

impl ImportServiceHandle {
    /// Project a File Tags candidate into a `ReleaseUserEdit` so the
    /// edit-metadata form can seed itself from what's on disk: the parsed CUE
    /// track layout for CUE-backed candidates, embedded tags for per-track-file
    /// ones. This backs "Use File Tags" — the UI previews, then shows the editor
    /// for verification before commit.
    ///
    /// This is only the seed. Import consumes the same stored snapshot, so the
    /// editor and commit agree on the file facts under the user's edits.
    pub async fn preview_file_tags_for_folder(
        &self,
        candidate_key: String,
    ) -> Result<crate::import::ReleaseUserEdit, crate::import::ImportError> {
        Ok(self.file_tag_seed(&candidate_key).await?.0)
    }

    /// The release the folder's own files describe: the parsed CUE track layout
    /// for CUE-backed candidates, embedded tags for per-track-file ones, with
    /// the files it was read from.
    async fn file_tag_seed(
        &self,
        candidate_key: &str,
    ) -> Result<
        (
            crate::import::ReleaseUserEdit,
            crate::import::folder_scanner::CategorizedFiles,
        ),
        crate::import::ImportError,
    > {
        let (candidate, snapshot) = self.file_tag_snapshot(candidate_key).await?;

        let clock = self.clock.clone();
        let ids = self.ids.clone();
        tokio::task::spawn_blocking(move || {
            let edit = candidate.file_tag_edit(&snapshot, clock.as_ref(), ids.as_ref())?;
            Ok((edit, candidate.into_files()))
        })
        .await
        .map_err(|e| crate::import::ImportError::Internal {
            detail: format!("file-tag preview projection task failed: {e}"),
        })?
    }

    pub(super) async fn file_tag_snapshot(
        &self,
        candidate_key: &str,
    ) -> Result<
        (
            crate::import::release_candidate::ReleaseCandidate,
            crate::import::file_tag_snapshot::FileTagSnapshot,
        ),
        crate::import::ImportError,
    > {
        self.file_tag_snapshot_with_reader(
            candidate_key,
            std::sync::Arc::new(crate::import::file_tag_snapshot::LoftyFileTagReader),
        )
        .await
    }

    pub(super) async fn file_tag_snapshot_with_reader(
        &self,
        candidate_key: &str,
        reader: std::sync::Arc<dyn crate::import::file_tag_snapshot::FileTagReader>,
    ) -> Result<
        (
            crate::import::release_candidate::ReleaseCandidate,
            crate::import::file_tag_snapshot::FileTagSnapshot,
        ),
        crate::import::ImportError,
    > {
        let Some(candidate) = self.get_release_candidate(candidate_key).await? else {
            return Err(crate::import::ImportError::Internal {
                detail: format!("{candidate_key} is not an actionable folder candidate"),
            });
        };
        let watched_folder_path = candidate.watched_folder_path().to_string();
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
        let audio_files = candidate.files().audio().cloned().collect::<Vec<_>>();
        let file_edit_revision = candidate.file_edit_revision();
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

        if extracted
            && !self
                .library_manager
                .replace_candidate_file_tag_snapshot(&watched_folder_path, candidate_key, &snapshot)
                .await?
        {
            return Err(crate::import::ImportError::FileTags {
                detail: format!(
                    "{candidate_key} changed while its file tags were being read; open it again"
                ),
            });
        }
        Ok((candidate, snapshot))
    }

    /// Build an import command from what the candidate stores and enqueue it.
    ///
    /// Nothing about the release rides in: the metadata draft, the metadata the
    /// user typed, the rows they corrected and the cover they chose are all
    /// rows under this candidate's content hash, so the commit reads the very
    /// values the pane drew. The caller says only where the files should live.
    ///
    /// The worker sources the release itself from the persisted provider
    /// documents for an external release, or the stored snapshot for File Tags.
    pub async fn start_import(
        &self,
        candidate_key: &str,
        storage_mode: StorageMode,
        pin: bool,
    ) -> Result<String, crate::import::ImportError> {
        let commit = self.folder_state_commit.lock().await;
        let Some(candidate) = self.get_release_candidate(candidate_key).await? else {
            return Err(crate::import::ImportError::Internal {
                detail: format!("{candidate_key} is not a scanned folder candidate"),
            });
        };
        let content_hash = candidate.files().content_hash();
        let preparation = self
            .library_manager
            .load_import_candidate_preparation(&content_hash)
            .await?
            .filter(|preparation| preparation.file_edit_revision == candidate.file_edit_revision())
            .ok_or_else(|| crate::import::ImportError::Internal {
                detail: format!(
                    "{candidate_key} has no complete preparation for its current files"
                ),
            })?;
        let metadata_provenance = preparation.metadata_provenance.clone();
        let needs_file_tag_snapshot = matches!(
            metadata_provenance,
            Some(crate::import::MetadataProvenance::FileTags)
        ) || matches!(
            preparation.cover,
            Some(crate::import::CoverSelection::Embedded(_))
        );
        let file_tag_snapshot = if needs_file_tag_snapshot {
            let Some(stored) = self
                .library_manager
                .load_candidate_file_tag_snapshot(candidate.watched_folder_path(), candidate_key)
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
            if snapshot_candidate.files().content_hash() != content_hash
                || snapshot_candidate.file_edit_revision() != candidate.file_edit_revision()
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
                        "{candidate_key}'s file tags have not been read; open File Tags again"
                    ),
                });
            };
            if snapshot.scan_generation != scan_generation
                || snapshot.file_edit_revision != snapshot_candidate.file_edit_revision()
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
            content_hash: content_hash.clone(),
            edit_revision: candidate.file_edit_revision(),
            metadata_revision: preparation.metadata_revision,
            file_tag_snapshot,
        };
        let command = ImportCommand {
            import_id: import_id.clone(),
            candidate_key: candidate_key.to_string(),
            source: candidate.source(),
            #[cfg(any(test, feature = "test-utils"))]
            selected_cover: None,
            storage_mode,
            pin,
            #[cfg(any(test, feature = "test-utils"))]
            metadata_provenance: None,
            #[cfg(any(test, feature = "test-utils"))]
            user_edit: None,
        };

        self.library_manager
            .clear_import_candidate_failure(expectation.content_hash())
            .await?;
        self.runtime.claim_for_import(candidate_key);
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
                &candidate.files().content_hash(),
                surviving_artist_id,
            )
            .await?;
        Ok(())
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
        self.library_manager
            .revalidate_discogs_token()
            .await
            .map_err(Into::into)
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

    /// Test helper that stores the scanned candidate an import requires, queues
    /// the command, and returns its import ID for progress tracking.
    #[cfg(all(
        any(test, feature = "test-utils"),
        not(any(target_os = "ios", target_os = "android"))
    ))]
    pub async fn send_command(
        &self,
        mut command: ImportCommand,
    ) -> Result<String, crate::import::ImportError> {
        let crate::import::release_candidate::CandidateSource::Folder {
            path: folder,
            scope,
        } = command.source.clone()
        else {
            return Err(crate::import::ImportError::Internal {
                detail: "the folder fixture helper requires a folder source".into(),
            });
        };
        let categorized =
            crate::import::folder_scanner::collect_release_candidate_files_with_scope(
                &folder,
                scope,
                &crate::import::folder_scanner::StoredCandidateEdits::none(),
            )?;
        let candidate_key =
            crate::import::folder_registry::canonical_absolute_root(&folder.to_string_lossy())?;
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
            resolved_boundaries: Vec::new(),
            combine_ancestor_key: None,
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
            );
            assets.artist_images = self
                .library_manager
                .prepare_discogs_artist_images(source_draft.mapped_new_discogs_artist_ids.clone())
                .await?;
            self.preparations
                .apply_source(
                    &candidate.watched_folder_path,
                    &content_hash,
                    &command.candidate_key,
                    candidate.file_edit_revision,
                    self.library_manager
                        .load_import_candidate_state(&content_hash)
                        .await?
                        .ok_or_else(|| crate::import::ImportError::Internal {
                            detail: "test import has no candidate state".into(),
                        })?
                        .metadata_revision,
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
            Some(crate::import::MetadataProvenance::FileTags)
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
                    detail: "test import has no prepared File Tags snapshot".into(),
                })?;
            Some(snapshot)
        } else {
            None
        };
        let expectation = crate::import::service::ImportExpectation {
            content_hash,
            edit_revision: candidate.file_edit_revision,
            metadata_revision,
            file_tag_snapshot,
        };
        self.send_command_with_expectation(command, expectation)
            .await
    }

    /// The one way an import command reaches the worker, and so the one place
    /// the candidate is claimed for it.
    ///
    /// The claim goes first and is the reason this is async: it is taken under
    /// the folder-state commit lock, so a queue-sweep verdict for this
    /// candidate either landed before the user committed to importing it or is
    /// refused. A command that never reaches the worker releases the claim
    /// again rather than leaving a candidate owned by an import that does not
    /// exist.
    #[cfg(any(test, feature = "test-utils"))]
    async fn send_command_with_expectation(
        &self,
        command: ImportCommand,
        expectation: crate::import::service::ImportExpectation,
    ) -> Result<String, crate::import::ImportError> {
        let import_id = command.import_id.clone();
        let candidate_key = command.candidate_key.clone();
        let commit = self.folder_state_commit.lock().await;
        // Whatever the last attempt left is about to be answered by this one,
        // so the pane stops offering Retry the moment the work is queued.
        self.library_manager
            .clear_import_candidate_failure(expectation.content_hash())
            .await?;
        self.runtime.claim_for_import(&candidate_key);
        drop(commit);
        self.send_claimed_command(command, expectation).await?;
        Ok(import_id)
    }

    async fn send_claimed_command(
        &self,
        command: ImportCommand,
        expectation: crate::import::service::ImportExpectation,
    ) -> Result<(), crate::import::ImportError> {
        let candidate_key = command.candidate_key.clone();
        if self
            .requests_tx
            .send(crate::import::service::ImportWorkerMessage::Import {
                command,
                expectation,
            })
            .is_err()
        {
            self.release_import_claim(&candidate_key).await;
            return Err(crate::import::ImportError::Internal {
                detail: "Failed to queue import command".to_string(),
            });
        }
        Ok(())
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
