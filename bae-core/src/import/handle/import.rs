use super::*;
use crate::discogs::DiscogsClient;
use crate::import::payloads::ReleasePayloads;
use crate::util::rate_limiter::CallPriority;

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
        let folder_name = Some(candidate.name);
        let categorized = candidate.files;

        let clock = self.clock.clone();
        let ids = self.ids.clone();
        tokio::task::spawn_blocking(move || {
            let parsed = crate::import::file_tag_mapper::map_file_tag_snapshot_to_db(
                &categorized,
                &snapshot,
                folder_name.as_deref(),
                clock.as_ref(),
                ids.as_ref(),
            )?;
            Ok((parsed_album_to_user_edit(&parsed), categorized))
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
            crate::import::folder_scanner::FolderCandidate,
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
            crate::import::folder_scanner::FolderCandidate,
            crate::import::file_tag_snapshot::FileTagSnapshot,
        ),
        crate::import::ImportError,
    > {
        let Some(candidate) = self.stored_actionable_candidate(candidate_key).await? else {
            return Err(crate::import::ImportError::Internal {
                detail: format!("{candidate_key} is not an actionable folder candidate"),
            });
        };
        let watched_folder_path = candidate.watched_folder_path;
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
                snapshot.scan_generation == scan_generation
                    && snapshot.file_edit_revision == file_edit_revision
                    && snapshot
                        .files
                        .iter()
                        .map(|fact| &fact.observation)
                        .eq(observations.iter())
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
    /// Nothing about the release rides in: the pick, the metadata the user
    /// typed, the rows they corrected and the cover they chose are all rows
    /// under this candidate's content hash, so the commit reads the very
    /// values the pane drew. The caller says only where the files should live.
    ///
    /// The worker sources the release itself — `prepare_release` for a picked
    /// release, reading the documents the pick archived; the stored snapshot
    /// for File Tags.
    pub async fn start_import(
        &self,
        candidate_key: &str,
        storage_mode: StorageMode,
        pin: bool,
    ) -> Result<String, crate::import::ImportError> {
        let Some(ImportCandidateSnapshot::Folder {
            candidate,
            actionable: true,
            ..
        }) = self.get_candidate(candidate_key).await?
        else {
            return Err(crate::import::ImportError::Internal {
                detail: format!("{candidate_key} is not a scanned folder candidate"),
            });
        };
        let content_hash = candidate.files.content_hash();
        let state = self
            .library_manager
            .load_import_candidate_state(&content_hash)
            .await?
            .filter(|state| state.file_edits.revision == candidate.file_edit_revision);
        let Some(pick) = state.as_ref().and_then(|state| state.metadata_seed.clone()) else {
            return Err(crate::import::ImportError::Internal {
                detail: format!("nothing is picked for {candidate_key}"),
            });
        };
        let durations = state.map(|state| state.durations).unwrap_or_default();
        let rows = self
            .library_manager
            .load_import_candidate_pane_rows(&content_hash)
            .await?;

        let file_tag_snapshot = if matches!(pick, crate::import::MetadataSeed::FileTags) {
            let (snapshot_candidate, snapshot) = self.file_tag_snapshot(candidate_key).await?;
            if snapshot_candidate.files.content_hash() != content_hash
                || snapshot_candidate.file_edit_revision != candidate.file_edit_revision
            {
                return Err(crate::import::ImportError::Internal {
                    detail: format!(
                        "{candidate_key} changed while its file-tag snapshot was being read"
                    ),
                });
            }
            Some(snapshot)
        } else {
            None
        };

        let seed_for_pane = pick.clone();
        let files = candidate.files.clone();
        let folder_name = candidate.name.clone();
        let clock = self.clock.clone();
        let ids = self.ids.clone();
        enum PaneSeed {
            ExternalRelease(ReleasePayloads),
            FileTags(crate::import::file_tag_snapshot::FileTagSnapshot),
            Manual,
        }
        let pane_seed = match &seed_for_pane {
            crate::import::MetadataSeed::ExternalRelease {
                source, release_id, ..
            } => PaneSeed::ExternalRelease(
                self.payloads_for_pick(
                    candidate_key,
                    &crate::import::MetadataRef::new(release_id.clone(), *source),
                )
                .await?,
            ),
            crate::import::MetadataSeed::FileTags => PaneSeed::FileTags(
                file_tag_snapshot
                    .clone()
                    .expect("File Tags always creates its snapshot before projection"),
            ),
            crate::import::MetadataSeed::Manual => PaneSeed::Manual,
        };
        let pane = tokio::task::spawn_blocking(move || match pane_seed {
            PaneSeed::ExternalRelease(payloads) => crate::import::pane::release_pane(
                &payloads,
                &files,
                &durations,
                &rows.edit,
                &rows.track_edits,
                clock.as_ref(),
                ids.as_ref(),
            ),
            PaneSeed::FileTags(snapshot) => crate::import::pane::file_tags_pane(
                &files,
                &snapshot,
                Some(&folder_name),
                &durations,
                &rows.edit,
                &rows.track_edits,
                clock.as_ref(),
                ids.as_ref(),
            ),
            PaneSeed::Manual => Ok(crate::import::pane::manual_pane(
                &files,
                &durations,
                &rows.edit,
                &rows.track_edits,
            )),
        })
        .await
        .map_err(|e| crate::import::ImportError::Internal {
            detail: format!("commit projection task failed: {e}"),
        })??;

        let mut raw = pane.edit;
        raw.tracks = crate::import::mapping_tracks(&pane.mapping);
        let user_edit = raw.shape()?;
        let selected_cover = rows.cover.clone().or_else(|| {
            pane.release
                .as_ref()
                .and_then(|release| release.default_cover())
                .map(|cover| crate::import::CoverSelection::Remote(cover.url.clone(), cover.source))
        });

        let import_id = self.library_manager.new_id();
        let expectation = match file_tag_snapshot {
            Some(snapshot) => crate::import::service::ImportExpectation::FileTags {
                content_hash: content_hash.clone(),
                snapshot,
            },
            None => crate::import::service::ImportExpectation::Candidate {
                content_hash: content_hash.clone(),
                edit_revision: candidate.file_edit_revision,
            },
        };
        let command = ImportCommand {
            import_id: import_id.clone(),
            candidate_key: candidate_key.to_string(),
            folder: candidate.file_root,
            scope: candidate.scope,
            selected_cover,
            storage_mode,
            pin,
            metadata_seed: pick,
            user_edit: Some(user_edit),
        };

        self.send_command_with_expectation(command, expectation)
            .await?;
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

    /// Queue an import command and return its import_id for progress tracking.
    /// Returns immediately — all the work (metadata resolution, file discovery,
    /// track mapping, DB insertion) happens in the service worker.
    pub async fn send_command(
        &self,
        command: ImportCommand,
    ) -> Result<String, crate::import::ImportError> {
        let categorized =
            crate::import::folder_scanner::collect_release_candidate_files_with_scope(
                &command.folder,
                command.scope,
                &crate::import::folder_scanner::StoredCandidateEdits::none(),
            )?;
        let content_hash = categorized.content_hash();
        let expectation = if matches!(command.metadata_seed, crate::import::MetadataSeed::FileTags)
        {
            let audio_files = categorized.audio().cloned().collect::<Vec<_>>();
            let snapshot = tokio::task::spawn_blocking(move || {
                crate::import::file_tag_snapshot::extract_file_tag_snapshot(
                    &audio_files,
                    0,
                    0,
                    &crate::import::file_tag_snapshot::LoftyFileTagReader,
                )
            })
            .await
            .map_err(|error| crate::import::ImportError::Internal {
                detail: format!("file-tag snapshot task failed: {error}"),
            })??;
            crate::import::service::ImportExpectation::FileTags {
                content_hash: categorized.content_hash(),
                snapshot,
            }
        } else {
            crate::import::service::ImportExpectation::Candidate {
                content_hash,
                edit_revision: 0,
            }
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
    async fn send_command_with_expectation(
        &self,
        command: ImportCommand,
        expectation: crate::import::service::ImportExpectation,
    ) -> Result<String, crate::import::ImportError> {
        let import_id = command.import_id.clone();
        let candidate_key = command.candidate_key.clone();
        // Whatever the last attempt left is about to be answered by this one,
        // so the pane stops offering Retry the moment the work is queued.
        self.library_manager
            .clear_import_candidate_failure(expectation.content_hash())
            .await?;
        self.claim_candidate_for_import(&candidate_key).await;
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
