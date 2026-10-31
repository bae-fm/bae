use super::*;
use crate::import::import_cancel::{ImportRunEnd, ImportStop};
use crate::import::types::TrackAudio;
use bae_util::worker_thread::WorkerThread;

impl ImportService {
    /// Start the import service worker: one task that drains the import queue
    /// sequentially, never concurrently. The returned handle is cloneable and
    /// is how the rest of the app submits import requests.
    pub(crate) async fn start(
        runtime_handle: tokio::runtime::Handle,
        library_manager: LibraryManager,
        database: crate::db::Database,
        diagnostics: crate::diagnostics::Diagnostics,
        preparations: crate::import::CandidatePreparations,
        clock: coven::ClockRef,
        ids: coven::IdSourceRef,
    ) -> Result<ImportServiceHandle, crate::import::ImportError> {
        // No run, import or open list outlives the process that held it, so
        // the library opens with no candidate standing and no list, before
        // anything reads the list.
        library_manager.clear_candidate_standings().await?;
        library_manager.close_import_lists().await?;
        let runtime = CandidateRuntime::new(database, diagnostics.clone());
        let event_tx = crate::import::handle::ImportEventBus::new(runtime.clone());
        let services = crate::import::ImportServices::new(
            event_tx,
            library_manager.clone(),
            preparations,
            clock.clone(),
            ids.clone(),
        );
        Ok(Self::start_with(
            runtime_handle,
            services,
            runtime,
            diagnostics,
        )?)
    }

    /// [`Self::start`] reading folders' tags through `file_tags`, for a test
    /// that decides how long a read takes.
    #[cfg(test)]
    pub(crate) fn start_reading_tags_with(
        runtime_handle: tokio::runtime::Handle,
        library_manager: LibraryManager,
        database: crate::db::Database,
        diagnostics: crate::diagnostics::Diagnostics,
        preparations: crate::import::CandidatePreparations,
        clock: coven::ClockRef,
        ids: coven::IdSourceRef,
        file_tags: Arc<dyn crate::import::file_tag_snapshot::FileTagReader>,
    ) -> ImportServiceHandle {
        let runtime = CandidateRuntime::new(database, diagnostics.clone());
        let event_tx = crate::import::handle::ImportEventBus::new(runtime.clone());
        let mut services =
            crate::import::ImportServices::new(event_tx, library_manager, preparations, clock, ids);
        services.file_tags = file_tags;
        Self::start_with(runtime_handle, services, runtime, diagnostics)
            .expect("a test's library is open while its import service starts")
    }

    fn start_with(
        runtime_handle: tokio::runtime::Handle,
        services: crate::import::ImportServices,
        runtime: CandidateRuntime,
        diagnostics: crate::diagnostics::Diagnostics,
    ) -> Result<ImportServiceHandle, crate::library::LibraryError> {
        let (fs_tx, fs_rx) = mpsc::unbounded_channel::<WatchReport>();
        let event_tx_for_worker = services.event_tx.clone();
        let runtime_for_worker = runtime.clone();
        let library_manager = services.library_manager.clone();
        let clock = services.clock.clone();
        let ids = services.ids.clone();
        let import_cancels = services.import_cancels.clone();

        // Constructed before the watcher task spawns; the task doesn't need the
        // watcher, only the `fs_rx` end of its event channel.
        let folder_watcher = Arc::new(FolderWatcher::new(fs_tx));

        let scan = ScanServices::new(services.clone(), folder_watcher);
        let watcher = WorkerThread::spawn("folder scan coordinator", move |_, watcher_rx| {
            ImportService::start_watcher(watcher_rx, fs_rx, scan)
        });

        let worker = WorkerThread::spawn("import worker thread", move |_, commands_rx| {
            std::thread::spawn(move || {
                let rt = tokio::runtime::Builder::new_current_thread()
                    .enable_all()
                    .build()
                    .expect("Failed to create import runtime");
                rt.block_on(async move {
                    let mut service = ImportService {
                        commands_rx,
                        event_tx: event_tx_for_worker,
                        runtime: runtime_for_worker,
                        library_manager,
                        clock,
                        ids,
                        import_cancels,
                        diagnostics,
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
            })
        });

        ImportServiceHandle::new(worker, watcher, services, runtime, runtime_handle)
    }

    pub(super) async fn do_import(&self, command: ImportCommand, expectation: ImportExpectation) {
        let import_id = command.import_id.clone();
        let candidate_key = command.candidate_key.clone();
        let content_hash = expectation.candidate.content_hash.clone();
        let edit_revision = expectation.candidate.file_edit_revision;
        let run = self
            .import_cancels
            .run(
                &candidate_key,
                &import_id,
                self.prepare_and_run_folder_import(
                    import_id.clone(),
                    candidate_key.clone(),
                    command.source,
                    expectation,
                    command.destination,
                ),
            )
            .await;
        let result = match run {
            ImportRunEnd::Ran(result) => result,
            // Whoever cancelled it while it waited already said it ended.
            ImportRunEnd::CancelledWaiting => return,
            // It wrote nothing and failed at nothing: no failure is recorded,
            // and its claim ends.
            ImportRunEnd::CancelledRunning => {
                info!("Import of {candidate_key} was cancelled");
                // No caller waits on the worker to hear it: a bug.
                if let Err(error) = self
                    .runtime
                    .move_import(&candidate_key, &import_id, None)
                    .await
                {
                    self.diagnostics.report_bug(
                        &format!("end the cancelled import of {candidate_key}"),
                        &error,
                    );
                }
                self.event_tx
                    .send(crate::import::handle::ImportEvent::ImportProgress {
                        candidate_key,
                        progress: ImportProgress::Cancelled { import_id },
                    });
                return;
            }
        };
        if let Err(e) = result {
            error!("Import failed: {}", e);
            self.library_manager
                .record_telemetry(TelemetryEvent::ImportFailed {});
            // The failure is recorded as what it is; the progress event carries
            // the error's diagnostic text, which the variant Displays build
            // from their `#[from]` source messages.
            let failed_at = self.library_manager.now();
            let failure = Self::terminal_failure(&e, failed_at);
            let error = e.to_string();

            // The row goes first and the event second: the event is what the
            // pane showing this import redraws from, and the row is what a
            // relaunched pane reads instead. The same write ends the import's
            // claim: the two commit or roll back together, so no read sees
            // the import ended without its failure. No caller waits on the
            // worker, so a write that will not land is a bug, reported with
            // the import's own failure, the one the person was waiting to see.
            if let Err(write) = self
                .library_manager
                .fail_candidate_import(
                    &content_hash,
                    edit_revision,
                    &failure,
                    &candidate_key,
                    &import_id,
                )
                .await
            {
                self.diagnostics.report_bug(
                    &format!("record the failed import of {candidate_key}"),
                    &format!("the import failed: {error}; recording that failed: {write}"),
                );
            }

            self.event_tx
                .send(crate::import::handle::ImportEvent::ImportProgress {
                    candidate_key,
                    progress: ImportProgress::Failed { error, import_id },
                });
        }
    }

    pub(crate) fn terminal_failure(
        error: &crate::import::ImportError,
        failed_at: chrono::DateTime<chrono::Utc>,
    ) -> crate::import::ImportFailure {
        let artist_identity_conflict = match error {
            crate::import::ImportError::Db(
                crate::library::LibraryError::ArtistIdentityConflict(conflict),
            ) => Some(conflict.as_ref().clone()),
            _ => None,
        };
        crate::import::ImportFailure {
            reason: crate::import::ImportFailureReason {
                detail: error.to_string(),
            },
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
        candidate_key: crate::import::CandidateKey,
        source: crate::import::release_candidate::CandidateSource,
        expectation: ImportExpectation,
        destination: ImportDestination,
    ) -> Result<(), ImportStop> {
        let library_manager = &self.library_manager;
        let expected_content_hash = expectation.candidate.content_hash.clone();
        let expected_edit_revision = expectation.candidate.file_edit_revision;

        let import_start = std::time::Instant::now();
        self.event_tx
            .send_once_written(
                crate::import::handle::ImportEvent::ImportProgress {
                    candidate_key: candidate_key.clone(),
                    progress: ImportProgress::Preparing {
                        import_id: import_id.clone(),
                        step: PrepareStep::ValidatingSourceFiles,
                    },
                },
                self.runtime.move_import(
                    &candidate_key,
                    &import_id,
                    Some(crate::import::ImportStanding::Running),
                ),
            )
            .await
            .map_err(crate::library::LibraryError::from)
            .map_err(crate::import::ImportError::from)?;

        let stored_candidate = match library_manager
            .load_release_candidate(&candidate_key)
            .await?
            .map_err(|reason| crate::import::ImportError::GroupingBlocked { reason })?
        {
            Some(candidate) => candidate,
            _ => {
                return Err(crate::import::ImportError::Internal {
                    detail: format!("{candidate_key} is no longer a valid stored import candidate"),
                }
                .into())
            }
        };
        if stored_candidate.source() != source
            || stored_candidate.files.content_hash() != expected_content_hash
            || stored_candidate.file_edit_revision != expected_edit_revision
        {
            return Err(crate::import::ImportError::Internal {
                detail: format!(
                    "{candidate_key} changed after it was selected; refresh and identify it again"
                ),
            }
            .into());
        }
        let identity_files = stored_candidate
            .files
            .release_files()
            .cloned()
            .collect::<Vec<_>>();
        let audio_observations = tokio::task::spawn_blocking(move || {
            crate::import::file_identity::validate_scanned_file_identities(&identity_files)
        })
        .await
        .map_err(|error| crate::import::ImportError::Internal {
            detail: format!("file identity validation task failed: {error}"),
        })??;
        let source_name = stored_candidate.name.to_string();
        let categorized = stored_candidate.files;

        let preparation = library_manager
            .load_import_candidate_preparation(&expected_content_hash)
            .await?
            .ok_or_else(|| crate::import::ImportError::Internal {
                detail: format!("{candidate_key} has no stored import preparation"),
            })?;
        if preparation.file_edit_revision != expected_edit_revision
            || preparation.metadata_revision != expectation.candidate.metadata_revision
        {
            return Err(crate::import::ImportError::Internal {
                detail: format!(
                    "{candidate_key}'s preparation changed after it was queued; import it again"
                ),
            }
            .into());
        }
        let release_link = preparation.release_link;
        let selected_cover = preparation.cover;
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
                }
                .into());
            }
        }
        if matches!(selected_cover, Some(CoverSelection::Embedded(_)))
            && file_tag_snapshot.is_none()
        {
            return Err(crate::import::ImportError::Internal {
                detail: format!("{candidate_key}'s embedded cover has no prepared tag snapshot"),
            }
            .into());
        }

        let records = match &release_link {
            Some(link) => linked_records(library_manager, link).await?,
            None => Vec::new(),
        };
        // The draft holds every value the release is written with; nothing
        // is read from its source again.
        let assembled = crate::import::assemble::assemble_release(
            &preparation.draft,
            self.clock.as_ref(),
            self.ids.as_ref(),
        )?;
        let track_bindings = assembled.bindings;

        let picked_album = library_manager
            .candidate_album_pick(&categorized.content_hash())
            .await?;
        let mut prepared = self
            .reconcile_prepared_release(assembled.parsed, records, picked_album, assembled.picked)
            .await?;

        prepared.db_release.source_folder_name = Some(source_name);

        // The pictures of the Discogs artists this import creates are fetched
        // now, before any file is read or row written, the one thing an
        // import asks a provider for. A fetch that fails fails the import;
        // importing again fetches again.
        let new_artists = library_manager
            .new_discogs_artists(
                &prepared.artist_credits,
                &prepared.picked_artists,
                Some(&categorized.content_hash()),
            )
            .await?;
        if !new_artists.is_empty() {
            self.event_tx
                .send(crate::import::handle::ImportEvent::ImportProgress {
                    candidate_key: candidate_key.clone(),
                    progress: ImportProgress::Preparing {
                        import_id: import_id.clone(),
                        step: PrepareStep::FetchingArtistPictures,
                    },
                });
        }
        let artist_images = library_manager
            .fetch_discogs_artist_images(new_artists)
            .await?;

        prepared.selected_cover = selected_cover.clone();

        // The selected remote cover and its exact prepared bytes are one
        // candidate revision. Import validates that pair and never fetches it.
        let remote_cover_data = match (&selected_cover, prepared_assets.remote_cover) {
            (Some(CoverSelection::Remote(remote, source)), Some(image)) => {
                Some(downloaded_cover(image, &remote.url, *source)?)
            }
            (Some(CoverSelection::Remote(_, _)), None) => {
                return Err(crate::import::ImportError::Internal {
                    detail: "selected remote cover has no prepared bytes".into(),
                }
                .into());
            }
            (Some(CoverSelection::Local(_) | CoverSelection::Embedded(_)) | None, None) => None,
            (Some(CoverSelection::Local(_) | CoverSelection::Embedded(_)) | None, Some(_)) => {
                return Err(crate::import::ImportError::Internal {
                    detail: "prepared remote-cover bytes have no remote cover selection".into(),
                }
                .into());
            }
        };

        // Each track row becomes a TrackFile's DbTrack, bound to the audio
        // its slot named and measured by it. Past here the tracks live in
        // `tracks_to_files`.
        let tracks_to_files = resolve_track_files(
            std::mem::take(&mut prepared.track_rows)
                .into_iter()
                .zip(track_bindings)
                .collect(),
            &categorized,
        )?;

        // File metadata captured its embedded image in the same snapshot that
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
                    return Err(crate::import::ImportError::Internal {
                        detail: format!(
                            "Selected embedded cover {source_file_id} does not match snapshot source {}",
                            cover.source_relative_path
                        ),
                    }
                    .into())
                }
                (Some(CoverSelection::Embedded(source_file_id)), None) => {
                    return Err(crate::import::ImportError::Internal {
                        detail: format!(
                            "Selected embedded cover {source_file_id} is absent from the file-tag snapshot"
                        ),
                    }
                    .into())
                }
                _ => None,
            }
        } else {
            None
        };

        // No storage yet: the winning cover's bytes go to coven's local store below
        // and its row is written by finalize.
        prepared.remote_cover_image = remote_cover_data;
        prepared.embedded_cover = embedded_cover;

        debug!(
            "Prepared album '{}' (release: {}) with {} tracks",
            prepared.db_album.title,
            prepared.db_release.id,
            tracks_to_files.len()
        );

        self.run_import(
            CommittedCandidate {
                source,
                expectation: expectation.clone(),
            },
            ImportRun {
                import_id: &import_id,
                candidate_key: &candidate_key,
            },
            destination,
            &mut prepared,
            ImportFiles {
                discovered: &categorized.files,
                tracks: &tracks_to_files,
            },
            &artist_images,
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

    /// Run an import: reference the files in place, measure loudness, and write
    /// the release in one transaction with the pictures `artist_images`
    /// fetched for its new artists, recording a `Remote` destination's
    /// uploads in that same write.
    ///
    /// The same transaction ends the import's claim on the candidate: no read
    /// sees the release beside the running import, or the import gone
    /// without its release. Its steps write where it stands — running once it
    /// is taken up, writing once it writes — before they are announced, and
    /// a step that cannot be written fails the import.
    pub(super) async fn run_import(
        &self,
        committed: CommittedCandidate,
        run: ImportRun<'_>,
        destination: ImportDestination,
        prepared: &mut PreparedMetadata,
        files: ImportFiles<'_>,
        artist_images: &[crate::import::DiscogsArtistImage],
    ) -> Result<(), ImportStop> {
        let library_manager = &self.library_manager;
        let ImportFiles {
            discovered: discovered_files,
            tracks: tracks_to_files,
        } = files;
        let ImportRun {
            import_id,
            candidate_key,
        } = run;
        let total_files = discovered_files.len();
        let PreparedMetadata {
            db_album,
            db_release,
            mediums,
            existing_album_id,
            track_artists,
            album_artists,
            work_graph,
            release_artist_roles,
            track_artist_roles,
            artist_credits,
            picked_artists,
            records,
            selected_cover,
            remote_cover_image,
            embedded_cover,
            ..
        } = prepared;
        let new_album = existing_album_id.is_none().then_some(&*db_album);
        let album_id = existing_album_id.as_deref().unwrap_or(&db_album.id);

        debug!(
            "Starting {} import for release {} ({} files)",
            destination_label(destination),
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
            let (file_paths, layout): (Vec<&Path>, _) = match &track_file.audio {
                TrackAudio::Standalone { file_path, .. } => (
                    vec![file_path.as_path()],
                    crate::album_detail::SourceAudioLayout::File,
                ),
                TrackAudio::CueBacked { cue_pair, .. } => (
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
                        }
                        .into());
                    }
                }
            }
        }
        let total_bytes: u128 = discovered_files
            .iter()
            .map(|entry| u128::from(entry.file.size))
            .sum();
        if total_bytes == 0 {
            return Err(crate::import::ImportError::Internal {
                detail: "stored import candidate contains no bytes".to_string(),
            }
            .into());
        }
        self.emit_phase_progress(run, &db_release.id, Some(0), ImportPhase::ReadingFiles);
        let mut bytes_read = 0u128;
        let mut last_release_percent = 0u8;
        for (idx, entry) in discovered_files.iter().enumerate() {
            let file = &entry.file;
            // The scan identity for every file was validated once before any
            // byte was read. Coven streams the file here and keeps the
            // resulting content identity opaque to bae.
            let bytes_read_before_file = bytes_read;
            let event_tx = self.event_tx.clone();
            let candidate_key_for_progress = candidate_key.clone();
            let release_id_for_progress = db_release.id.clone();
            let import_id_for_progress = import_id.to_string();
            let reported_percent = Arc::new(std::sync::atomic::AtomicU8::new(last_release_percent));
            let reported_percent_for_progress = Arc::clone(&reported_percent);
            let original = coven::UserFile {
                path: file.path.clone(),
                size: file.size,
                modified_at: chrono::DateTime::from_timestamp_nanos(file.modified_at_ns).into(),
            };
            let prepared_source = match destination {
                ImportDestination::Local => PreparedImportSource::Local(original),
                ImportDestination::Remote { pin } => PreparedImportSource::Shared {
                    original,
                    keep_here: pin,
                    prepared: Box::pin(coven::prepare_user_file(&file.path, move |consumed| {
                        let completed = bytes_read_before_file + u128::from(consumed);
                        let percent = ((completed * 100) / total_bytes).min(100) as u8;
                        let previous = reported_percent_for_progress
                            .swap(percent, std::sync::atomic::Ordering::Relaxed);
                        if previous != percent {
                            ImportService::emit_phase_progress_on(
                                &event_tx,
                                ImportRun {
                                    import_id: &import_id_for_progress,
                                    candidate_key: &candidate_key_for_progress,
                                },
                                &release_id_for_progress,
                                Some(percent),
                                ImportPhase::ReadingFiles,
                            );
                        }
                    }))
                    .await
                    .map_err(|error| {
                        crate::import::ImportError::UnusableFile {
                            detail: format!(
                                "coven could not prepare {} for import: {error}",
                                file.path.display()
                            ),
                        }
                    })?,
                },
            };
            bytes_read += u128::from(file.size);
            let completed_percent = ((bytes_read * 100) / total_bytes).min(100) as u8;
            if reported_percent.load(std::sync::atomic::Ordering::Relaxed) != completed_percent {
                self.emit_phase_progress(
                    run,
                    &db_release.id,
                    Some(completed_percent),
                    ImportPhase::ReadingFiles,
                );
            }
            last_release_percent = completed_percent;
            let mut db_file = DbFile::new(
                &db_release.id,
                &file.relative_path,
                file.size as i64,
                resolve_file_content_type(entry)?,
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
                        duration_ms: audio.duration_ms,
                    });
            file_ids.insert(file.path.clone(), db_file.id.clone());
            prepared_files.push(PreparedImportFile {
                row: db_file,
                source: prepared_source,
            });
            debug!(
                "Read file {}/{}: {}",
                idx + 1,
                total_files,
                file.relative_path,
            );
        }

        // Byte landings seek through each CUE image with FFmpeg: blocking file
        // reads, so they run off the worker's runtime.
        let mut built_audio = {
            let tracks = tracks_to_files.to_vec();
            let file_ids = file_ids.clone();
            let clock = self.clock.clone();
            let ids = self.ids.clone();
            let diagnostics = self.diagnostics.clone();
            tokio::task::spawn_blocking(move || {
                Self::build_audio_formats(
                    &tracks,
                    &file_ids,
                    clock.as_ref(),
                    ids.as_ref(),
                    &diagnostics,
                )
            })
            .await
            .map_err(|e| crate::import::ImportError::Internal {
                detail: format!("audio format task failed: {e}"),
            })??
        };
        let source_file_sizes: HashMap<PathBuf, u64> = discovered_files
            .iter()
            .map(|entry| (entry.file.path.clone(), entry.file.size))
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
        // Decoding is CPU work: measure as many tracks at once as there are
        // cores, which also bounds the source files open at once.
        let parallelism = std::thread::available_parallelism().unwrap_or_else(|error| {
            warn!(
                "could not read the available parallelism ({error}); measuring one track at a time"
            );
            std::num::NonZeroUsize::MIN
        });
        let loudness = crate::import::loudness::measure_loudness(
            &self.event_tx,
            parallelism,
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
        if self
            .library_manager
            .get_config()
            .prefs
            .verify_decode_on_import
            && !loudness.broken.is_empty()
        {
            return Err(crate::import::ImportError::DecodeVerification {
                broken: loudness.broken,
            }
            .into());
        }

        // The candidate's stored cover selection is what it commits with, and
        // the only thing: the image a person chose, the one its
        // identification fetched, or the folder's own cover the scan stored
        // for it. Nothing is chosen here. Finalize writes the winner's bytes
        // and row in one coven batch.
        let cover_candidate = match selected_cover.as_ref() {
            Some(CoverSelection::Remote(_, _)) => match remote_cover_image.take() {
                Some(remote) => Some(remote),
                None => {
                    return Err(crate::import::ImportError::Internal {
                        detail: "selected remote cover produced no downloaded image".to_string(),
                    }
                    .into())
                }
            },
            Some(CoverSelection::Local(path)) => {
                Self::pick_folder_cover(discovered_files, path).await?
            }
            Some(CoverSelection::Embedded(_)) => {
                embedded_cover
                    .take()
                    .map(|(bytes, _content_type)| cover_image::CoverCandidate {
                        bytes,
                        source: "embedded".to_string(),
                        source_url: None,
                    })
            }
            None => None,
        };
        // Resize the winner to a ≤600px JPEG thumbnail — one funnel for all three
        // sources — and build the row from that output, so its hash, size and
        // content type describe the file that gets stored rather than its source.
        let cover_winner = match cover_candidate {
            Some(candidate) => {
                // Decoding and re-encoding the image is CPU work.
                let source = candidate.bytes;
                let bytes = tokio::task::spawn_blocking(move || {
                    bae_util::stored_image::stored_jpeg(&source)
                })
                .await
                .map_err(|e| crate::import::ImportError::Internal {
                    detail: format!("cover resize task failed: {e}"),
                })?
                .map_err(|detail| crate::import::ImportError::CoverArt { detail })?;
                let image = crate::db::DbLibraryImage::cover(
                    &db_release.id,
                    &candidate.source,
                    candidate.source_url,
                    library_manager.now(),
                );
                Some((image, bytes))
            }
            None => None,
        };
        let library_image = cover_winner
            .as_ref()
            .map(|(image, bytes)| (image, bytes.as_slice()));

        // Past here the release is written in one transaction, which a cancel
        // no longer interrupts; one that came first stops the import here.
        self.import_cancels
            .begin_writing(candidate_key, import_id)?;
        self.event_tx
            .send_once_written(
                Self::phase_progress(run, &db_release.id, None, ImportPhase::Finalizing),
                self.runtime.move_import(
                    candidate_key,
                    import_id,
                    Some(crate::import::ImportStanding::Writing),
                ),
            )
            .await
            .map_err(crate::library::LibraryError::from)
            .map_err(crate::import::ImportError::from)?;

        let commit_guard = crate::db::ImportCommitGuard::Candidate {
            candidate_key: candidate_key.clone(),
            source: committed.source,
            expectation: committed.expectation,
            import_id: import_id.to_string(),
        };
        // Attaching the originals queues their uploads in the import transaction.
        library_manager
            .finalize_import_atomic(
                commit_guard,
                new_album,
                db_release,
                tracks_to_files,
                crate::db::ImportRows {
                    mediums,
                    track_artists,
                    album_artists,
                    works: &work_graph.works,
                    work_artists: &work_graph.work_artists,
                    work_parts: &work_graph.work_parts,
                    track_works: &work_graph.track_works,
                    release_artist_roles,
                    track_artist_roles,
                    artists: crate::db::ArtistCredits {
                        credits: artist_credits,
                        picked: picked_artists,
                    },
                    audio_formats: &built_audio.audio_formats,
                    audio_segments: &built_audio.audio_segments,
                    scrub_points: &loudness.scrub_points,
                    records,
                },
                prepared_files,
                library_image,
                artist_images,
            )
            .await?;

        let progress = match destination {
            ImportDestination::Remote { .. } => ImportProgress::RemoteUploadQueued {
                id: db_release.id.to_string(),
                import_id: import_id.to_string(),
                album_id: album_id.to_string(),
            },
            ImportDestination::Local => ImportProgress::Complete {
                id: db_release.id.to_string(),
                import_id: import_id.to_string(),
                album_id: album_id.to_string(),
            },
        };
        self.event_tx
            .send(crate::import::handle::ImportEvent::ImportProgress {
                candidate_key: candidate_key.clone(),
                progress,
            });

        info!("Import complete for release {}", db_release.id);
        Ok(())
    }
}

/// The records an import of a candidate linked to `link` stores.
///
/// A linked album is stored as each catalog's album record. A linked
/// pressing is stored as every catalog its stored releases describe it in,
/// the linked primary's own record first.
async fn linked_records(
    library_manager: &LibraryManager,
    link: &crate::import::ReleaseLink,
) -> Result<Vec<crate::import::ReleaseRecord>, crate::import::ImportError> {
    let link = match link {
        crate::import::ReleaseLink::Album(album) => return Ok(album.records()),
        crate::import::ReleaseLink::Pressing(pressing) => pressing,
    };
    let mut claimed = Vec::new();
    for release in link.claimed() {
        claimed.push(
            library_manager
                .load_source_release(release)
                .await?
                .ok_or_else(|| crate::import::ImportError::Internal {
                    detail: format!(
                        "the candidate is linked to {} release {} that nothing stored",
                        release.catalog.as_str(),
                        release.key
                    ),
                })?,
        );
    }
    let (primary, partners) = claimed
        .split_first()
        .expect("a link claims at least its primary");
    Ok(records_for_commit(primary, partners))
}
