//! Export domain operations for [`LibraryManager`].

use super::*;

#[cfg(not(any(target_os = "ios", target_os = "android")))]
const EXPORT_MARKER_FILE: &str = ".bae-export";

impl LibraryManager {
    // ── Export queue ─────────────────────────────────────────────────
    //
    // Exporting a release copies its whole file set out verbatim to a
    // user-chosen directory, routed through an in-memory serial queue (mirroring
    // the download/pin queue): one release copies out at a time, the rest wait,
    // and the user can pause/cancel/retry. The queue is transient — on restart
    // it's empty. Export changes no release state: it only reads (through coven's
    // locality-aware read) and writes to a user directory, so a Remote release
    // stays Remote and no gate flips, external ref changes, or cloud tombstones.

    /// Enqueue a release to export its files verbatim to `target_dir`. Skips ids
    /// already in the queue (any state); otherwise resolves its title /
    /// file_count / total_size from its storage summary so the Exporting pane can
    /// render the row without a re-query. Wakes the parked worker and emits a
    /// fresh `ExportQueueChanged`.
    pub async fn enqueue_export(
        &self,
        release_id: &str,
        target_dir: std::path::PathBuf,
        selection: crate::config::ExportSelection,
    ) -> Result<(), LibraryError> {
        // The target dir round-trips back to the Exporting pane as a string (the
        // snapshot renders `target_dir` for each row), so it must be valid UTF-8.
        // Fail loud here rather than let a non-UTF-8 path be lossily rewritten into
        // a different directory the export would then write to.
        target_dir.to_str().ok_or_else(|| {
            LibraryError::Import(format!(
                "export target directory is not valid UTF-8: {}",
                target_dir.display()
            ))
        })?;

        if self.export_queue.contains(release_id) {
            debug!("enqueue_export: {release_id} already queued, skipping");
            return Ok(());
        }
        let summary = self
            .find_release_storage_summary(release_id)
            .await?
            .ok_or_else(|| {
                LibraryError::Import(format!("cannot export release {release_id}: not found"))
            })?;

        let op = crate::library::ExportOp {
            release_id: release_id.to_string(),
            title: summary.album_title,
            file_count: summary.file_count,
            total_size: summary.total_size,
            created_at: self.clock.now().timestamp_millis(),
            payload: crate::library::export_snapshot::ExportRequest {
                target_dir,
                selection,
            },
            state: crate::library::ExportState::Queued,
        };
        if self.export_queue.enqueue(op) {
            self.export_queue.wake();
            self.emit_export_queue_changed();
        }
        Ok(())
    }

    /// Pause or resume the export queue. While paused the worker parks instead of
    /// starting the next release; the in-flight one runs to completion. Resuming
    /// wakes the worker. Emits a fresh `ExportQueueChanged`.
    pub fn set_exports_paused(&self, paused: bool) {
        let was_paused = self.export_queue.set_paused(paused);
        if was_paused && !paused {
            self.export_queue.wake();
        }
        self.emit_export_queue_changed();
    }

    /// Cancel a release's export. Drops a queued/failed entry; for the active one,
    /// aborts its in-flight copy task. The copy writes every file into a hidden
    /// staging directory and renames it into place only after all files succeed, so
    /// the abort leaves no output at the final path — the staging directory is
    /// removed when the aborted task's future drops. Emits a fresh snapshot.
    pub fn cancel_export(&self, release_id: &str) {
        self.export_queue.cancel(release_id);
        self.emit_export_queue_changed();
    }

    /// Flip every failed export back to queued and wake the worker to retry them.
    /// Emits a fresh `ExportQueueChanged`.
    pub fn retry_exports(&self) {
        if self.export_queue.retry_failed() {
            self.export_queue.wake();
        }
        self.emit_export_queue_changed();
    }

    /// The serial export worker loop. Parks on the queue's `Notify` whenever the
    /// queue is paused or holds nothing queued; otherwise takes the next queued
    /// release and copies it out. Processes strictly one release at a time.
    #[cfg(not(any(target_os = "ios", target_os = "android")))]
    pub(super) async fn run_export_worker(&self) {
        loop {
            let Some(op) = self.export_queue.next_queued() else {
                self.export_queue.wait().await;
                continue;
            };
            self.run_queued_export(op).await;
        }
    }

    /// Run one queued release's export: spawn the copy task, flip the entry to
    /// `Active` and register its abort handle atomically, then await the task. The
    /// task updates the queue's per-release percent and re-emits the snapshot as
    /// it copies each file. On success drop the entry; on failure mark it `Failed`
    /// (it stays in the queue for retry).
    ///
    /// `cancel_export` aborts the in-flight task via the registered handle and
    /// removes the queue entry; on its way out we check whether the entry is still
    /// present before recording a failure — a cancelled export isn't a failure.
    #[cfg(not(any(target_os = "ios", target_os = "android")))]
    async fn run_queued_export(&self, op: crate::library::ExportOp) {
        let release_id = op.release_id.clone();
        let request = op.payload.clone();

        let worker = self.clone();
        let task_release_id = release_id.clone();
        let task = self.runtime_handle.spawn(async move {
            worker
                .export_release_to_dir(&task_release_id, request)
                .await
        });
        let abort = task.abort_handle();

        // Flip to Active and register the abort handle atomically. If a cancel
        // removed the entry in the gap since we picked it, abort the task we just
        // spawned and bail — nothing was written.
        if !self.export_queue.activate(&release_id, abort.clone(), 0) {
            abort.abort();
            debug!("Export for {release_id} cancelled before it started; aborting");
            return;
        }
        self.emit_export_queue_changed();

        let outcome = task.await;
        self.export_queue.clear_active_abort();

        // A cancel removed the entry while the copy was in flight. This isn't a
        // failure — don't re-add the entry or mark it Failed.
        if !self.export_queue.contains(&release_id) {
            debug!("Export for {release_id} ended after cancel; leaving queue as-is");
            return;
        }

        match outcome {
            Ok(Ok(())) => {
                self.export_queue.remove(&release_id);
                self.emit_export_queue_changed();
            }
            Ok(Err(error)) => {
                error!("Export failed for release {release_id}: {error}");
                self.export_queue
                    .mark_failed(&release_id, error.to_string());
                self.emit_export_queue_changed();
            }
            Err(join_error) if join_error.is_cancelled() => {
                // Aborted by a cancel that also removed the entry — handled above.
                debug!("Export task for {release_id} aborted");
            }
            Err(join_error) => {
                error!("Export task for {release_id} panicked: {join_error}");
                self.export_queue
                    .mark_failed(&release_id, join_error.to_string());
                self.emit_export_queue_changed();
            }
        }
    }

    /// Copy a release's files verbatim to `<target_dir>/<source_folder_name>/`,
    /// reading each blob through coven's locality-aware read (a Remote release
    /// fetches from cloud/cache and decrypts). The export is all-or-nothing: every
    /// file is written into a hidden staging directory alongside the final one, and
    /// only after all files succeed is the staging directory renamed into place. Any
    /// read/write error, cancel, or panic drops the [`StagingDir`] guard, which
    /// removes the staging directory — so a failed or cancelled export never leaves
    /// partial output at the final path. Updates the queue's per-release percent (by
    /// file index) and re-emits the snapshot after each file. Fails loudly when the
    /// release has no source folder name.
    #[cfg(not(any(target_os = "ios", target_os = "android")))]
    async fn export_release_to_dir(
        &self,
        release_id: &str,
        request: crate::library::export_snapshot::ExportRequest,
    ) -> Result<(), LibraryError> {
        let release = self
            .get_release_by_id(release_id)
            .await?
            .ok_or_else(|| LibraryError::Import(format!("release not found: {release_id}")))?;
        let folder = release.source_folder_name.ok_or_else(|| {
            LibraryError::Import(format!(
                "release {release_id} has no source folder name; cannot reconstruct its folder"
            ))
        })?;

        let final_dir = request.target_dir.join(&folder);
        // Stage under the target dir (not a temp dir elsewhere) so the final rename
        // stays on one filesystem and is atomic. The name is hidden and carries the
        // release id so a concurrent export of a different release can't collide.
        let staging = StagingDir::create(
            request
                .target_dir
                .join(format!(".{folder}.export-{release_id}")),
        )?;

        let files = self.database.get_files_for_release(release_id).await?;
        let selection_kind = match &request.selection {
            crate::config::ExportSelection::Original => "original",
            crate::config::ExportSelection::Preset { .. } => "preset",
        };
        info!(
            release_id,
            folder = folder.as_str(),
            file_count = files.len(),
            selection = selection_kind,
            "Exporting release"
        );

        match request.selection {
            crate::config::ExportSelection::Original => {
                let total = files.len();
                for (index, file) in files.iter().enumerate() {
                    self.export_one_file(file, staging.path()).await?;
                    let percent = (((index + 1) * 100) / total.max(1)) as u8;
                    self.set_export_progress(release_id, percent);
                }
            }
            crate::config::ExportSelection::Preset { preset_id } => {
                self.export_release_tracks_to_dir(release_id, &preset_id, staging.path())
                    .await?;
            }
        }

        write_export_marker(staging.path(), release_id)?;
        replace_export_dir(staging.path(), &final_dir, &folder, release_id)?;
        staging.disarm();
        Ok(())
    }

    /// Copy one release file's verbatim bytes to `<staging_dir>/<original_filename>`.
    /// `original_filename` may name a subfolder (e.g. `CD1/CDImage.ape`), so its
    /// parent is created first. No per-file temp is needed: the whole staging
    /// directory is the atomic unit, renamed into place only once every file is
    /// written.
    #[cfg(not(any(target_os = "ios", target_os = "android")))]
    async fn export_one_file(
        &self,
        file: &DbFile,
        staging_dir: &std::path::Path,
    ) -> Result<(), LibraryError> {
        let bytes = self.read_release_blob(file).await?;
        let file_path = staging_dir.join(&file.original_filename);
        if let Some(parent) = file_path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(&file_path, &bytes)?;
        Ok(())
    }

    #[cfg(not(any(target_os = "ios", target_os = "android")))]
    pub async fn export_release(
        &self,
        release_id: &str,
        target_dir: &Path,
        selection: crate::config::ExportSelection,
    ) -> Result<(), LibraryError> {
        self.export_release_to_dir(
            release_id,
            crate::library::export_snapshot::ExportRequest {
                target_dir: target_dir.to_path_buf(),
                selection,
            },
        )
        .await
    }

    /// Resolve one track's tag data from the database alone — the tag fields, its
    /// track number, the release's track total, and whether the media is digital.
    /// Reads no audio and no cover, so both the filename-suggestion path (which
    /// must not download a whole file) and the full export plan share it.
    #[cfg(not(any(target_os = "ios", target_os = "android")))]
    async fn resolve_export_tags(
        &self,
        meta: &TrackAudioMeta,
    ) -> Result<ResolvedExportTags, LibraryError> {
        let album = self.database.get_album_for_release(&meta.release).await?;

        let album_artists = self.database.get_artists_for_album(&album.id).await?;
        let artist = join_artist_names(&album_artists);

        let release_tracks = self
            .database
            .get_tracks_for_release(&meta.release.id)
            .await?;
        let total_tracks = release_tracks.len();
        let has_multiple_sides = release_tracks
            .iter()
            .map(|t| t.side)
            .collect::<std::collections::HashSet<_>>()
            .len()
            > 1;
        let disc = if has_multiple_sides {
            Some(meta.track.side)
        } else {
            None
        };

        let year = meta.release.pressing.year.or(album.year);
        let is_digital =
            crate::util::format::is_digital_format(meta.release.pressing.format.as_deref());

        let tags = ExportTags {
            title: meta.track.title.clone(),
            artist,
            album: album.title,
            year,
            disc,
        };

        Ok(ResolvedExportTags {
            tags,
            track_number: meta.track.track_number,
            total_tracks,
            is_digital,
            primary_release_id: album.primary_release_id,
        })
    }

    /// Assemble everything `ExportService::export_track` needs for a
    /// track in one pass: source audio bytes, tag fields, cover image bytes,
    /// neighbour counts, and the raw audio-format aggregate for decoding.
    /// Cloud-only tracks download + decrypt here — export never requires a
    /// local copy.
    #[cfg(not(any(target_os = "ios", target_os = "android")))]
    pub async fn get_export_track_plan(
        &self,
        track_id: &str,
    ) -> Result<ExportTrackPlan, LibraryError> {
        let meta = TrackAudioMeta::resolve(&self.database, track_id).await?;
        let resolved = self.resolve_export_tags(&meta).await?;

        let mut audio_bytes = Vec::new();
        for audio_file in &meta.audio_files {
            let bytes = crate::storage::local::transfer::read_release_file_bytes(audio_file, self)
                .await
                .map_err(|e| {
                    LibraryError::TrackMapping(format!(
                        "Couldn't read audio file {} for track {track_id}: {e}",
                        audio_file.id
                    ))
                })?;
            audio_bytes.push(super::ExportAudioBytes {
                file_id: audio_file.id.clone(),
                bytes,
            });
        }

        let cover_image_bytes = match resolved.primary_release_id.as_deref() {
            Some(rid) => match self.cover_ref(rid).await? {
                Some(image) => self.read_image_blob(&image).await?,
                None => None,
            },
            None => None,
        };

        let ResolvedExportTags {
            tags,
            track_number,
            total_tracks,
            is_digital,
            primary_release_id: _,
        } = resolved;

        Ok(ExportTrackPlan {
            audio_bytes,
            tags,
            cover_image_bytes,
            track_number,
            total_tracks,
            is_digital,
            audio_window: export_window_from_meta(&meta),
            audio_meta: meta,
        })
    }

    /// The default filename (stem, no extension) a single-track "Save As…" export
    /// suggests for `track_id`, rendered from the configured template and the
    /// track's tag data. Reads no audio and no cover — only the database — so
    /// seeding a save panel never touches a whole file or the cloud.
    #[cfg(not(any(target_os = "ios", target_os = "android")))]
    pub async fn export_track_suggested_name(
        &self,
        track_id: &str,
    ) -> Result<String, LibraryError> {
        let meta = TrackAudioMeta::resolve(&self.database, track_id).await?;
        let resolved = self.resolve_export_tags(&meta).await?;
        Ok(crate::library::export::render_export_filename(
            &self.export_filename_template(),
            &resolved,
        ))
    }

    #[cfg(not(any(target_os = "ios", target_os = "android")))]
    pub async fn export_track_extension(
        &self,
        track_id: &str,
        selection: crate::config::ExportSelection,
    ) -> Result<String, LibraryError> {
        match selection {
            crate::config::ExportSelection::Original => {
                let meta = TrackAudioMeta::resolve(&self.database, track_id).await?;
                Ok(meta.audio_format.content_type.file_extension().to_string())
            }
            crate::config::ExportSelection::Preset { preset_id } => {
                let preset = self
                    .export_presets()
                    .into_iter()
                    .find(|preset| preset.id == preset_id && preset.applies_to_track)
                    .ok_or_else(|| {
                        LibraryError::Import(format!(
                            "export preset {preset_id} is not available for track export"
                        ))
                    })?;
                Ok(preset.codec.extension().to_string())
            }
        }
    }

    #[cfg(not(any(target_os = "ios", target_os = "android")))]
    pub async fn export_track(
        &self,
        track_id: &str,
        output_path: &Path,
        selection: crate::config::ExportSelection,
    ) -> Result<(), LibraryError> {
        match selection {
            crate::config::ExportSelection::Original => {
                let mut plan = self.get_export_track_plan(track_id).await?;
                plan.audio_window = export_window_for_track_file(
                    &plan.audio_meta,
                    None,
                    crate::config::ExportPregapPlacement::Exclude,
                    false,
                );
                ExportService::export_original_track(plan, output_path)
                    .await
                    .map_err(LibraryError::Import)
            }
            crate::config::ExportSelection::Preset { preset_id } => {
                let preset = self
                    .export_presets()
                    .into_iter()
                    .find(|preset| preset.id == preset_id && preset.applies_to_track)
                    .ok_or_else(|| {
                        LibraryError::Import(format!(
                            "export preset {preset_id} is not available for track export"
                        ))
                    })?;
                let mut plan = self.get_export_track_plan(track_id).await?;
                let release_tracks = self
                    .database
                    .get_tracks_for_release(&plan.audio_meta.release.id)
                    .await?;
                let track_index = release_tracks
                    .iter()
                    .position(|track| track.id == plan.audio_meta.track.id)
                    .ok_or_else(|| {
                        LibraryError::Import(format!(
                            "track {} is not ordered in release {}",
                            plan.audio_meta.track.id, plan.audio_meta.release.id
                        ))
                    })?;
                let next_meta = if track_index + 1 < release_tracks.len() {
                    Some(
                        TrackAudioMeta::resolve(
                            &self.database,
                            &release_tracks[track_index + 1].id,
                        )
                        .await?,
                    )
                } else {
                    None
                };
                plan.audio_window = export_window_for_track_file(
                    &plan.audio_meta,
                    next_meta.as_ref(),
                    preset.pregap_placement,
                    track_index == 0,
                );
                ExportService::export_track(plan, output_path, preset)
                    .await
                    .map_err(LibraryError::Import)
            }
        }
    }

    #[cfg(not(any(target_os = "ios", target_os = "android")))]
    async fn export_release_tracks_to_dir(
        &self,
        release_id: &str,
        preset_id: &str,
        staging_dir: &std::path::Path,
    ) -> Result<(), LibraryError> {
        let preset = self
            .export_presets()
            .into_iter()
            .find(|preset| preset.id == preset_id && preset.applies_to_release)
            .ok_or_else(|| {
                LibraryError::Import(format!(
                    "export preset {preset_id} is not available for release export"
                ))
            })?;
        let tracks = self.database.get_tracks_for_release(release_id).await?;
        let total = tracks.len();
        if preset.pregap_placement == crate::config::ExportPregapPlacement::SingleFileWithCue {
            self.export_release_image_with_cue_to_dir(release_id, preset, &tracks, staging_dir)
                .await?;
            self.set_export_progress(release_id, 100);
            return Ok(());
        }
        let mut used_paths = std::collections::HashSet::new();

        for (index, track) in tracks.iter().enumerate() {
            let mut plan = self.get_export_track_plan(&track.id).await?;
            let next_meta = if index + 1 < tracks.len() {
                Some(TrackAudioMeta::resolve(&self.database, &tracks[index + 1].id).await?)
            } else {
                None
            };
            plan.audio_window = export_window_for_track_file(
                &plan.audio_meta,
                next_meta.as_ref(),
                preset.pregap_placement,
                index == 0,
            );

            let stem = crate::library::export::render_export_filename(
                &preset.filename_template,
                &resolved_tags_from_plan(&plan),
            );
            let output_path = unique_export_path(
                staging_dir,
                &stem,
                preset.codec.extension(),
                &mut used_paths,
            );
            ExportService::export_track(plan, &output_path, preset.clone())
                .await
                .map_err(LibraryError::Import)?;
            let percent = (((index + 1) * 100) / total.max(1)) as u8;
            self.set_export_progress(release_id, percent);
        }

        Ok(())
    }

    #[cfg(not(any(target_os = "ios", target_os = "android")))]
    fn set_export_progress(&self, release_id: &str, percent: u8) {
        if self.export_queue.contains(release_id) {
            self.export_queue.set_active_percent(release_id, percent);
            self.emit_export_queue_changed();
        }
    }

    #[cfg(not(any(target_os = "ios", target_os = "android")))]
    async fn export_release_image_with_cue_to_dir(
        &self,
        release_id: &str,
        preset: crate::config::ExportPreset,
        tracks: &[DbTrack],
        staging_dir: &std::path::Path,
    ) -> Result<(), LibraryError> {
        let mut plans = Vec::with_capacity(tracks.len());
        for track in tracks {
            let mut plan = self.get_export_track_plan(&track.id).await?;
            plan.audio_window = export_window_from_meta(&plan.audio_meta);
            plan.audio_window.leading_silence_samples =
                non_negative_samples(plan.audio_meta.audio_format.generated_pregap_samples);
            plans.push(plan);
        }

        let release = self
            .get_release_by_id(release_id)
            .await?
            .ok_or_else(|| LibraryError::Import(format!("release not found: {release_id}")))?;
        let folder = release.source_folder_name.ok_or_else(|| {
            LibraryError::Import(format!(
                "release {release_id} has no source folder name; cannot name its CUE image"
            ))
        })?;
        let stem = crate::library::export::sanitize_filename_stem(&folder);
        if stem.is_empty() {
            return Err(LibraryError::Import(format!(
                "release {release_id} source folder name has no usable filename characters"
            )));
        }
        let output_audio_path = staging_dir.join(format!("{stem}.{}", preset.codec.extension()));
        let output_cue_path = staging_dir.join(format!("{stem}.cue"));

        ExportService::export_release_image_with_cue(
            plans,
            &output_audio_path,
            &output_cue_path,
            release.pressing.barcode,
            preset,
        )
        .await
        .map_err(LibraryError::Import)
    }
}

#[cfg(not(any(target_os = "ios", target_os = "android")))]
fn export_window_from_meta(meta: &TrackAudioMeta) -> super::ExportAudioWindow {
    super::ExportAudioWindow {
        segments: export_segment_windows(meta, true),
        leading_silence_samples: 0,
        trailing_silence_samples: 0,
    }
}

#[cfg(not(any(target_os = "ios", target_os = "android")))]
fn export_window_for_track_file(
    meta: &TrackAudioMeta,
    next_meta: Option<&TrackAudioMeta>,
    placement: crate::config::ExportPregapPlacement,
    is_first_track: bool,
) -> super::ExportAudioWindow {
    let own_audio_pregap = non_negative_samples(meta.audio_format.pregap_samples);
    let own_generated_pregap = non_negative_samples(meta.audio_format.generated_pregap_samples);
    let includes_htoa = is_first_track
        && placement == crate::config::ExportPregapPlacement::AppendToPreviousIncludingHtoa;
    let mut segments = export_segment_windows(meta, includes_htoa || own_audio_pregap == 0);
    let leading_silence_samples = if includes_htoa {
        own_generated_pregap
    } else {
        0
    };

    let mut trailing_silence_samples = 0;
    if matches!(
        placement,
        crate::config::ExportPregapPlacement::AppendToPreviousExceptHtoa
            | crate::config::ExportPregapPlacement::AppendToPreviousIncludingHtoa
    ) {
        if let Some(next) = next_meta {
            let next_audio_pregap = non_negative_samples(next.audio_format.pregap_samples);
            if next_audio_pregap > 0 {
                segments.extend(export_segment_windows_for_role(
                    next,
                    crate::db::DbAudioSegmentRole::AudioPregap,
                ));
            }
            trailing_silence_samples =
                non_negative_samples(next.audio_format.generated_pregap_samples);
        }
    }

    super::ExportAudioWindow {
        segments,
        leading_silence_samples,
        trailing_silence_samples,
    }
}

#[cfg(not(any(target_os = "ios", target_os = "android")))]
fn export_segment_windows(
    meta: &TrackAudioMeta,
    include_audio_pregap: bool,
) -> Vec<super::ExportAudioSegmentWindow> {
    meta.audio_segments
        .iter()
        .filter(|segment| {
            include_audio_pregap || segment.role == crate::db::DbAudioSegmentRole::Main
        })
        .map(export_segment_window)
        .collect()
}

#[cfg(not(any(target_os = "ios", target_os = "android")))]
fn export_segment_windows_for_role(
    meta: &TrackAudioMeta,
    role: crate::db::DbAudioSegmentRole,
) -> Vec<super::ExportAudioSegmentWindow> {
    meta.audio_segments
        .iter()
        .filter(|segment| segment.role == role)
        .map(export_segment_window)
        .collect()
}

#[cfg(not(any(target_os = "ios", target_os = "android")))]
fn export_segment_window(segment: &crate::db::DbAudioSegment) -> super::ExportAudioSegmentWindow {
    super::ExportAudioSegmentWindow {
        file_id: segment.file_id.clone(),
        source_start_sample: u64::try_from(segment.start_sample)
            .expect("audio segment start_sample is non-negative"),
        source_end_sample: segment
            .end_sample
            .map(|sample| u64::try_from(sample).expect("audio segment end_sample is non-negative")),
    }
}

#[cfg(not(any(target_os = "ios", target_os = "android")))]
fn non_negative_samples(samples: Option<i64>) -> u64 {
    samples.map_or(0, |sample| {
        u64::try_from(sample).expect("audio_format pregap samples are non-negative")
    })
}

#[cfg(not(any(target_os = "ios", target_os = "android")))]
fn resolved_tags_from_plan(plan: &ExportTrackPlan) -> ResolvedExportTags {
    ResolvedExportTags {
        tags: ExportTags {
            title: plan.tags.title.clone(),
            artist: plan.tags.artist.clone(),
            album: plan.tags.album.clone(),
            year: plan.tags.year,
            disc: plan.tags.disc,
        },
        track_number: plan.track_number,
        total_tracks: plan.total_tracks,
        is_digital: plan.is_digital,
        primary_release_id: None,
    }
}

#[cfg(not(any(target_os = "ios", target_os = "android")))]
fn unique_export_path(
    dir: &std::path::Path,
    stem: &str,
    extension: &str,
    used_paths: &mut std::collections::HashSet<std::path::PathBuf>,
) -> std::path::PathBuf {
    let mut index = 1usize;
    loop {
        let candidate_stem = if index == 1 {
            stem.to_string()
        } else {
            format!("{stem} ({index})")
        };
        let path = dir.join(format!("{candidate_stem}.{extension}"));
        if used_paths.insert(path.clone()) {
            return path;
        }
        index += 1;
    }
}

#[cfg(not(any(target_os = "ios", target_os = "android")))]
fn write_export_marker(
    staging_dir: &std::path::Path,
    release_id: &str,
) -> Result<(), LibraryError> {
    std::fs::write(staging_dir.join(EXPORT_MARKER_FILE), release_id)?;
    Ok(())
}

#[cfg(not(any(target_os = "ios", target_os = "android")))]
fn replace_export_dir(
    staging_dir: &std::path::Path,
    final_dir: &std::path::Path,
    folder: &str,
    release_id: &str,
) -> Result<(), LibraryError> {
    let had_existing = final_dir.exists();
    if had_existing && !final_dir.join(EXPORT_MARKER_FILE).exists() {
        return Err(LibraryError::Import(format!(
            "export target exists and is not a prior bae export: {}",
            final_dir.display()
        )));
    }

    let backup_dir = final_dir.with_file_name(format!(".{folder}.replace-{release_id}"));
    if backup_dir.exists() {
        return Err(LibraryError::Import(format!(
            "export replacement backup already exists: {}",
            backup_dir.display()
        )));
    }

    if had_existing {
        std::fs::rename(final_dir, &backup_dir)?;
    }

    match std::fs::rename(staging_dir, final_dir) {
        Ok(()) => {
            if had_existing {
                if let Err(cleanup_error) = std::fs::remove_dir_all(&backup_dir) {
                    if let Err(move_new_error) = std::fs::rename(final_dir, staging_dir) {
                        return Err(LibraryError::Import(format!(
                            "failed to remove prior export backup {} ({cleanup_error}); also failed to move new export back to staging ({move_new_error})",
                            backup_dir.display()
                        )));
                    }
                    if let Err(restore_error) = std::fs::rename(&backup_dir, final_dir) {
                        return Err(LibraryError::Import(format!(
                            "failed to remove prior export backup {} ({cleanup_error}); moved new export back to staging but failed to restore prior export ({restore_error})",
                            backup_dir.display()
                        )));
                    }
                    return Err(LibraryError::Import(format!(
                        "failed to remove prior export backup {}: {cleanup_error}",
                        backup_dir.display()
                    )));
                }
            }
            Ok(())
        }
        Err(rename_error) => {
            if had_existing {
                if let Err(restore_error) = std::fs::rename(&backup_dir, final_dir) {
                    return Err(LibraryError::Import(format!(
                        "failed to move export into place ({rename_error}); also failed to restore prior export from {} ({restore_error})",
                        backup_dir.display()
                    )));
                }
            }
            Err(rename_error.into())
        }
    }
}

/// A staging directory for one in-flight release export, removed on drop unless
/// [`disarm`](StagingDir::disarm)ed. The export writes every file here and, once
/// all succeed, renames it into place and disarms the guard. Any earlier exit —
/// a read/write error (`?`), a panic, or the worker task being aborted on cancel
/// (which drops this future) — drops the guard, which removes the directory. That
/// is what keeps a failed or cancelled export from leaving partial output behind.
#[cfg(not(any(target_os = "ios", target_os = "android")))]
struct StagingDir {
    path: std::path::PathBuf,
    armed: bool,
}

#[cfg(not(any(target_os = "ios", target_os = "android")))]
impl StagingDir {
    /// Create the staging directory fresh. A leftover directory at this path (from
    /// a prior crash that skipped the drop cleanup) is removed first so the export
    /// starts from an empty tree rather than mixing in stale files.
    fn create(path: std::path::PathBuf) -> Result<Self, LibraryError> {
        if path.exists() {
            std::fs::remove_dir_all(&path)?;
        }
        std::fs::create_dir_all(&path)?;
        Ok(Self { path, armed: true })
    }

    fn path(&self) -> &std::path::Path {
        &self.path
    }

    /// Stop removing the directory on drop — called after it's been renamed into
    /// place, so the exported files survive.
    fn disarm(mut self) {
        self.armed = false;
    }
}

#[cfg(not(any(target_os = "ios", target_os = "android")))]
impl Drop for StagingDir {
    fn drop(&mut self) {
        if !self.armed {
            return;
        }
        if let Err(e) = std::fs::remove_dir_all(&self.path) {
            if e.kind() != std::io::ErrorKind::NotFound {
                warn!(
                    "failed to remove export staging dir {}: {e}",
                    self.path.display()
                );
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn replace_export_dir_restores_prior_export_when_staging_rename_fails() {
        let temp = tempfile::tempdir().expect("create temp dir");
        let final_dir = temp.path().join("album");
        std::fs::create_dir(&final_dir).expect("create prior export dir");
        std::fs::write(final_dir.join(EXPORT_MARKER_FILE), b"release-1")
            .expect("write prior marker");
        std::fs::write(final_dir.join("prior.txt"), b"prior").expect("write prior export");

        let missing_staging = temp.path().join("missing-staging");
        replace_export_dir(&missing_staging, &final_dir, "album", "release-1")
            .expect_err("missing staging fails");

        assert_eq!(
            std::fs::read(final_dir.join("prior.txt")).expect("read prior export"),
            b"prior"
        );
        assert!(!temp.path().join(".album.replace-release-1").exists());
    }

    #[test]
    fn replace_export_dir_refuses_to_replace_unmarked_directory() {
        let temp = tempfile::tempdir().expect("create temp dir");
        let final_dir = temp.path().join("album");
        std::fs::create_dir(&final_dir).expect("create existing dir");
        std::fs::write(final_dir.join("user-file.txt"), b"user data").expect("write user file");

        let staging_dir = temp.path().join("staging");
        std::fs::create_dir(&staging_dir).expect("create staging dir");
        std::fs::write(staging_dir.join(EXPORT_MARKER_FILE), b"release-1").expect("write marker");
        std::fs::write(staging_dir.join("export.txt"), b"export").expect("write export");

        let error = replace_export_dir(&staging_dir, &final_dir, "album", "release-1")
            .expect_err("unmarked existing target must be refused");

        assert!(
            error.to_string().contains("is not a prior bae export"),
            "unexpected error: {error}",
        );
        assert_eq!(
            std::fs::read(final_dir.join("user-file.txt")).expect("read user file"),
            b"user data",
        );
        assert!(staging_dir.exists());
        assert!(!temp.path().join(".album.replace-release-1").exists());
    }

    #[test]
    fn replace_export_dir_replaces_marked_directory() {
        let temp = tempfile::tempdir().expect("create temp dir");
        let final_dir = temp.path().join("album");
        std::fs::create_dir(&final_dir).expect("create prior export dir");
        std::fs::write(final_dir.join(EXPORT_MARKER_FILE), b"release-1")
            .expect("write prior marker");
        std::fs::write(final_dir.join("prior.txt"), b"prior").expect("write prior export");

        let staging_dir = temp.path().join("staging");
        std::fs::create_dir(&staging_dir).expect("create staging dir");
        std::fs::write(staging_dir.join(EXPORT_MARKER_FILE), b"release-1").expect("write marker");
        std::fs::write(staging_dir.join("export.txt"), b"export").expect("write export");

        replace_export_dir(&staging_dir, &final_dir, "album", "release-1")
            .expect("marked prior export can be replaced");

        assert_eq!(
            std::fs::read(final_dir.join("export.txt")).expect("read export"),
            b"export",
        );
        assert!(!final_dir.join("prior.txt").exists());
        assert!(!temp.path().join(".album.replace-release-1").exists());
    }
}
