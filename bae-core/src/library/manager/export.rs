//! Export domain operations for [`LibraryManager`].

use super::*;

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
    /// render the row without a re-query. Spawns the single worker on the first
    /// enqueue, then wakes it. Emits a fresh `ExportQueueChanged`.
    pub async fn enqueue_export(
        &self,
        release_id: &str,
        target_dir: std::path::PathBuf,
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
            payload: target_dir,
            state: crate::library::ExportState::Queued,
        };
        if self.export_queue.enqueue(op) {
            self.ensure_export_worker();
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

    /// Spawn the single serial export worker if it isn't running yet. Claimed
    /// exactly once across all manager clones; safe to call on every enqueue.
    fn ensure_export_worker(&self) {
        if self.export_queue.claim_worker_spawn() {
            let manager = self.clone();
            self.runtime_handle.spawn(async move {
                manager.run_export_worker().await;
            });
        }
    }

    /// The serial export worker loop. Parks on the queue's `Notify` whenever the
    /// queue is paused or holds nothing queued; otherwise takes the next queued
    /// release and copies it out. Processes strictly one release at a time.
    async fn run_export_worker(&self) {
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
    async fn run_queued_export(&self, op: crate::library::ExportOp) {
        let release_id = op.release_id.clone();
        let target_dir = op.payload.clone();

        let worker = self.clone();
        let task_release_id = release_id.clone();
        let task = self.runtime_handle.spawn(async move {
            worker
                .export_release_to_dir(&task_release_id, &target_dir)
                .await
        });
        let abort = task.abort_handle();

        // Flip to Active and register the abort handle atomically. If a cancel
        // removed the entry in the gap since we picked it, abort the task we just
        // spawned and bail — nothing was written.
        if !self.export_queue.activate(&release_id, abort.clone()) {
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
    async fn export_release_to_dir(
        &self,
        release_id: &str,
        target_dir: &std::path::Path,
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

        let final_dir = target_dir.join(&folder);
        // Stage under the target dir (not a temp dir elsewhere) so the final rename
        // stays on one filesystem and is atomic. The name is hidden and carries the
        // release id so a concurrent export of a different release can't collide.
        let staging =
            StagingDir::create(target_dir.join(format!(".{folder}.export-{release_id}")))?;

        let files = self.database.get_files_for_release(release_id).await?;
        info!(
            release_id,
            folder = folder.as_str(),
            file_count = files.len(),
            "Exporting release files verbatim"
        );

        let total = files.len();
        for (index, file) in files.iter().enumerate() {
            self.export_one_file(file, staging.path()).await?;
            let percent = (((index + 1) * 100) / total.max(1)) as u8;
            self.export_queue.set_active_percent(release_id, percent);
            self.emit_export_queue_changed();
        }

        // Every file landed. Re-export replaces any prior copy: remove the existing
        // final dir immediately before the rename. This leaves an unavoidable window
        // where the final path is briefly absent, kept minimal by doing all the slow
        // work (cloud reads, writes) into staging first so only the rename remains.
        if final_dir.exists() {
            std::fs::remove_dir_all(&final_dir)?;
        }
        std::fs::rename(staging.path(), &final_dir)?;
        staging.disarm();
        Ok(())
    }

    /// Copy one release file's verbatim bytes to `<staging_dir>/<original_filename>`.
    /// `original_filename` may name a subfolder (e.g. `CD1/CDImage.ape`), so its
    /// parent is created first. No per-file temp is needed: the whole staging
    /// directory is the atomic unit, renamed into place only once every file is
    /// written.
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
    ) -> Result<(), LibraryError> {
        self.export_release_to_dir(release_id, target_dir).await
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
    /// neighbour counts, the metadata selection, and the raw audio-format
    /// aggregate for decoding. Cloud-only tracks download + decrypt here —
    /// export never requires a local copy.
    #[cfg(not(any(target_os = "ios", target_os = "android")))]
    pub async fn get_export_track_plan(
        &self,
        track_id: &str,
    ) -> Result<ExportTrackPlan, LibraryError> {
        let meta = TrackAudioMeta::resolve(&self.database, track_id).await?;
        let resolved = self.resolve_export_tags(&meta).await?;

        let audio_bytes =
            crate::storage::local::transfer::read_release_file_bytes(&meta.audio_file, self)
                .await
                .map_err(|e| {
                    LibraryError::TrackMapping(format!(
                        "Couldn't read audio for track {track_id}: {e}"
                    ))
                })?;

        let selection = self.export_metadata();

        // Read the cover only when the user selected it: the album's primary
        // release carries it, reached through the id `resolve_export_tags`
        // already carried out. Skipping this when cover art is off
        // short-circuits the cloud image fetch + resize entirely.
        let cover_image_bytes = if selection.cover_art {
            match resolved.primary_release_id.as_deref() {
                Some(rid) => match self.cover_ref(rid).await? {
                    Some(image) => self.read_image_blob(&image).await?,
                    None => None,
                },
                None => None,
            }
        } else {
            None
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
            metadata: selection,
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
    pub async fn export_track(
        &self,
        track_id: &str,
        output_path: &Path,
        format: crate::library::ExportFormat,
    ) -> Result<(), LibraryError> {
        let plan = self.get_export_track_plan(track_id).await?;
        ExportService::export_track(plan, output_path, format)
            .await
            .map_err(LibraryError::Import)
    }
}

/// A staging directory for one in-flight release export, removed on drop unless
/// [`disarm`](StagingDir::disarm)ed. The export writes every file here and, once
/// all succeed, renames it into place and disarms the guard. Any earlier exit —
/// a read/write error (`?`), a panic, or the worker task being aborted on cancel
/// (which drops this future) — drops the guard, which removes the directory. That
/// is what keeps a failed or cancelled export from leaving partial output behind.
struct StagingDir {
    path: std::path::PathBuf,
    armed: bool,
}

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
