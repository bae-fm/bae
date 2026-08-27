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
                command.selected_cover,
                command.storage_mode,
                command.pin,
                command.metadata_seed,
                command.user_edit,
            )
            .await;

        if let Err(e) = result {
            error!("Import failed: {}", e);
            self.library_manager
                .record_telemetry(TelemetryEvent::ImportFailed {});
            // The typed error becomes a user-facing string only here, at the
            // pipeline's terminal consumer. The variant Displays embed their
            // `#[from]` source messages, so `to_string()` carries the chain.
            let error = e.to_string();

            // The row goes first and the event second: the event is what the
            // pane showing this import redraws from, and the row is what a
            // relaunched pane reads instead. A row that will not write is
            // logged rather than swallowing the failure the user is waiting
            // to see.
            if let Err(write) = self
                .library_manager
                .save_import_candidate_failure(&content_hash, &folder_path, edit_revision, &error)
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

    /// Prepare and run a folder import by dispatching on its stored metadata
    /// seed, then walk the folder and run the shared mapping and write path.
    pub(super) async fn prepare_and_run_folder_import(
        &self,
        import_id: String,
        candidate_key: String,
        folder: PathBuf,
        scope: crate::import::folder_scanner::ReleaseFileScope,
        expectation: ImportExpectation,
        selected_cover: Option<CoverSelection>,
        storage_mode: StorageMode,
        pin: bool,
        metadata_seed: crate::import::MetadataSeed,
        user_edit: Option<crate::import::ReleaseUserEdit>,
    ) -> Result<(), crate::import::ImportError> {
        let library_manager = &self.library_manager;
        let expected_content_hash = expectation.content_hash().to_string();
        let expected_edit_revision = expectation.edit_revision();

        let import_start = std::time::Instant::now();
        let mut step_times: Vec<(&str, std::time::Duration)> = Vec::new();
        let mut last_step_start = import_start;

        send_event(
            &self.event_tx,
            crate::import::handle::ImportEvent::ImportProgress {
                candidate_key: candidate_key.clone(),
                progress: ImportProgress::Preparing {
                    import_id: import_id.clone(),
                    step: PrepareStep::ReadingFolder,
                    album_title: String::new(),
                    artist_name: String::new(),
                },
            },
        );

        // Re-walk the folder. Scan and commit are separated by user interaction,
        // and the user can move, rename, or reorganize in that window — so the
        // worker treats the disk at commit time as the source of truth. Their
        // sheet bindings come with it: what the folder is includes what they
        // said it is, and a commit that re-derived without them would import
        // the shape they corrected.
        let folder_buf = folder.clone();
        let stored_edits = library_manager.load_stored_candidate_edits().await?;
        let current_edit_revision = stored_edits.revision_for_hash(&expected_content_hash);
        let categorized = tokio::task::spawn_blocking(move || {
            crate::import::folder_scanner::collect_release_candidate_files_with_scope(
                &folder_buf,
                scope,
                &stored_edits,
            )
        })
        .await
        .map_err(|e| crate::import::ImportError::Internal {
            detail: format!("Folder scan task failed: {e}"),
        })??;
        if categorized.content_hash() != expected_content_hash
            || current_edit_revision != expected_edit_revision
        {
            return Err(crate::import::ImportError::Internal {
                detail: format!(
                    "{candidate_key} changed after it was selected; refresh and identify it again"
                ),
            });
        }

        let file_tag_snapshot = match (&metadata_seed, &expectation) {
            (
                crate::import::MetadataSeed::FileTags,
                ImportExpectation::FileTags { snapshot, .. },
            ) => {
                let audio_files = categorized.audio().cloned().collect::<Vec<_>>();
                let current_observations = tokio::task::spawn_blocking(move || {
                    crate::import::file_tag_snapshot::observe_audio_files(&audio_files)
                })
                .await
                .map_err(|error| crate::import::ImportError::Internal {
                    detail: format!("file-tag validation task failed: {error}"),
                })??;
                if !snapshot
                    .files
                    .iter()
                    .map(|fact| &fact.observation)
                    .eq(current_observations.iter())
                {
                    return Err(crate::import::ImportError::FileTags {
                        detail: format!(
                            "{candidate_key}'s audio changed after its file tags were read"
                        ),
                    });
                }
                Some(snapshot)
            }
            (crate::import::MetadataSeed::FileTags, ImportExpectation::Candidate { .. }) => {
                return Err(crate::import::ImportError::Internal {
                    detail: format!("{candidate_key}'s File Tags import has no metadata snapshot"),
                });
            }
            (_, ImportExpectation::FileTags { .. }) => {
                return Err(crate::import::ImportError::Internal {
                    detail: format!(
                        "{candidate_key}'s metadata source changed after its file tags were read"
                    ),
                });
            }
            (_, ImportExpectation::Candidate { .. }) => None,
        };

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

        send_event(
            &self.event_tx,
            crate::import::handle::ImportEvent::ImportProgress {
                candidate_key: candidate_key.clone(),
                progress: ImportProgress::Preparing {
                    import_id: import_id.clone(),
                    step: PrepareStep::ParsingMetadata,
                    album_title: String::new(),
                    artist_name: String::new(),
                },
            },
        );

        let (parsed, release_cover) = match &metadata_seed {
            crate::import::MetadataSeed::ExternalRelease { source, release_id } => {
                // The documents are archived by `prepare_release`, keyed by the
                // picked source release — so nothing about this release's rows
                // needs to carry them, and the pointer written below is what
                // finds them again.
                let release_ref = crate::import::MetadataRef::new(release_id.clone(), *source);
                let payloads =
                    prepare_release(library_manager, &release_ref, CallPriority::Interactive)
                        .await?;
                let parsed = payloads.parsed(self.clock.as_ref(), self.ids.as_ref())?;
                (parsed, payloads.default_cover()?)
            }
            crate::import::MetadataSeed::FileTags => {
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
                    detail: format!("unknown-seed mapping task failed: {e}"),
                })??;
                // A File Tags import claims no source release, so there is no
                // release cover to derive: its art comes from the folder or the
                // files' own tags.
                (parsed, None)
            }
            crate::import::MetadataSeed::Manual => {
                let parsed = crate::import::manual_mapper::map_manual_candidate_to_db(
                    &categorized,
                    self.clock.as_ref(),
                    self.ids.as_ref(),
                );
                (parsed, None)
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
            .reconcile_prepared_release(parsed, user_edit, &replacement_release_ids)
            .await?;

        let emit_preparing = {
            let import_id = import_id.clone();
            let candidate_key = candidate_key.clone();
            let album_title = prepared.album_title.clone();
            let artist_name = prepared.artist_name.clone();
            let event_tx = self.event_tx.clone();
            move |step: PrepareStep| {
                send_event(
                    &event_tx,
                    crate::import::handle::ImportEvent::ImportProgress {
                        candidate_key: candidate_key.clone(),
                        progress: ImportProgress::Preparing {
                            import_id: import_id.clone(),
                            step,
                            album_title: album_title.clone(),
                            artist_name: artist_name.clone(),
                        },
                    },
                );
            }
        };

        step_times.push(("resolve_metadata", last_step_start.elapsed()));
        last_step_start = std::time::Instant::now();

        prepared.db_release.source_folder_name = folder
            .file_name()
            .and_then(|n| n.to_str())
            .map(|s| s.to_string());
        prepared.db_release.content_hash = Some(content_hash);

        // Which cover this import downloads. A pick the command carries is the
        // user's, and must be there. With no pick, the release's own first
        // cover option is the one the confirmation pane offered — the pane
        // seeds its selection from exactly this list — so the commit fetches
        // that rather than reading "the command names no cover" as "no cover
        // wanted", which is what imported releases bare whenever the pane's
        // options came up empty.
        let remote_cover_data = match (&selected_cover, &release_cover) {
            (Some(CoverSelection::Remote(url, source)), _) => {
                emit_preparing(PrepareStep::WritingCoverArt);
                let image = self
                    .library_manager
                    .fetch_required_remote_image(url)
                    .await?;
                Some(downloaded_cover(image, url, *source)?)
            }
            (None, Some(cover)) => {
                emit_preparing(PrepareStep::WritingCoverArt);
                // A source that answers "no image here" has dropped the art
                // since its document was stored — an answer, and one that
                // leaves the release to whatever its folder holds. A fetch that
                // *fails* fails the import: a cover the source says exists must
                // not go missing quietly.
                match self.library_manager.fetch_remote_image(&cover.url).await? {
                    Some(image) => Some(downloaded_cover(image, &cover.url, cover.source)?),
                    None => {
                        warn!(
                            "{} serves no image at {}; importing with whatever the folder holds",
                            cover.label, cover.url
                        );
                        None
                    }
                }
            }
            (Some(CoverSelection::Local(_)), _) | (None, None) => None,
        };

        step_times.push(("write_cover_art", last_step_start.elapsed()));
        last_step_start = std::time::Instant::now();

        emit_preparing(PrepareStep::DiscoveringFiles);
        let discovered_files = crate::import::handle::flatten_categorized_files(&categorized);

        step_times.push(("discover_files", last_step_start.elapsed()));
        last_step_start = std::time::Instant::now();

        // Each DbTrack moves into its TrackFile variant, bound to the audio its
        // slot named and carrying the `duration_ms` that audio yields. Past here
        // the DbTracks live in `tracks_to_files`.
        emit_preparing(PrepareStep::ValidatingTracks);
        let tracks_to_files = resolve_track_files(
            std::mem::take(&mut prepared.db_tracks)
                .into_iter()
                .zip(track_bindings)
                .collect(),
            &categorized,
        )?;

        let selected_cover_path = match &selected_cover {
            Some(CoverSelection::Local(path)) => Some(path.as_str()),
            _ => None,
        };

        // Embedded cover art is the lowest-priority source: `run_import` uses it
        // only when neither an explicit pick nor a folder image supplies one.
        // File Tags captured its embedded image in the same snapshot that
        // seeded the pane, so the worker never opens the tags a second time.
        let embedded_cover = if selected_cover.is_none()
            && matches!(metadata_seed, crate::import::MetadataSeed::FileTags)
        {
            file_tag_snapshot
                .and_then(|snapshot| snapshot.embedded_cover.as_ref())
                .map(|cover| (cover.data.clone(), cover.content_type.clone()))
        } else {
            None
        };

        step_times.push(("validate_tracks", last_step_start.elapsed()));
        last_step_start = std::time::Instant::now();

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
            &storage_mode,
            pin,
            &mut prepared,
            &discovered_files,
            &tracks_to_files,
            selected_cover_path,
            &import_id,
            &candidate_key,
            embedded_cover,
            &replacement_plans,
        )
        .await?;

        step_times.push(("storage", last_step_start.elapsed()));

        let total_duration = import_start.elapsed();
        // The release is written (`run_import` succeeded via `?` above). Report
        // the real track count and the monotonic elapsed — never a zero default.
        self.library_manager
            .record_telemetry(TelemetryEvent::ImportCompleted {
                track_count: tracks_to_files.len() as u32,
                duration_ms: total_duration,
            });
        let step_summary: Vec<String> = step_times
            .iter()
            .map(|(name, dur)| format!("{}={:.0?}", name, dur))
            .collect();
        info!(
            "Import timing for '{}': total={:.0?} [{}]",
            prepared.album_title,
            total_duration,
            step_summary.join(", ")
        );

        if std::env::var("BAE_IMPORT_TRACE").is_ok_and(|v| v == "1") {
            if let Some(home) = std::env::var_os("HOME") {
                let trace_dir = PathBuf::from(home).join(".bae-traces");
                if let Err(e) = std::fs::create_dir_all(&trace_dir) {
                    warn!("import trace dir {:?}: {}", trace_dir, e);
                }
                let trace_path = trace_dir.join("imports.jsonl");
                let line = import_trace_line(
                    library_manager.now().to_rfc3339(),
                    &import_id,
                    &prepared.album_title,
                    &prepared.artist_name,
                    total_duration,
                    &step_times,
                );
                match std::fs::OpenOptions::new()
                    .create(true)
                    .append(true)
                    .open(&trace_path)
                {
                    Ok(mut f) => {
                        use std::io::Write;
                        if let Err(e) = f.write_all(line.as_bytes()) {
                            warn!("import trace write {:?}: {}", trace_path, e);
                        }
                    }
                    Err(e) => warn!("import trace open {:?}: {}", trace_path, e),
                }
            }
        }

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
        storage_mode: &StorageMode,
        pin: bool,
        prepared: &mut PreparedMetadata,
        discovered_files: &[ScannedFile],
        tracks_to_files: &[TrackFile],
        selected_cover_path: Option<&str>,
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
        let mut db_files: Vec<DbFile> = Vec::with_capacity(total_files);
        let mut file_ids: HashMap<PathBuf, String> = HashMap::new();
        let file_to_tracks: HashMap<PathBuf, Vec<String>> = {
            let mut map: HashMap<PathBuf, Vec<String>> = HashMap::new();
            for tf in tracks_to_files {
                map.entry(tf.file_path().to_path_buf())
                    .or_default()
                    .push(tf.db_track().id.clone());
            }
            map
        };
        self.emit_phase_progress(
            candidate_key,
            &db_release.id,
            0,
            ImportPhase::ReadingFiles,
            import_id,
        );
        for (idx, file) in discovered_files.iter().enumerate() {
            // coven verifies this blob's bytes against this hash on every
            // cloud fetch — required so a later make-Remote + pin round trip
            // (or another device's download) can ever read it back. See
            // `crate::util::fs::hash_file`.
            let content_hash = crate::util::fs::hash_file(&file.path).map_err(|e| {
                crate::import::ImportError::UnusableFile {
                    detail: format!("failed to hash {}: {e}", file.path.display()),
                }
            })?;
            let db_file = DbFile::new(
                &db_release.id,
                &file.relative_path,
                file.size as i64,
                resolve_file_content_type(&file.path)?,
                library_manager.new_id(),
                files_now,
                content_hash,
            );
            file_ids.insert(file.path.clone(), db_file.id.clone());
            db_files.push(db_file);
            if let Some(track_ids) = file_to_tracks.get(&file.path) {
                for track_id in track_ids {
                    self.emit_phase_progress(
                        candidate_key,
                        track_id,
                        100,
                        ImportPhase::ReadingFiles,
                        import_id,
                    );
                }
            }
            let release_percent = ((idx + 1) * 100 / total_files.max(1)) as u8;
            self.emit_phase_progress(
                candidate_key,
                &db_release.id,
                release_percent,
                ImportPhase::ReadingFiles,
                import_id,
            );
            debug!(
                "Read file {}/{}: {}",
                idx + 1,
                total_files,
                file.relative_path,
            );
        }

        // Every import lands LOCAL: reference the files in place and record their
        // common-ancestor folder as the release's local source. Until a Remote
        // import's upload lands it stays a valid, playable local release, so
        // another device never sees a release before its audio is in the cloud.
        let local_root = {
            let mut ancestor: Option<&Path> = None;
            for file in discovered_files.iter() {
                let parent =
                    file.path
                        .parent()
                        .ok_or_else(|| crate::import::ImportError::Internal {
                            detail: format!("File has no parent: {:?}", file.path),
                        })?;
                ancestor = Some(match ancestor {
                    None => parent,
                    Some(a) => common_ancestor(a, parent),
                });
            }
            ancestor.ok_or_else(|| crate::import::ImportError::Internal {
                detail: "No files to determine local path".to_string(),
            })?
        };
        let local_path = local_root
            .to_str()
            .ok_or_else(|| crate::import::ImportError::Internal {
                detail: format!("Cannot convert path to string: {:?}", local_root),
            })?
            .to_string();

        let mut built_audio = Self::build_audio_formats(
            tracks_to_files,
            &file_ids,
            self.clock.as_ref(),
            self.ids.as_ref(),
        )?;

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
        self.emit_phase_progress(
            candidate_key,
            &db_release.id,
            0,
            ImportPhase::MeasuringLoudness,
            import_id,
        );
        let loudness = crate::import::loudness::measure_loudness(
            &self.event_tx,
            &mut built_audio.audio_formats,
            &built_audio.audio_segments,
            &file_ids,
            tracks_to_files,
            candidate_key,
            &db_release.id,
            import_id,
        )
        .await;
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

        // Cover priority: Remote > local folder image > embedded. Finalize writes
        // the winner's bytes and row in one coven batch.
        let cover_candidate = match remote_cover_image.take() {
            Some(remote) => Some(remote),
            None => match self.pick_folder_cover(discovered_files, selected_cover_path)? {
                Some(local) => Some(local),
                None => embedded_cover.map(|(bytes, _content_type)| cover_image::CoverCandidate {
                    bytes,
                    source: "embedded".to_string(),
                    source_url: None,
                }),
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
            0,
            ImportPhase::Finalizing,
            import_id,
        );

        let remote_intent = matches!(storage_mode, StorageMode::Remote);
        library_manager
            .finalize_import_atomic(
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
                &db_files,
                &built_audio.audio_formats,
                &built_audio.audio_segments,
                library_image,
                &artist_images,
                cover_rel_id,
                identities,
                &local_path,
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
