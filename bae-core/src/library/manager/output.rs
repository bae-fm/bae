//! Export domain operations for [`LibraryManager`].

use super::*;
use crate::storage::path_fragment::validate_path_fragment;
use tracing::info;

/// A hidden file bae writes into every output folder — export or save — naming
/// the release id. It is what lets a re-run safely replace a prior bae output of
/// the same folder; an unmarked directory at the target is never touched.
const OUTPUT_MARKER_FILE: &str = ".bae-output";

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

    /// Enqueue a release to export its files verbatim to `target_dir` — the
    /// inverse of import, reconstructing the imported file set byte-for-byte.
    pub async fn enqueue_export(
        &self,
        release_id: &str,
        target_dir: std::path::PathBuf,
    ) -> Result<(), LibraryError> {
        self.enqueue_output(release_id, target_dir, crate::library::OutputKind::Export)
            .await
    }

    /// Enqueue a release-level save to `target_dir` under the preset named by
    /// `preset_id`. The preset is resolved (must exist and apply to release
    /// saves) and captured whole into the queue payload, so a later config edit
    /// or delete can't change or break this queued save.
    pub async fn enqueue_release_save(
        &self,
        release_id: &str,
        target_dir: std::path::PathBuf,
        preset_id: &str,
    ) -> Result<(), LibraryError> {
        let preset = self
            .save_presets()
            .into_iter()
            .find(|preset| preset.id == preset_id && preset.applies_to_release)
            .ok_or_else(|| {
                LibraryError::Save(format!(
                    "save preset {preset_id} is not available for release save"
                ))
            })?;
        self.enqueue_output(
            release_id,
            target_dir,
            crate::library::OutputKind::Save { preset },
        )
        .await
    }

    /// Shared enqueue body for both release-level outputs. Skips ids already in
    /// the queue (any state); otherwise resolves its title / file_count /
    /// total_size from its storage summary so the Exporting pane can render the
    /// row without a re-query. Wakes the parked worker and emits a fresh
    /// `OutputQueueChanged`.
    async fn enqueue_output(
        &self,
        release_id: &str,
        target_dir: std::path::PathBuf,
        kind: crate::library::OutputKind,
    ) -> Result<(), LibraryError> {
        // The target dir round-trips back to the Exporting pane as a string (the
        // snapshot renders `target_dir` for each row), so it must be valid UTF-8.
        // Fail loud here rather than let a non-UTF-8 path be lossily rewritten into
        // a different directory the output would then write to.
        target_dir.to_str().ok_or_else(|| {
            LibraryError::Export(format!(
                "export target directory is not valid UTF-8: {}",
                target_dir.display()
            ))
        })?;

        if self.outputs.contains(release_id) {
            debug!("enqueue_output: {release_id} already queued, skipping");
            return Ok(());
        }
        let summary = self
            .find_release_storage_summary(release_id)
            .await?
            .ok_or_else(|| {
                LibraryError::Export(format!("cannot export release {release_id}: not found"))
            })?;

        let op = crate::library::OutputOp {
            release_id: release_id.to_string(),
            title: summary.album_title,
            file_count: summary.file_count,
            total_size: summary.total_size,
            created_at: self.clock.now().timestamp_millis(),
            payload: crate::library::output_snapshot::OutputRequest { target_dir, kind },
            state: crate::library::OutputState::Queued,
        };
        self.outputs.enqueue_all([op]);
        Ok(())
    }

    /// Pause or resume the export queue. While paused the worker parks instead of
    /// starting the next release; the in-flight one runs to completion. Resuming
    /// wakes the worker. Emits a fresh `OutputQueueChanged`.
    pub fn set_outputs_paused(&self, paused: bool) {
        self.outputs.set_paused(paused);
    }

    /// Cancel a release's export. Drops a queued/failed entry; for the active one,
    /// aborts its in-flight copy task. The copy writes every file into a hidden
    /// staging directory and renames it into place only after all files succeed, so
    /// the abort leaves no output at the final path — the staging directory is
    /// removed when the aborted task's future drops. Emits a fresh snapshot.
    pub fn cancel_output(&self, release_id: &str) {
        self.outputs.cancel(release_id);
    }

    /// Flip every failed export back to queued and wake the worker to retry them.
    /// Emits a fresh `OutputQueueChanged`.
    pub fn retry_outputs(&self) {
        self.outputs.retry_failed();
    }

    /// The serial export worker: drains the export queue one release at a time
    /// through [`run_serial_worker`], which owns the queue protocol (the
    /// activate/cancel race, cancel-is-not-a-failure, remove vs mark-failed).
    ///
    /// All this supplies is how an export runs — spawn the copy task and yield its
    /// outcome, mapping a panicked task to a failure that names the panic — and the
    /// diagnostics event for the outcome. The copy itself updates the queue's
    /// per-release percent and re-emits the snapshot as it writes each file.
    pub(super) async fn run_output_worker(&self) {
        use crate::library::release_queue::RunningOp;
        use crate::library::OutputKind;
        use std::sync::atomic::{AtomicBool, Ordering};

        // The completion telemetry differs by kind (export vs save), but the
        // `on_done` hook only receives the release id. The worker is strictly
        // serial — start, then on_done, never interleaved — so the running op's
        // kind is stashed here by `start` and read back by `on_done`. An atomic
        // (not a `Cell`) so the borrowing closures stay `Send`, which the spawned
        // worker future requires.
        let running_is_save = AtomicBool::new(false);

        self.outputs
            .run_serial(
                "Export",
                |op| {
                    running_is_save.store(
                        matches!(op.payload.kind, OutputKind::Save { .. }),
                        Ordering::Relaxed,
                    );
                    let release_id = op.release_id.clone();
                    let worker = self.clone();
                    let task = self.runtime_handle.spawn(async move {
                        worker
                            .export_release_to_dir(&op.release_id, op.payload)
                            .await
                    });
                    let abort = task.abort_handle();
                    async move {
                        let outcome = async move {
                            match task.await {
                                Ok(result) => result,
                                // Aborted by a cancel, which also removed the entry — the
                                // driver's `contains` check is what reads that as a cancel.
                                Err(join_error) if join_error.is_cancelled() => {
                                    Err(LibraryError::Export(format!(
                                        "export of {release_id} was cancelled"
                                    )))
                                }
                                Err(join_error) => Err(LibraryError::Export(format!(
                                    "export task for {release_id} panicked: {join_error}"
                                ))),
                            }
                        };
                        Ok((0, RunningOp::new(abort, outcome)))
                    }
                },
                |release_id, result| {
                    let release_id = crate::diagnostics::LocalId(release_id.to_string());
                    let is_save = running_is_save.load(Ordering::Relaxed);
                    self.diagnostics.event(match (is_save, result) {
                        (false, Ok(())) => TelemetryEvent::ExportCompleted { release_id },
                        (false, Err(_)) => TelemetryEvent::ExportFailed { release_id },
                        (true, Ok(())) => TelemetryEvent::SaveCompleted { release_id },
                        (true, Err(_)) => TelemetryEvent::SaveFailed { release_id },
                    });
                },
            )
            .await
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
        request: crate::library::output_snapshot::OutputRequest,
    ) -> Result<(), LibraryError> {
        let release = self
            .get_release_by_id(release_id)
            .await?
            .ok_or_else(|| LibraryError::Export(format!("release not found: {release_id}")))?;
        let folder = release.source_folder_name.ok_or_else(|| {
            LibraryError::Export(format!(
                "release {release_id} has no source folder name; cannot reconstruct its folder"
            ))
        })?;
        validate_path_fragment(release_id, "source_folder_name", &folder)?;

        let final_dir = request.target_dir.join(&folder);
        // Stage under the target dir (not a temp dir elsewhere) so the final rename
        // stays on one filesystem and is atomic. The name is hidden and carries the
        // release id so a concurrent export of a different release can't collide.
        let staging = StagingDir::create(
            request
                .target_dir
                .join(format!(".{folder}.export-{release_id}")),
        )?;

        match request.kind {
            crate::library::OutputKind::Export => {
                self.copy_release_files_to_staging(release_id, &folder, staging.path())
                    .await?;
            }
            crate::library::OutputKind::Save { preset } => {
                info!(
                    release_id,
                    folder = folder.as_str(),
                    kind = "save",
                    preset = preset.id.as_str(),
                    "Writing release output"
                );
                self.save_release_tracks_to_dir(release_id, preset, staging.path())
                    .await?;
            }
        }

        write_output_marker(staging.path(), release_id)?;
        replace_output_dir(staging.path(), &final_dir, &folder, release_id)?;
        staging.disarm();
        Ok(())
    }

    /// Write a release's output to `target_dir`, bypassing the output queue.
    /// Production always goes through `enqueue_export` / `enqueue_release_save`;
    /// only a test helper for exercising the write directly.
    #[cfg(any(test, feature = "test-utils"))]
    pub async fn export_release(
        &self,
        release_id: &str,
        target_dir: &Path,
        kind: crate::library::OutputKind,
    ) -> Result<(), LibraryError> {
        self.export_release_to_dir(
            release_id,
            crate::library::output_snapshot::OutputRequest {
                target_dir: target_dir.to_path_buf(),
                kind,
            },
        )
        .await
    }

    pub(super) fn set_output_progress(&self, release_id: &str, percent: u8) {
        self.outputs.set_active_percent(release_id, percent);
    }
}

fn write_output_marker(
    staging_dir: &std::path::Path,
    release_id: &str,
) -> Result<(), LibraryError> {
    std::fs::write(staging_dir.join(OUTPUT_MARKER_FILE), release_id)?;
    Ok(())
}

fn replace_output_dir(
    staging_dir: &std::path::Path,
    final_dir: &std::path::Path,
    folder: &str,
    release_id: &str,
) -> Result<(), LibraryError> {
    let had_existing = final_dir.exists();
    if had_existing && !final_dir.join(OUTPUT_MARKER_FILE).exists() {
        return Err(LibraryError::Export(format!(
            "output target exists and is not a prior bae output: {}",
            final_dir.display()
        )));
    }

    let backup_dir = final_dir.with_file_name(format!(".{folder}.replace-{release_id}"));
    if backup_dir.exists() {
        return Err(LibraryError::Export(format!(
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
                        return Err(LibraryError::Export(format!(
                            "failed to remove prior export backup {} ({cleanup_error}); also failed to move new export back to staging ({move_new_error})",
                            backup_dir.display()
                        )));
                    }
                    if let Err(restore_error) = std::fs::rename(&backup_dir, final_dir) {
                        return Err(LibraryError::Export(format!(
                            "failed to remove prior export backup {} ({cleanup_error}); moved new export back to staging but failed to restore prior export ({restore_error})",
                            backup_dir.display()
                        )));
                    }
                    return Err(LibraryError::Export(format!(
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
                    return Err(LibraryError::Export(format!(
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn replace_output_dir_restores_prior_export_when_staging_rename_fails() {
        let temp = tempfile::tempdir().expect("create temp dir");
        let final_dir = temp.path().join("album");
        std::fs::create_dir(&final_dir).expect("create prior export dir");
        std::fs::write(final_dir.join(OUTPUT_MARKER_FILE), b"release-1")
            .expect("write prior marker");
        std::fs::write(final_dir.join("prior.txt"), b"prior").expect("write prior export");

        let missing_staging = temp.path().join("missing-staging");
        replace_output_dir(&missing_staging, &final_dir, "album", "release-1")
            .expect_err("missing staging fails");

        assert_eq!(
            std::fs::read(final_dir.join("prior.txt")).expect("read prior export"),
            b"prior"
        );
        assert!(!temp.path().join(".album.replace-release-1").exists());
    }

    #[test]
    fn replace_output_dir_refuses_to_replace_unmarked_directory() {
        let temp = tempfile::tempdir().expect("create temp dir");
        let final_dir = temp.path().join("album");
        std::fs::create_dir(&final_dir).expect("create existing dir");
        std::fs::write(final_dir.join("user-file.txt"), b"user data").expect("write user file");

        let staging_dir = temp.path().join("staging");
        std::fs::create_dir(&staging_dir).expect("create staging dir");
        std::fs::write(staging_dir.join(OUTPUT_MARKER_FILE), b"release-1").expect("write marker");
        std::fs::write(staging_dir.join("export.txt"), b"export").expect("write export");

        let error = replace_output_dir(&staging_dir, &final_dir, "album", "release-1")
            .expect_err("unmarked existing target must be refused");

        assert!(
            error.to_string().contains("is not a prior bae output"),
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
    fn replace_output_dir_replaces_marked_directory() {
        let temp = tempfile::tempdir().expect("create temp dir");
        let final_dir = temp.path().join("album");
        std::fs::create_dir(&final_dir).expect("create prior export dir");
        std::fs::write(final_dir.join(OUTPUT_MARKER_FILE), b"release-1")
            .expect("write prior marker");
        std::fs::write(final_dir.join("prior.txt"), b"prior").expect("write prior export");

        let staging_dir = temp.path().join("staging");
        std::fs::create_dir(&staging_dir).expect("create staging dir");
        std::fs::write(staging_dir.join(OUTPUT_MARKER_FILE), b"release-1").expect("write marker");
        std::fs::write(staging_dir.join("export.txt"), b"export").expect("write export");

        replace_output_dir(&staging_dir, &final_dir, "album", "release-1")
            .expect("marked prior export can be replaced");

        assert_eq!(
            std::fs::read(final_dir.join("export.txt")).expect("read export"),
            b"export",
        );
        assert!(!final_dir.join("prior.txt").exists());
        assert!(!temp.path().join(".album.replace-release-1").exists());
    }
}
