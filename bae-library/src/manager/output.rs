//! Export domain operations for [`LibraryManager`].

use super::*;
use tracing::info;

/// A hidden file bae writes into every output folder — export or save — naming
/// the release id. It is what lets a re-run safely replace a prior bae output of
/// the same folder; an unmarked directory at the target is never touched.
const OUTPUT_MARKER_FILE: &str = ".bae-output";

impl LibraryManager {
    // ── Export queue ─────────────────────────────────────────────────
    //
    // Exporting a release copies its whole file set out verbatim to a
    // user-chosen directory, routed through a serial queue (mirroring the
    // download/pin queue): one release copies out at a time, the rest wait,
    // and the user can pause or cancel. The queue is the `output_queue`
    // table, which the library empties as it opens: on restart it's empty. Export changes no release state: it only reads (through coven's
    // file reader) and writes to a user directory. Its attachments and local
    // source paths remain unchanged.

    /// Enqueue a release to export its files verbatim to `target_dir` — the
    /// inverse of import, reconstructing the imported file set byte-for-byte.
    pub async fn enqueue_export(
        &self,
        release_id: &str,
        target_dir: std::path::PathBuf,
    ) -> Result<(), LibraryError> {
        self.enqueue_output(release_id, target_dir, bae_transfers::OutputKind::Export).await
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
        let preset = self.save_preset_for(preset_id, bae_preferences::SaveLevel::Release).await?;
        self.enqueue_output(release_id, target_dir, bae_transfers::OutputKind::Save { preset })
            .await
    }

    /// Shared enqueue body for both release-level outputs. Skips ids already in
    /// the queue (any state) and fails for a release the library does not
    /// hold. The entry names the release by id only; a reader of the queue
    /// reads its title, file count, and size from the library as it shows the
    /// row. Wakes the parked worker.
    async fn enqueue_output(
        &self,
        release_id: &str,
        target_dir: std::path::PathBuf,
        kind: bae_transfers::OutputKind,
    ) -> Result<(), LibraryError> {
        if self.outputs.contains(release_id).await? {
            debug!("enqueue_output: {release_id} already queued, skipping");
            return Ok(());
        }
        self.get_release_by_id(release_id)
            .await?
            .ok_or_else(|| LibraryError::release_missing(release_id, output_work(&kind)))?;

        let entry = bae_transfers::OutputEntry {
            release_id: release_id.to_string(),
            created_at: bae_store::utc_now(self.clock.as_ref()).timestamp_millis(),
            payload: bae_transfers::OutputRequest { target_dir, kind },
        };
        self.outputs.enqueue_all(vec![entry]).await?;
        Ok(())
    }

    /// Pause or resume the export queue. Pausing stops the running export at once
    /// and leaves its entry queued; while paused the worker parks instead of
    /// starting the next release. Resuming wakes the worker, which starts the
    /// stopped export again from the beginning.
    pub async fn set_outputs_paused(&self, paused: bool) -> Result<(), LibraryError> {
        Ok(self.outputs.set_paused(paused).await?)
    }

    /// Cancel a release's export. Drops a queued/failed entry; for the active one,
    /// aborts its in-flight copy task. The copy writes every file into a hidden
    /// staging directory and renames it into place only after all files succeed, so
    /// the abort leaves no output at the final path — the staging directory is
    /// removed when the aborted task's future drops.
    pub async fn cancel_output(&self, release_id: &str) -> Result<(), LibraryError> {
        Ok(self.outputs.cancel(release_id).await?)
    }

    /// The serial export worker: drains the export queue one release at a time
    /// through [`QueuedReleases::run_serial`], which owns the queue protocol
    /// (the activate/cancel race, cancel-is-not-a-failure, remove vs
    /// mark-failed).
    ///
    /// [`QueuedReleases::run_serial`]: bae_transfers::QueuedReleases::run_serial
    ///
    /// All this supplies is how an export runs — spawn the copy task and yield its
    /// outcome, a panicked task failing it and reported as a bug — and the
    /// diagnostics event for the outcome. The copy itself updates the active
    /// export's percent as it writes each file.
    pub(super) async fn run_output_worker(&self) {
        use bae_transfers::OutputKind;
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
                |op, entry| {
                    running_is_save.store(
                        matches!(op.payload.kind, OutputKind::Save { .. }),
                        Ordering::Relaxed,
                    );
                    let release_id = op.release_id.clone();
                    let worker = self.clone();
                    // A panic fails the entry as any failure does, and is
                    // reported as the bug it is.
                    let running = self.spawn_ending(
                        "export a release",
                        async move {
                            worker.export_release_to_dir(&op.release_id, op.payload).await?;
                            // The files are in place: the export ends its entry.
                            Ok::<_, LibraryError>(
                                worker.store.call(move |sql| entry.finish_on(sql)).await?,
                            )
                        },
                        |exported| exported.unwrap_or_else(|bug| Err(bug.into())),
                    );
                    async move {
                        let running = running.then(|ended| async move {
                            match ended.await {
                                Ok(result) => result,
                                // Aborted by a cancel, which also removed the entry — the
                                // driver's `contains` check is what reads that as a cancel.
                                Err(bae_diagnostics::Unfinished::Stopped) => {
                                    Err(LibraryError::Export(format!(
                                        "export of {release_id} was cancelled"
                                    )))
                                }
                                // `end` only hands the result on; a panic in it
                                // was reported as it happened.
                                Err(bae_diagnostics::Unfinished::Panicked(bug)) => Err(bug.into()),
                            }
                        });
                        Ok((0, running))
                    }
                },
                |release_id, result| {
                    let release_id = bae_diagnostics::LocalId(release_id.to_string());
                    let is_save = running_is_save.load(Ordering::Relaxed);
                    self.diagnostics.event(match (is_save, result) {
                        (false, Ok(())) => TelemetryEvent::ExportCompleted { release_id },
                        (false, Err(_)) => TelemetryEvent::ExportFailed { release_id },
                        (true, Ok(())) => TelemetryEvent::SaveCompleted { release_id },
                        (true, Err(_)) => TelemetryEvent::SaveFailed { release_id },
                    });
                },
                |label, error| self.report_queue_store_failure(label, error),
                |panic| self.panicked(panic),
            )
            .await
    }

    /// Copy a release's files verbatim to `<target_dir>/<source_folder_name>/`,
    /// the folder name written as coven writes a name (see
    /// [`bae_records::stored_name`]), reading each file through coven's
    /// locality-aware read (a Remote release fetches from cloud/cache and
    /// decrypts). The export is all-or-nothing: every
    /// file is written into a hidden staging directory alongside the final one, and
    /// only after all files succeed is the staging directory renamed into place. Any
    /// read/write error, cancel, or panic drops the [`StagingDir`] guard, which
    /// removes the staging directory — so a failed or cancelled export never leaves
    /// partial output at the final path. Updates the active export's percent (by
    /// file index) after each file. Fails loudly when the
    /// release has no source folder name.
    async fn export_release_to_dir(
        &self,
        release_id: &str,
        request: bae_transfers::OutputRequest,
    ) -> Result<(), LibraryError> {
        let release = self
            .get_release_by_id(release_id)
            .await?
            .ok_or_else(|| LibraryError::release_missing(release_id, output_work(&request.kind)))?;
        let folder = release.source_folder_name.clone().ok_or_else(|| {
            LibraryError::Export(format!(
                "release {release_id} has no source folder name; cannot reconstruct its folder"
            ))
        })?;
        let final_dir = request
            .target_dir
            .join(bae_records::stored_name::folder_path(release_id, &folder)?.local_path());
        // Stage under the target dir (not a temp dir elsewhere) so the final rename
        // stays on one filesystem and is atomic. The name is hidden and carries the
        // release id so a concurrent export of a different release can't collide.
        let staging_path = request.target_dir.join(output_sibling(&folder, "export", release_id));
        let staging = self
            .diagnostics
            .blocking("create an output's staging folder", move || StagingDir::create(staging_path))
            .await??;

        match request.kind {
            bae_transfers::OutputKind::Export => {
                self.copy_release_files_to_staging(release_id, &folder, staging.path()).await?;
            }
            bae_transfers::OutputKind::Save { preset } => {
                info!(
                    release_id,
                    folder = folder.as_str(),
                    kind = "save",
                    preset = preset.id.as_str(),
                    "Writing release output"
                );
                self.save_release_tracks_to_dir(&release, &folder, preset, staging.path()).await?;
            }
        }

        let staging_path = staging.path().to_path_buf();
        let marker_release_id = release_id.to_string();
        self.diagnostics
            .blocking("move an output into place", move || -> Result<(), LibraryError> {
                write_output_marker(&staging_path, &marker_release_id)?;
                replace_output_dir(&staging_path, &final_dir, &folder, &marker_release_id)
            })
            .await??;
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
        kind: bae_transfers::OutputKind,
    ) -> Result<(), LibraryError> {
        self.export_release_to_dir(
            release_id,
            bae_transfers::OutputRequest { target_dir: target_dir.to_path_buf(), kind },
        )
        .await
    }

    pub(super) fn set_output_progress(&self, release_id: &str, percent: u8) {
        self.outputs.set_active_percent(release_id, percent);
    }
}

/// How far along an output is, in whole percent, when `done` of its `total`
/// steps are finished. An output with no steps is at zero.
pub(super) fn output_percent(done: usize, total: usize) -> u8 {
    ((done * 100) / total.max(1)) as u8
}

/// The hidden folder beside a release's output folder that an output uses
/// for `purpose`, named after the release's folder name and id, written as
/// coven writes a name (see [`bae_records::stored_name`]).
fn output_sibling(folder: &str, purpose: &str, release_id: &str) -> std::path::PathBuf {
    coven::RelativePath::new([format!(".{folder}.{purpose}-{release_id}")])
        .expect("a name holding the release id is not empty")
        .local_path()
}

/// Write the marker as a new file: when one of the release's own files is
/// named `.bae-output`, it is already there, and the output fails rather than
/// write over it.
fn write_output_marker(staging_dir: &std::path::Path, release_id: &str) -> std::io::Result<()> {
    use std::io::Write;
    std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(staging_dir.join(OUTPUT_MARKER_FILE))?
        .write_all(release_id.as_bytes())
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

    let backup_dir = final_dir.with_file_name(output_sibling(folder, "replace", release_id));
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
    fn create(path: std::path::PathBuf) -> std::io::Result<Self> {
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
                warn!("failed to remove export staging dir {}: {e}", self.path.display());
            }
        }
    }
}

/// What an output of `kind` is, as a refusal of it is shown.
fn output_work(kind: &bae_transfers::OutputKind) -> super::ReleaseWork {
    match kind {
        bae_transfers::OutputKind::Export => super::ReleaseWork::Export,
        bae_transfers::OutputKind::Save { .. } => super::ReleaseWork::Save,
    }
}

#[cfg(test)]
#[path = "output_tests.rs"]
mod tests;
