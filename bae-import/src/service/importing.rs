use super::artist_pictures::DiscogsArtistImage;
use super::cover_image::ImportCover;
use super::*;
use crate::import_cancel::{ImportRunEnd, ImportStop};
use crate::types::{ImportStep, TrackAudio};
use bae_util::task_failure::{blocking, caught, TaskFailed};

impl ImportService {
    pub(super) async fn do_import(&self, command: ImportCommand, expectation: ImportExpectation) {
        let import_id = command.import_id.clone();
        let candidate_key = command.candidate_key.clone();
        let content_hash = expectation.candidate.content_hash.clone();
        let edit_revision = expectation.candidate.file_edit_revision;
        // A panic in the work fails this import as an error does, so its
        // failure is recorded and the worker goes on to the next import.
        let work = self.prepare_and_run_folder_import(
            import_id.clone(),
            candidate_key.clone(),
            command.source,
            expectation,
            command.destination,
        );
        let run = self
            .import_cancels
            .run(&candidate_key, &import_id, async {
                caught("an import", work).await.unwrap_or_else(|bug| {
                    Err(TaskFailed::Panicked(self.diagnostics.panicked(bug)).into())
                })
            })
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
                if let Err(error) = self.runtime.move_import(&candidate_key, &import_id, None).await
                {
                    self.diagnostics.report_failure(
                        &format!("end the cancelled import of {candidate_key}"),
                        &error,
                    );
                }
                self.event_tx.send(crate::ImportEvent::ImportProgress {
                    candidate_key,
                    progress: ImportProgress::Cancelled { import_id },
                });
                return;
            }
        };
        if let Err(e) = result {
            error!("Import failed: {}", e);
            self.diagnostics.event(TelemetryEvent::ImportFailed {});
            // The failure is recorded as what it is; the progress event carries
            // the error's diagnostic text, which the variant Displays build
            // from their `#[from]` source messages.
            let failed_at = bae_store::utc_now(self.clock.as_ref());
            let failure = crate::ImportFailure::of(&e, failed_at);
            let error = e.to_string();

            // The row goes first and the event second: the event is what the
            // pane showing this import redraws from, and the row is what a
            // relaunched pane reads instead. The same write ends the import's
            // claim: the two commit or roll back together, so no read sees
            // the import ended without its failure. No caller waits on the
            // worker, so a write that will not land is a bug, reported with
            // the import's own failure, the one the person was waiting to see.
            if let Err(write) = self
                .tables
                .fail_candidate_import(
                    &content_hash,
                    edit_revision,
                    &failure,
                    &candidate_key,
                    &import_id,
                )
                .await
            {
                self.diagnostics.report_failure_as(
                    &format!("record the failed import of {candidate_key}"),
                    &write,
                    &format!("the import failed: {error}; recording that failed: {write}"),
                );
            }

            self.event_tx.send(crate::ImportEvent::ImportProgress {
                candidate_key,
                progress: ImportProgress::Failed { error, import_id },
            });
        }
    }

    /// Prepare and run a folder import from its stored candidate and metadata
    /// seed after validating that every physical file still has the scanned
    /// identity, then run the shared mapping and write path.
    pub(super) async fn prepare_and_run_folder_import(
        &self,
        import_id: String,
        candidate_key: crate::CandidateKey,
        source: crate::release_candidate::CandidateSource,
        expectation: ImportExpectation,
        destination: ImportDestination,
    ) -> Result<(), ImportStop> {
        let expected_content_hash = expectation.candidate.content_hash.clone();
        let expected_edit_revision = expectation.candidate.file_edit_revision;

        let import_start = std::time::Instant::now();
        self.event_tx
            .send_once_written(
                crate::ImportEvent::ImportProgress {
                    candidate_key: candidate_key.clone(),
                    progress: ImportProgress::Preparing {
                        import_id: import_id.clone(),
                        step: PrepareStep::ValidatingSourceFiles,
                    },
                },
                self.runtime.move_import(
                    &candidate_key,
                    &import_id,
                    Some(ImportStep::Preparing(PrepareStep::ValidatingSourceFiles)),
                ),
            )
            .await
            .map_err(crate::ImportError::from)?;

        let stored_candidate = match self
            .tables
            .load_release_candidate(&candidate_key)
            .await?
            .map_err(|reason| crate::ImportError::GroupingBlocked { reason })?
        {
            Some(candidate) => candidate,
            _ => {
                return Err(crate::ImportError::Internal {
                    detail: format!("{candidate_key} is no longer a valid stored import candidate"),
                }
                .into())
            }
        };
        if stored_candidate.source() != source
            || stored_candidate.files.content_hash() != expected_content_hash
            || stored_candidate.file_edit_revision != expected_edit_revision
        {
            return Err(crate::ImportError::Internal {
                detail: format!(
                    "{candidate_key} changed after it was selected; refresh and identify it again"
                ),
            }
            .into());
        }
        let identity_files = stored_candidate.files.release_files().cloned().collect::<Vec<_>>();
        let audio_observations = blocking("validate the import's file identities", move || {
            super::file_identity::validate_scanned_file_identities(&identity_files)
        })
        .await
        .map_err(|failed| self.diagnostics.bug(failed))??;
        let source_name = stored_candidate.name.to_string();
        let categorized = stored_candidate.files;

        let preparation =
            crate::preparations::stored_preparation(&self.tables, &expected_content_hash)
                .await?
                .ok_or_else(|| crate::ImportError::Internal {
                    detail: format!("{candidate_key} has no stored import preparation"),
                })?;
        if !preparation.file_edits.is_at_revision(expected_edit_revision)
            || preparation.metadata_revision != expectation.candidate.metadata_revision
        {
            return Err(crate::ImportError::Internal {
                detail: format!(
                    "{candidate_key}'s preparation changed after it was queued; import it again"
                ),
            }
            .into());
        }
        let release_link = preparation.link;

        let file_tag_snapshot = expectation.file_tag_snapshot.as_ref();
        if let Some(snapshot) = file_tag_snapshot {
            if !snapshot.files.iter().map(|fact| &fact.observation).eq(audio_observations.iter()) {
                return Err(crate::ImportError::FileTags {
                    detail: format!(
                        "{candidate_key}'s audio changed after its file tags were read"
                    ),
                }
                .into());
            }
        }
        let cover =
            ImportCover::selected(preparation.metadata.cover, file_tag_snapshot, &candidate_key)?;

        let records = match &release_link {
            Some(link) => linked_records(&self.tables, link).await?,
            None => Vec::new(),
        };
        // The draft holds every value the release is written with; nothing
        // is read from its source again.
        let assembled = crate::assemble::assemble_release(
            &preparation.metadata.draft,
            self.clock.as_ref(),
            self.ids.as_ref(),
        )?;
        let track_bindings = assembled.bindings;

        let picked_album = self.tables.candidate_album_pick(&categorized.content_hash()).await?;
        let mut prepared = self
            .reconcile_prepared_release(assembled.parsed, records, picked_album, assembled.picked)
            .await?;

        prepared.db_release.source_folder_name = Some(source_name);

        // The pictures of the Discogs artists this import creates are fetched
        // now, before any file is read or row written, the one thing an
        // import asks a provider for. A fetch that fails fails the import;
        // importing again fetches again.
        let new_artists =
            self.new_discogs_artists(&prepared.artist_credits, &prepared.picked_artists).await?;
        if !new_artists.is_empty() {
            self.event_tx
                .send_once_written(
                    crate::ImportEvent::ImportProgress {
                        candidate_key: candidate_key.clone(),
                        progress: ImportProgress::Preparing {
                            import_id: import_id.clone(),
                            step: PrepareStep::FetchingArtistPictures,
                        },
                    },
                    self.runtime.move_import(
                        &candidate_key,
                        &import_id,
                        Some(ImportStep::Preparing(PrepareStep::FetchingArtistPictures)),
                    ),
                )
                .await
                .map_err(crate::ImportError::from)?;
        }
        let artist_images = self.fetch_discogs_artist_images(new_artists).await?;

        // Each track row becomes a TrackFile's DbTrack, bound to the audio
        // its slot named and measured by it. Past here the tracks live in
        // `tracks_to_files`.
        let tracks_to_files = resolve_track_files(
            std::mem::take(&mut prepared.track_rows).into_iter().zip(track_bindings).collect(),
            &categorized,
        )?;

        debug!(
            "Prepared album '{}' (release: {}) with {} tracks",
            prepared.db_album.title,
            prepared.db_release.id,
            tracks_to_files.len()
        );

        self.run_import(
            CommittedCandidate { source, expectation: expectation.clone() },
            ImportRun { import_id: &import_id, candidate_key: &candidate_key },
            destination,
            &mut prepared,
            ImportFiles { discovered: &categorized.files, tracks: &tracks_to_files },
            cover,
            &artist_images,
        )
        .await?;

        let total_duration = import_start.elapsed();
        // The release is written (`run_import` succeeded via `?` above). Report
        // the real track count and the monotonic elapsed — never a zero default.
        self.diagnostics.event(TelemetryEvent::ImportCompleted {
            track_count: tracks_to_files.len() as u32,
            duration_ms: total_duration,
        });
        info!("Imported '{}' in {:.0?}", prepared.album_title, total_duration,);

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
        cover: Option<ImportCover>,
        artist_images: &[DiscogsArtistImage],
    ) -> Result<(), ImportStop> {
        let ImportFiles { discovered: discovered_files, tracks: tracks_to_files } = files;
        let ImportRun { import_id, candidate_key } = run;
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
        let files_now = bae_store::utc_now(self.clock.as_ref());
        let mut prepared_files = Vec::with_capacity(total_files);
        let mut file_ids: HashMap<PathBuf, String> = HashMap::new();
        let mut source_audio_layouts = HashMap::new();
        for track_file in tracks_to_files {
            let (file_paths, layout): (Vec<&Path>, _) = match &track_file.audio {
                TrackAudio::Standalone { file_path, .. } => {
                    (vec![file_path.as_path()], bae_records::source_audio::SourceAudioLayout::File)
                }
                TrackAudio::CueBacked { cue_pair, .. } => (
                    cue_pair.audio_files.iter().map(|audio| audio.path.as_path()).collect(),
                    bae_records::source_audio::SourceAudioLayout::Cue,
                ),
            };
            for file_path in file_paths {
                if let Some(existing) = source_audio_layouts.insert(file_path, layout) {
                    if existing != layout {
                        return Err(crate::ImportError::Internal {
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
        let total_bytes: u128 =
            discovered_files.iter().map(|entry| u128::from(entry.file.version.size)).sum();
        if total_bytes == 0 {
            return Err(crate::ImportError::Internal {
                detail: "stored import candidate contains no bytes".to_string(),
            }
            .into());
        }
        self.event_tx
            .send_once_written(
                super::progress::phase_progress(
                    run,
                    &db_release.id,
                    Some(0),
                    ImportPhase::ReadingFiles,
                ),
                self.runtime.move_import(
                    candidate_key,
                    import_id,
                    Some(ImportStep::Running(ImportPhase::ReadingFiles)),
                ),
            )
            .await
            .map_err(crate::ImportError::from)?;
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
            // coven reads the original whole and records it as it finds it
            // now, on this device only or for the cloud alike, so what it
            // finds must be the file the scan read.
            let original =
                Box::pin(coven::prepare_user_file(&file.version.path, move |consumed| {
                    let completed = bytes_read_before_file + u128::from(consumed);
                    let percent = ((completed * 100) / total_bytes).min(100) as u8;
                    let previous = reported_percent_for_progress
                        .swap(percent, std::sync::atomic::Ordering::Relaxed);
                    if previous != percent {
                        super::progress::emit_phase_progress_on(
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
                .map_err(|error| crate::ImportError::UnusableFile {
                    detail: format!(
                        "coven could not prepare {} for import: {error}",
                        file.version.path.display()
                    ),
                })?;
            super::file_identity::require_scanned_identity(
                file,
                original.size(),
                original.modified_at(),
            )?;
            let place = match destination {
                ImportDestination::Local => coven::Place::DeviceOnly,
                ImportDestination::Remote { pin: true } => coven::Place::CloudKeptHere,
                ImportDestination::Remote { pin: false } => coven::Place::Cloud,
            };
            bytes_read += u128::from(file.version.size);
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
            let mut db_file = NewReleaseFile::new(
                &db_release.id,
                &file.relative_path,
                resolve_file_content_type(entry)?,
                self.ids.new_id().to_string(),
                files_now,
            );
            db_file.source_audio = file.source_audio.as_ref().map(|audio| {
                bae_records::source_audio::SourceAudioFile {
                    layout: source_audio_layouts.get(file.version.path.as_path()).copied(),
                    format: audio.format.clone(),
                    content_type: audio.content_type.clone(),
                    duration_ms: audio.duration_ms,
                }
            });
            file_ids.insert(file.version.path.clone(), db_file.id.clone());
            prepared_files.push(PreparedImportFile { row: db_file, original, place });
            debug!("Read file {}/{}: {}", idx + 1, total_files, file.relative_path,);
        }

        // Byte landings seek through each CUE image with FFmpeg: blocking file
        // reads, so they run off the worker's runtime.
        let mut built_audio = {
            let tracks = tracks_to_files.to_vec();
            let file_ids = file_ids.clone();
            let clock = self.clock.clone();
            let ids = self.ids.clone();
            let diagnostics = self.diagnostics.clone();
            blocking("build the import's audio formats", move || {
                super::build_audio_formats(
                    &tracks,
                    &file_ids,
                    clock.as_ref(),
                    ids.as_ref(),
                    &diagnostics,
                )
            })
            .await
            .map_err(|failed| self.diagnostics.bug(failed))??
        };
        let source_file_sizes: HashMap<PathBuf, u64> = discovered_files
            .iter()
            .map(|entry| (entry.file.version.path.clone(), entry.file.version.size))
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
        self.event_tx
            .send_once_written(
                super::progress::phase_progress(
                    run,
                    &db_release.id,
                    None,
                    ImportPhase::MeasuringLoudness,
                ),
                self.runtime.move_import(
                    candidate_key,
                    import_id,
                    Some(ImportStep::Running(ImportPhase::MeasuringLoudness)),
                ),
            )
            .await
            .map_err(crate::ImportError::from)?;
        let loudness = crate::loudness::measure_loudness(
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
            &self.diagnostics,
        )
        .await?;
        db_release.album_loudness_lufs = loudness.album_loudness_lufs;
        db_release.album_peak_linear = loudness.album_peak_linear;

        // Finalize writes the cover's bytes and row in one coven batch.
        let cover_winner = match cover {
            Some(cover) => {
                Some(cover.stored(discovered_files, &db_release.id, &self.diagnostics).await?)
            }
            None => None,
        };
        let library_image = cover_winner.as_ref().map(|(image, bytes)| (image, bytes.as_slice()));

        // Past here the release is written in one transaction, which a cancel
        // no longer interrupts; one that came first stops the import here.
        self.import_cancels.begin_writing(candidate_key, import_id)?;
        self.event_tx
            .send_once_written(
                super::progress::phase_progress(run, &db_release.id, None, ImportPhase::Finalizing),
                self.runtime.move_import(
                    candidate_key,
                    import_id,
                    Some(ImportStep::Running(ImportPhase::Finalizing)),
                ),
            )
            .await
            .map_err(crate::ImportError::from)?;

        let commit_guard = crate::tables::ImportCommitGuard::Candidate {
            candidate_key: candidate_key.clone(),
            source: committed.source,
            expectation: committed.expectation,
            import_id: import_id.to_string(),
        };
        // Staging the originals queues a cloud import's uploads in the import
        // transaction.
        self.finalize_import_atomic(
            commit_guard,
            new_album,
            db_release,
            tracks_to_files,
            crate::tables::ImportRows {
                mediums,
                track_artists,
                album_artists,
                works: &work_graph.works,
                work_artists: &work_graph.work_artists,
                work_parts: &work_graph.work_parts,
                track_works: &work_graph.track_works,
                release_artist_roles,
                track_artist_roles,
                artists: bae_records::ArtistCredits {
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
        self.event_tx.send(crate::ImportEvent::ImportProgress {
            candidate_key: candidate_key.clone(),
            progress,
        });

        info!("Import complete for release {}", db_release.id);
        Ok(())
    }
}

impl ImportService {
    /// Insert all of an import's data in one transaction, so the release
    /// exists complete or not at all. A Remote import stages its files for
    /// the cloud in the same write, kept here once stored when it asks for
    /// that.
    ///
    /// The pictures of the artists the import creates are staged from
    /// `artist_images` ([`Self::staged_artist_pictures`]); the write fails
    /// if its own artist resolution creates different artists, so no picture
    /// lands without its artist.
    async fn finalize_import_atomic(
        &self,
        guard: crate::tables::ImportCommitGuard,
        album: Option<&DbAlbum>,
        release: &DbRelease,
        tracks_to_files: &[TrackFile],
        rows: crate::tables::ImportRows<'_>,
        files: Vec<PreparedImportFile>,
        library_image: Option<(&bae_records::DbLibraryImage, &[u8])>,
        artist_images: &[DiscogsArtistImage],
    ) -> Result<(), crate::ImportError> {
        let pictures = self.staged_artist_pictures(&rows.artists, artist_images).await?;
        let images: Vec<_> =
            pictures.images.iter().map(|(image, bytes)| (image, bytes.as_slice())).collect();
        self.tables
            .finalize_import_atomic(
                &*self.ids,
                guard,
                album,
                release,
                tracks_to_files,
                rows,
                files,
                library_image,
                crate::tables::NewArtistImages {
                    expected_new_artists: &pictures.expected_new_artists,
                    images: &images,
                },
            )
            .await?;
        Ok(())
    }
}

/// The records an import of a candidate linked to `link` stores.
///
/// A linked album is stored as each catalog's album record. A linked
/// pressing is stored as every catalog its stored releases describe it in,
/// the linked primary's own record first.
async fn linked_records(
    tables: &crate::tables::ImportTables,
    link: &crate::ReleaseLink,
) -> Result<Vec<bae_catalogs::records::ReleaseRecord>, crate::ImportError> {
    let link = match link {
        crate::ReleaseLink::Album(album) => return Ok(album.records()),
        crate::ReleaseLink::Pressing(pressing) => pressing,
    };
    let mut claimed = Vec::new();
    for release in link.claimed() {
        claimed.push(tables.load_source_release(release).await?.ok_or_else(|| {
            crate::ImportError::Internal {
                detail: format!(
                    "the candidate is linked to {} release {} that nothing stored",
                    release.catalog.as_str(),
                    release.key
                ),
            }
        })?);
    }
    let (primary, partners) = claimed.split_first().expect("a link claims at least its primary");
    Ok(records_for_commit(primary, partners))
}

#[cfg(test)]
#[path = "importing_tests.rs"]
mod tests;
