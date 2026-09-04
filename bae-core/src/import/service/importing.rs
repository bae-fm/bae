use super::*;

impl ImportService {
    /// Start the import service worker: one task that drains the import queue
    /// sequentially, never concurrently. The returned handle is cloneable and
    /// is how the rest of the app submits import requests.
    pub(crate) async fn start(
        runtime_handle: tokio::runtime::Handle,
        library_manager: LibraryManager,
        clock: coven::ClockRef,
        ids: coven::IdRef,
    ) -> Result<ImportServiceHandle, crate::import::ImportError> {
        let (commands_tx, commands_rx) = mpsc::unbounded_channel();
        let (watcher_tx, watcher_rx) = mpsc::unbounded_channel();
        let (fs_tx, fs_rx) = mpsc::unbounded_channel::<DebounceEventResult>();
        let (event_tx, _) = broadcast::channel(1024);
        let event_tx_for_worker = event_tx.clone();
        let library_manager_for_handle = library_manager.clone();
        // One `Arc` shared by the watcher (which reads the skip set while stamping
        // candidates) and the handle (which mutates it on add/remove/skip).
        let loaded_registry = library_manager_for_handle
            .load_import_folder_registry()
            .await?;
        let runtime = CandidateRuntime::default();
        let folder_registry = Arc::new(Mutex::new(loaded_registry));
        let folder_state_commit = Arc::new(tokio::sync::Mutex::new(()));

        // Constructed before the watcher task spawns; the task doesn't need the
        // debouncer, only the `fs_rx` end of its event channel.
        let folder_watcher = Arc::new(FolderWatcher::new(fs_tx));

        let watcher_thread = ImportService::start_watcher(
            watcher_rx,
            fs_rx,
            event_tx.clone(),
            library_manager_for_handle.clone(),
            clock.clone(),
            ids.clone(),
            folder_registry.clone(),
            folder_state_commit.clone(),
            folder_watcher.clone(),
        );

        let clock_for_handle = clock.clone();
        let ids_for_handle = ids.clone();
        let worker_thread = std::thread::spawn(move || {
            let rt = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .expect("Failed to create import runtime");
            rt.block_on(async move {
                let mut service = ImportService {
                    commands_rx,
                    event_tx: event_tx_for_worker,
                    library_manager,
                    clock,
                    ids,
                };

                while let Some(message) = service.commands_rx.recv().await {
                    match message {
                        ImportWorkerMessage::Import {
                            command,
                            expectation,
                        } => service.do_import(command, expectation).await,
                        ImportWorkerMessage::Shutdown => break,
                    }
                }
            });
        });

        Ok(ImportServiceHandle::new(
            commands_tx,
            worker_thread,
            watcher_thread,
            library_manager_for_handle,
            clock_for_handle,
            ids_for_handle,
            runtime_handle,
            watcher_tx,
            event_tx,
            folder_registry,
            runtime,
            folder_state_commit,
        ))
    }

    pub(super) async fn do_import(&self, command: ImportCommand, expectation: ImportExpectation) {
        let import_id = command.import_id.clone();
        let candidate_key = command.candidate_key.clone();
        let content_hash = expectation.content_hash().to_string();
        let folder_path = command.folder.to_string_lossy().into_owned();
        let edit_revision = expectation.edit_revision();
        let result = self
            .prepare_and_run_folder_import(
                import_id.clone(),
                candidate_key.clone(),
                command.folder,
                command.scope,
                expectation,
                command.storage_mode,
                command.pin,
            )
            .await;

        if let Err(e) = result {
            error!("Import failed: {}", e);
            self.library_manager
                .record_telemetry(TelemetryEvent::ImportFailed {});
            // The typed error becomes a user-facing string only here, at the
            // pipeline's terminal consumer. The variant Displays embed their
            // `#[from]` source messages, so `to_string()` carries the chain.
            let failed_at = self.library_manager.now();
            let failure = Self::terminal_failure(&e, failed_at);
            let error = failure.error.clone();

            // The row goes first and the event second: the event is what the
            // pane showing this import redraws from, and the row is what a
            // relaunched pane reads instead. A row that will not write is
            // logged rather than swallowing the failure the user is waiting
            // to see.
            if let Err(write) = self
                .library_manager
                .save_import_candidate_failure(&content_hash, edit_revision, &failure)
                .await
            {
                error!("could not record the failed import of {folder_path}: {write}");
            }

            send_event(
                &self.event_tx,
                crate::import::handle::ImportEvent::ImportProgress {
                    candidate_key,
                    progress: ImportProgress::Failed { error, import_id },
                },
            );
        }
    }

    pub(super) fn terminal_failure(
        error: &crate::import::ImportError,
        failed_at: chrono::DateTime<chrono::Utc>,
    ) -> crate::import::ImportFailure {
        let message = error.to_string();
        let artist_identity_conflict = match error {
            crate::import::ImportError::Db(
                crate::library::LibraryError::ArtistIdentityConflict(conflict),
            ) => Some(conflict.as_ref().clone()),
            _ => None,
        };
        crate::import::ImportFailure {
            error: message,
            failed_at,
            artist_identity_conflict,
        }
    }

    /// Prepare and run a folder import from its stored candidate and metadata
    /// seed after validating that every physical file still has the scanned
    /// identity, then run the shared mapping and write path.
    pub(super) async fn prepare_and_run_folder_import(
        &self,
        import_id: String,
        candidate_key: String,
        folder: PathBuf,
        scope: crate::import::folder_scanner::ReleaseFileScope,
        expectation: ImportExpectation,
        storage_mode: StorageMode,
        pin: bool,
    ) -> Result<(), crate::import::ImportError> {
        let library_manager = &self.library_manager;
        let expected_content_hash = expectation.content_hash().to_string();
        let expected_edit_revision = expectation.edit_revision();

        let import_start = std::time::Instant::now();
        send_event(
            &self.event_tx,
            crate::import::handle::ImportEvent::ImportProgress {
                candidate_key: candidate_key.clone(),
                progress: ImportProgress::Preparing {
                    import_id: import_id.clone(),
                    step: PrepareStep::ValidatingSourceFiles,
                    album_title: String::new(),
                    artist_name: String::new(),
                },
            },
        );

        let stored_candidate = match library_manager
            .load_folder_scan_item(&candidate_key)
            .await?
        {
            Some(crate::import::folder_scanner::ScanItem::Valid(candidate)) => candidate,
            _ => {
                return Err(crate::import::ImportError::Internal {
                    detail: format!("{candidate_key} is no longer a valid stored import candidate"),
                })
            }
        };
        if stored_candidate.file_root != folder
            || stored_candidate.scope != scope
            || stored_candidate.files.content_hash() != expected_content_hash
            || stored_candidate.file_edit_revision != expected_edit_revision
        {
            return Err(crate::import::ImportError::Internal {
                detail: format!(
                    "{candidate_key} changed after it was selected; refresh and identify it again"
                ),
            });
        }
        let identity_files = stored_candidate
            .files
            .release_files()
            .cloned()
            .collect::<Vec<_>>();
        let audio_observations = tokio::task::spawn_blocking(move || {
            super::file_identity::validate_scanned_file_identities(&identity_files)
        })
        .await
        .map_err(|error| crate::import::ImportError::Internal {
            detail: format!("file identity validation task failed: {error}"),
        })??;
        let categorized = stored_candidate.files;

        let preparation = library_manager
            .load_import_candidate_preparation(&expected_content_hash)
            .await?
            .ok_or_else(|| crate::import::ImportError::Internal {
                detail: format!("{candidate_key} has no stored import preparation"),
            })?;
        if preparation.file_edit_revision != expected_edit_revision
            || preparation.metadata_revision != expectation.metadata_revision()
        {
            return Err(crate::import::ImportError::Internal {
                detail: format!(
                    "{candidate_key}'s preparation changed after it was queued; import it again"
                ),
            });
        }
        let metadata_provenance = preparation.metadata_provenance;
        let selected_cover = preparation.cover;
        let user_edit = Some(
            crate::import::edits::apply_track_mappings_to_draft(
                preparation.metadata_draft,
                &preparation.track_mappings,
            )?
            .shape()?,
        );
        let prepared_assets = preparation.assets;

        let file_tag_snapshot = expectation.file_tag_snapshot.as_ref();
        if let Some(snapshot) = file_tag_snapshot {
            if !snapshot
                .files
                .iter()
                .map(|fact| &fact.observation)
                .eq(audio_observations.iter())
            {
                return Err(crate::import::ImportError::FileTags {
                    detail: format!(
                        "{candidate_key}'s audio changed after its file tags were read"
                    ),
                });
            }
        }
        if matches!(
            metadata_provenance,
            Some(crate::import::MetadataProvenance::FileTags)
        ) && file_tag_snapshot.is_none()
        {
            return Err(crate::import::ImportError::Internal {
                detail: format!("{candidate_key}'s File Tags import has no metadata snapshot"),
            });
        }
        if matches!(selected_cover, Some(CoverSelection::Embedded(_)))
            && file_tag_snapshot.is_none()
        {
            return Err(crate::import::ImportError::CoverArt {
                detail: format!("{candidate_key}'s embedded cover has no prepared tag snapshot"),
            });
        }

        // Overwrites a prior import of the same files (below), then gets stamped
        // onto the new release row.
        let content_hash = categorized.content_hash();
        let replacement_plans = library_manager
            .import_replacement_plans_for_content_hash(&content_hash)
            .await?;
        let replacement_release_ids: Vec<String> = replacement_plans
            .iter()
            .map(|plan| plan.db_delete.release_id.clone())
            .collect();

        let source_durations = crate::import::probe::source_durations(&categorized)?;
        let audio_durations =
            crate::import::track_slots::audio_durations(&categorized, &source_durations)?;

        let parsed = match &metadata_provenance {
            Some(crate::import::MetadataProvenance::ExternalRelease {
                source,
                release_id,
                partners,
            }) => {
                // The documents are archived by `prepare_release`, keyed by the
                // picked source release — so nothing about this release's rows
                // needs to carry them, and the pointer written below is what
                // finds them again.
                let release_ref = crate::import::MetadataRef::new(release_id.clone(), *source);
                let payloads = library_manager
                    .load_release_payloads(&release_ref)
                    .await?
                    .ok_or_else(|| crate::import::ImportError::Internal {
                        detail: format!(
                            "{candidate_key}'s selected release payloads are not prepared"
                        ),
                    })?;
                let mut parsed = payloads.parsed_for_audio(
                    &audio_durations,
                    self.clock.as_ref(),
                    self.ids.as_ref(),
                )?;
                parsed.identities = crate::import::service::identities_with_partners(
                    library_manager,
                    parsed.identities,
                    partners,
                )
                .await?;
                parsed
            }
            Some(crate::import::MetadataProvenance::FileTags) => {
                let folder_name = folder
                    .file_name()
                    .and_then(|n| n.to_str())
                    .map(|s| s.to_string());
                let clock = self.clock.clone();
                let ids = self.ids.clone();
                let categorized_for_seed = categorized.clone();
                let snapshot = file_tag_snapshot
                    .expect("File Tags was paired with its snapshot above")
                    .clone();
                let parsed = tokio::task::spawn_blocking(move || {
                    crate::import::file_tag_mapper::map_file_tag_snapshot_to_db(
                        &categorized_for_seed,
                        &snapshot,
                        folder_name.as_deref(),
                        clock.as_ref(),
                        ids.as_ref(),
                    )
                })
                .await
                .map_err(|e| crate::import::ImportError::Internal {
                    detail: format!("File Tags mapping task failed: {e}"),
                })??;
                // A File Tags import claims no source release, so there is no
                // release cover to derive: its art comes from the folder or the
                // files' own tags.
                parsed
            }
            None => {
                let parsed = crate::import::direct_entry_mapper::map_direct_entry_candidate_to_db(
                    &categorized,
                    self.clock.as_ref(),
                    self.ids.as_ref(),
                );
                parsed
            }
        };

        // The release's track rows and the folder's audio are reconciled before
        // any metadata is applied, so everything downstream works on the track
        // list the import will actually write.
        let mut parsed = parsed;
        let mut user_edit = user_edit;
        let track_bindings = settle_track_rows(
            &mut parsed,
            &mut user_edit,
            &categorized,
            self.ids.as_ref(),
            self.clock.now(),
        );

        let mut prepared = self
            .reconcile_prepared_release(
                parsed,
                user_edit,
                &replacement_release_ids,
                &prepared_assets.artist_images,
            )
            .await?;

        prepared.db_release.source_folder_name = folder
            .file_name()
            .and_then(|n| n.to_str())
            .map(|s| s.to_string());
        prepared.db_release.content_hash = Some(content_hash);

        // The selected remote cover and its exact prepared bytes are one
        // candidate revision. Import validates that pair and never fetches it.
        let remote_cover_data = match (&selected_cover, prepared_assets.remote_cover) {
            (Some(CoverSelection::Remote(url, source)), Some(image)) => {
                Some(downloaded_cover(image, url, *source)?)
            }
            (Some(CoverSelection::Remote(_, _)), None) => {
                return Err(crate::import::ImportError::CoverArt {
                    detail: "selected remote cover has no prepared bytes".into(),
                });
            }
            (Some(CoverSelection::Local(_) | CoverSelection::Embedded(_)) | None, None) => None,
            (Some(CoverSelection::Local(_) | CoverSelection::Embedded(_)) | None, Some(_)) => {
                return Err(crate::import::ImportError::CoverArt {
                    detail: "prepared remote-cover bytes have no remote cover selection".into(),
                });
            }
        };

        let discovered_files = crate::import::handle::flatten_categorized_files(&categorized);

        // Each DbTrack moves into its TrackFile variant, bound to the audio its
        // slot named and carrying the `duration_ms` that audio yields. Past here
        // the DbTracks live in `tracks_to_files`.
        let tracks_to_files = resolve_track_files(
            std::mem::take(&mut prepared.db_tracks)
                .into_iter()
                .zip(track_bindings)
                .collect(),
            &categorized,
        )?;

        // File Tags captured its embedded image in the same snapshot that
        // seeded the pane, so the worker never opens the tags a second time.
        // A stored embedded selection names the source audio exactly; no
        // different snapshot image may silently stand in for it.
        let embedded_cover = if file_tag_snapshot.is_some() {
            let cover = file_tag_snapshot.and_then(|snapshot| snapshot.embedded_cover.as_ref());
            match (&selected_cover, cover) {
                (Some(CoverSelection::Embedded(source_file_id)), Some(cover))
                    if &cover.source_relative_path == source_file_id =>
                {
                    Some((cover.data.clone(), cover.content_type.clone()))
                }
                (Some(CoverSelection::Embedded(source_file_id)), Some(cover)) => {
                    return Err(crate::import::ImportError::CoverArt {
                        detail: format!(
                            "Selected embedded cover {source_file_id} does not match snapshot source {}",
                            cover.source_relative_path
                        ),
                    })
                }
                (Some(CoverSelection::Embedded(source_file_id)), None) => {
                    return Err(crate::import::ImportError::CoverArt {
                        detail: format!(
                            "Selected embedded cover {source_file_id} is absent from the File Tags snapshot"
                        ),
                    })
                }
                (None, Some(cover)) => Some((cover.data.clone(), cover.content_type.clone())),
                _ => None,
            }
        } else {
            None
        };

        // No storage yet: the winning cover's bytes go to coven's local store below
        // and its row is written by finalize.
        prepared.remote_cover_image = remote_cover_data;

        debug!(
            "Prepared album '{}' (release: {}) with {} tracks",
            prepared.db_album.title,
            prepared.db_release.id,
            tracks_to_files.len()
        );

        self.run_import(
            crate::db::ImportCommitGuard::Candidate {
                candidate_key: candidate_key.clone(),
                folder: folder.clone(),
                scope,
                expectation: expectation.clone(),
            },
            &storage_mode,
            pin,
            &mut prepared,
            &discovered_files,
            &tracks_to_files,
            selected_cover.as_ref(),
            &import_id,
            &candidate_key,
            embedded_cover,
            &replacement_plans,
        )
        .await?;

        let total_duration = import_start.elapsed();
        // The release is written (`run_import` succeeded via `?` above). Report
        // the real track count and the monotonic elapsed — never a zero default.
        self.library_manager
            .record_telemetry(TelemetryEvent::ImportCompleted {
                track_count: tracks_to_files.len() as u32,
                duration_ms: total_duration,
            });
        info!(
            "Imported '{}' in {:.0?}",
            prepared.album_title, total_duration,
        );

        Ok(())
    }

    /// Run an import. ONE path regardless of storage mode: build DbFile +
    /// audio-format records, reference the files in place, measure loudness,
    /// then finalize atomically as a LOCAL release (playable immediately) and
    /// emit events. No bytes move here, and every DB write lands in the single
    /// transaction at the end.
    ///
    /// A `Remote` import then transitions to the cloud via `coven_make_remote`,
    /// carrying `pin` as the upload's retain-pinned intent; coven flips `remote`
    /// true once the last upload lands. `pin` is ignored for a `Local` import.
    #[allow(clippy::too_many_arguments)]
    pub(super) async fn run_import(
        &self,
        commit_guard: crate::db::ImportCommitGuard,
        storage_mode: &StorageMode,
        pin: bool,
        prepared: &mut PreparedMetadata,
        discovered_files: &[ScannedFile],
        tracks_to_files: &[TrackFile],
        selected_cover: Option<&CoverSelection>,
        import_id: &str,
        candidate_key: &str,
        embedded_cover: Option<(Vec<u8>, crate::util::content_type::ContentType)>,
        replacement_plans: &[crate::library::manager::ImportReplacementPlan],
    ) -> Result<(), crate::import::ImportError> {
        let library_manager = &self.library_manager;
        let total_files = discovered_files.len();
        let PreparedMetadata {
            db_album,
            db_release,
            existing_album_id,
            remapped_track_artists,
            remapped_album_artists,
            work_graph,
            remapped_release_artist_roles,
            remapped_track_artist_roles,
            artists,
            artist_external_id_updates,
            artist_images,
            identities,
            remote_cover_image,
            ..
        } = prepared;
        let new_album = existing_album_id.is_none().then_some(&*db_album);
        let album_id = existing_album_id.as_deref().unwrap_or(&db_album.id);

        debug!(
            "Starting {} import for release {} ({} files)",
            storage_mode_label(storage_mode),
            db_release.id,
            total_files,
        );

        // Keyed by absolute path, the same key TrackFile uses, so disc-subfolder
        // siblings with identical bare filenames stay distinct.
        let files_now = library_manager.now();
        let mut prepared_files = Vec::with_capacity(total_files);
        let mut file_ids: HashMap<PathBuf, String> = HashMap::new();
        let mut source_audio_layouts = HashMap::new();
        for track_file in tracks_to_files {
            let (file_paths, layout): (Vec<&Path>, _) = match track_file {
                TrackFile::Standalone { file_path, .. } => (
                    vec![file_path.as_path()],
                    crate::album_detail::SourceAudioLayout::File,
                ),
                TrackFile::CueBacked { cue_pair, .. } => (
                    cue_pair
                        .audio_files
                        .iter()
                        .map(|audio| audio.path.as_path())
                        .collect(),
                    crate::album_detail::SourceAudioLayout::Cue,
                ),
            };
            for file_path in file_paths {
                if let Some(existing) = source_audio_layouts.insert(file_path, layout) {
                    if existing != layout {
                        return Err(crate::import::ImportError::Internal {
                            detail: format!(
                                "{} is assigned both whole-file and CUE source layouts",
                                file_path.display()
                            ),
                        });
                    }
                }
            }
        }
        let total_bytes: u128 = discovered_files
            .iter()
            .map(|file| u128::from(file.size))
            .sum();
        if total_bytes == 0 {
            return Err(crate::import::ImportError::Internal {
                detail: "stored import candidate contains no bytes".to_string(),
            });
        }
        self.emit_phase_progress(
            candidate_key,
            &db_release.id,
            Some(0),
            ImportPhase::ReadingFiles,
            import_id,
        );
        let mut bytes_read = 0u128;
        let mut last_release_percent = 0u8;
        for (idx, file) in discovered_files.iter().enumerate() {
            // The scan identity for every file was validated once before any
            // byte was read. Coven streams the file here and keeps the
            // resulting content identity opaque to bae.
            let bytes_read_before_file = bytes_read;
            let event_tx = self.event_tx.clone();
            let candidate_key_for_progress = candidate_key.to_string();
            let release_id_for_progress = db_release.id.clone();
            let import_id_for_progress = import_id.to_string();
            let reported_percent = Arc::new(std::sync::atomic::AtomicU8::new(last_release_percent));
            let reported_percent_for_progress = Arc::clone(&reported_percent);
            let prepared_blob = coven::prepare_external_blob(&file.path, move |consumed| {
                let completed = bytes_read_before_file + u128::from(consumed);
                let percent = ((completed * 100) / total_bytes).min(100) as u8;
                let previous = reported_percent_for_progress
                    .swap(percent, std::sync::atomic::Ordering::Relaxed);
                if previous != percent {
                    ImportService::emit_phase_progress_on(
                        &event_tx,
                        &candidate_key_for_progress,
                        &release_id_for_progress,
                        Some(percent),
                        ImportPhase::ReadingFiles,
                        &import_id_for_progress,
                    );
                }
            })
            .await
            .map_err(|error| crate::import::ImportError::UnusableFile {
                detail: format!(
                    "coven could not prepare {} for import: {error}",
                    file.path.display()
                ),
            })?;
            bytes_read += u128::from(file.size);
            last_release_percent = reported_percent.load(std::sync::atomic::Ordering::Relaxed);
            let mut db_file = DbFile::new(
                &db_release.id,
                &file.relative_path,
                file.size as i64,
                resolve_file_content_type(file)?,
                library_manager.new_id(),
                files_now,
            );
            db_file.source_audio =
                file.source_audio
                    .as_ref()
                    .map(|audio| crate::album_detail::SourceAudioFile {
                        layout: source_audio_layouts.get(file.path.as_path()).copied(),
                        format: audio.format.clone(),
                        content_type: audio.content_type.clone(),
                        duration_ms: i64::try_from(audio.duration_ms)
                            .expect("scan audio duration fits SQLite's integer range"),
                    });
            file_ids.insert(file.path.clone(), db_file.id.clone());
            prepared_files.push(PreparedImportFile {
                row: db_file,
                blob: prepared_blob,
            });
            debug!(
                "Read file {}/{}: {}",
                idx + 1,
                total_files,
                file.relative_path,
            );
        }

        let mut built_audio = Self::build_audio_formats(
            tracks_to_files,
            &file_ids,
            self.clock.as_ref(),
            self.ids.as_ref(),
        )?;
        let source_file_sizes: HashMap<PathBuf, u64> = discovered_files
            .iter()
            .map(|file| (file.path.clone(), file.size))
            .collect();

        // Measured from the source decode: bae stores originals verbatim (no
        // transcode), so source samples == stored samples. The sources are always
        // present here — every import references them in place and lands local, and
        // a remote import's uploads queue only after finalize. Per-track and album
        // NULLs are legitimate "not measured" results, each logged at its skip
        // point inside `measure_loudness`.
        //
        // Unconditional: `import::loudness` compiles under the same predicate this
        // module does, so there is no configuration where the import runs and the
        // measurement doesn't. A `cfg` here could say otherwise, and once did.
        let loudness = crate::import::loudness::measure_loudness(
            &self.event_tx,
            &mut built_audio.audio_formats,
            &built_audio.audio_segments,
            &file_ids,
            &source_file_sizes,
            tracks_to_files,
            candidate_key,
            &db_release.id,
            import_id,
        )
        .await?;
        db_release.album_loudness_lufs = loudness.album_loudness_lufs;
        db_release.album_peak_linear = loudness.album_peak_linear;

        // A track that didn't decode fully (fatal errors, a truncated body) would
        // import fine and then fail at play time. With verify on, fail now —
        // before finalize commits anything to the library.
        if self.library_manager.get_config().verify_decode_on_import && !loudness.broken.is_empty()
        {
            return Err(crate::import::ImportError::DecodeVerification {
                broken: loudness.broken,
            });
        }

        // A selected cover is exact. With no selection, File Tags' embedded
        // artwork leads, then the folder's deterministic image default.
        // Finalize writes the winner's bytes and row in one coven batch.
        let cover_candidate = match remote_cover_image.take() {
            Some(remote) => Some(remote),
            None => match selected_cover {
                Some(CoverSelection::Local(path)) => {
                    self.pick_folder_cover(discovered_files, Some(path))?
                }
                Some(CoverSelection::Embedded(_)) => {
                    embedded_cover.map(|(bytes, _content_type)| cover_image::CoverCandidate {
                        bytes,
                        source: "embedded".to_string(),
                        source_url: None,
                    })
                }
                Some(CoverSelection::Remote(_, _)) => {
                    return Err(crate::import::ImportError::CoverArt {
                        detail: "selected remote cover produced no downloaded image".to_string(),
                    })
                }
                None => match embedded_cover {
                    Some((bytes, _content_type)) => Some(cover_image::CoverCandidate {
                        bytes,
                        source: "embedded".to_string(),
                        source_url: None,
                    }),
                    None => self.pick_folder_cover(discovered_files, None)?,
                },
            },
        };
        // Resize the winner to a ≤600px JPEG thumbnail — one funnel for all three
        // sources — and build the row from that output, so its hash, size and
        // content type describe the blob that gets stored rather than the image it
        // was made from. `finalize_import_atomic` derives the readable
        // `cloud_path` extension from the same row.
        let cover_winner = match cover_candidate {
            Some(candidate) => {
                let bytes = crate::util::cover::resize_cover(&candidate.bytes)
                    .map_err(|detail| crate::import::ImportError::CoverArt { detail })?;
                let image = crate::db::DbLibraryImage::cover(
                    &db_release.id,
                    &library_manager.new_id(),
                    &candidate.source,
                    candidate.source_url,
                    &bytes,
                    library_manager.now(),
                );
                Some((image, bytes))
            }
            None => None,
        };
        let library_image = cover_winner
            .as_ref()
            .map(|(image, bytes)| (image, bytes.as_slice()));
        let artist_images: Vec<_> = artist_images
            .iter()
            .map(|(image, bytes)| (image, bytes.as_slice()))
            .collect();
        let cover_rel_id = Some((album_id, db_release.id.as_str()));

        self.emit_phase_progress(
            candidate_key,
            &db_release.id,
            None,
            ImportPhase::Finalizing,
            import_id,
        );

        let remote_intent = matches!(storage_mode, StorageMode::Remote);
        library_manager
            .finalize_import_atomic(
                commit_guard,
                new_album,
                db_release,
                tracks_to_files,
                remapped_track_artists,
                remapped_album_artists,
                &work_graph.works,
                &work_graph.work_artists,
                &work_graph.work_parts,
                &work_graph.track_works,
                remapped_release_artist_roles,
                remapped_track_artist_roles,
                artists,
                artist_external_id_updates,
                prepared_files,
                &built_audio.audio_formats,
                &built_audio.audio_segments,
                library_image,
                &artist_images,
                cover_rel_id,
                identities,
                replacement_plans,
            )
            .await?;

        // A Remote import transitions to the cloud in the background — the same
        // flow the "Make Remote" action runs: coven uploads each file from its
        // external (in-place) source, and on the last flips `remote` true, drops
        // the external refs, and re-emits the subtree (the cover rides along). The
        // user's original files stay where they are — coven never deletes a
        // user-provided source. This runs BEFORE the events below so the outbox
        // already holds the upload by the time any consumer observes the release
        // or `Complete`.
        let outbox_revision = if remote_intent {
            match library_manager.coven_make_remote(&db_release.id, pin).await {
                Ok(revision) => Some(revision),
                Err(e) => {
                    let remote_error = format!(
                        "Remote import of {} could not start its cloud upload: {e}",
                        db_release.id
                    );
                    if let Err(delete_error) = library_manager
                        .fail_import_and_delete_release(&db_release.id)
                        .await
                    {
                        return Err(crate::import::ImportError::Internal {
                            detail: format!(
                                "{remote_error}; removing the release it had already finalized failed: {delete_error}"
                            ),
                        });
                    }
                    return Err(crate::import::ImportError::Db(
                        crate::library::LibraryError::Storage(remote_error),
                    ));
                }
            }
        } else {
            None
        };

        let progress = if remote_intent {
            ImportProgress::RemoteUploadQueued {
                id: db_release.id.to_string(),
                import_id: import_id.to_string(),
                album_id: album_id.to_string(),
                outbox_revision: outbox_revision
                    .expect("a Remote import publishes its queued outbox revision"),
            }
        } else {
            ImportProgress::Complete {
                id: db_release.id.to_string(),
                import_id: import_id.to_string(),
                album_id: album_id.to_string(),
            }
        };
        send_event(
            &self.event_tx,
            crate::import::handle::ImportEvent::ImportProgress {
                candidate_key: candidate_key.to_string(),
                progress,
            },
        );

        info!("Import complete for release {}", db_release.id);
        Ok(())
    }
}
