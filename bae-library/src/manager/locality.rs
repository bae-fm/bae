//! Release file transitions: making uploaded releases local through the
//! download queue, unpinning, and cancelling whichever transition runs.

use super::*;
use bae_diagnostics::{LocalId, TelemetryEvent};
use bae_transfers::storage_moves::UnpinRefusal;

impl LibraryManager {
    /// Queue the uploaded releases of a selection to be made local into
    /// `folder`, a folder the person picked, each in a folder of its own
    /// below it named for the release, in one write that passes over a
    /// release the library does not hold, one that is local, and one already
    /// waiting or downloading. Coven downloads each release's files there,
    /// then one write switches them to this device only, each copy becoming
    /// the original it records, and takes the release out of the queue. A
    /// failure or a cancel before that write removes every copy and folder the
    /// call made, and the release stays in the cloud. Wakes the parked
    /// worker.
    #[cfg(desktop)]
    pub async fn make_releases_local(
        &self,
        release_ids: Vec<String>,
        folder: &std::path::Path,
    ) -> Result<(), LibraryError> {
        let enqueued_at = bae_store::utc_now(self.clock.as_ref()).timestamp_millis();
        match self
            .downloads
            .enqueue_make_locals(release_ids, folder.to_path_buf(), enqueued_at)
            .await
        {
            Ok(true) => {}
            Ok(false) => tracing::debug!("make_releases_local: nothing to make local"),
            Err(error) => {
                self.diagnostics.report_failure("queue releases to make local", &error);
                return Err(error.into());
            }
        }
        Ok(())
    }

    /// Start the make-local the download queue took up: list the copies it
    /// downloads, and run the download and the write that switches the files
    /// on a task of its own, which a cancel or a pause aborts. The abort drops
    /// coven's download, which removes every copy and folder it made.
    #[cfg(desktop)]
    pub(super) async fn start_make_local(
        &self,
        release_id: String,
        folder: std::path::PathBuf,
        entry: bae_transfers::ActiveEntry<bae_transfers::DownloadRequest>,
    ) -> Result<
        (
            bae_transfers::DownloadTransfer,
            bae_transfers::RunningOp<
                std::pin::Pin<
                    Box<
                        dyn std::future::Future<
                                Output = Result<bae_transfers::Finished, LibraryError>,
                            > + Send,
                    >,
                >,
            >,
        ),
        LibraryError,
    > {
        let files = self.local_copy_paths(&release_id).await?;
        let total = self.release_bytes(&release_id).await?;
        let manager = self.clone();
        let task = self.runtime_handle.spawn(async move {
            let file_count = u32::try_from(files.len()).expect("a release's files fit a u32");
            let made = caught(
                "make a release local",
                manager.make_local_into(&release_id, &folder, files, entry),
            )
            .await
            .unwrap_or_else(|bug| Err(manager.panicked(bug).into()));
            match &made {
                Ok(_) => {
                    tracing::info!(%release_id, "release made local");
                    manager.record_telemetry(TelemetryEvent::StorageTransferCompleted {
                        action: ReleaseStorageAction::DetachAudio,
                        release_id: LocalId(release_id),
                        file_count,
                    });
                }
                Err(error) => {
                    tracing::error!(%release_id, failure = %error, "making a release local failed");
                    manager.record_telemetry(TelemetryEvent::StorageTransferFailed {
                        action: ReleaseStorageAction::DetachAudio,
                        release_id: LocalId(release_id),
                    });
                }
            }
            made
        });
        let abort = task.abort_handle();
        let manager = self.clone();
        let outcome = async move {
            match task.await {
                Ok(made) => made,
                Err(join_error) if join_error.is_cancelled() => {
                    Err(LibraryError::Storage("making the release local was cancelled".to_string()))
                }
                Err(join_error) => Err(manager
                    .bug(bae_util::task_failure::TaskFailed::of_join(
                        "make a release local",
                        join_error,
                    ))
                    .into()),
            }
        };
        Ok((
            bae_transfers::DownloadTransfer::MakeLocal(
                bae_transfers::MakeLocalTransferProgress::begin(total),
            ),
            bae_transfers::RunningOp::new(abort, outcome).boxed(),
        ))
    }

    /// Download `files` into `folder`, then switch every file of the release
    /// to this device only and take its queue entry out, in one write.
    #[cfg(desktop)]
    async fn make_local_into(
        &self,
        release_id: &str,
        folder: &std::path::Path,
        files: Vec<(coven::FileId, coven::RelativePath)>,
        entry: bae_transfers::ActiveEntry<bae_transfers::DownloadRequest>,
    ) -> Result<bae_transfers::Finished, LibraryError> {
        // Only dropping the call stops it: the entry's cancel aborts the task.
        let (_stop, stopped) = tokio::sync::watch::channel(false);
        let prepared = self
            .store
            .prepare_device_only(folder, files, &stopped, |progress| {
                self.downloads.set_active_progress(
                    release_id,
                    bae_transfers::DownloadTransfer::MakeLocal(
                        bae_transfers::MakeLocalTransferProgress::of(progress),
                    ),
                );
            })
            .await
            .map_err(|error| LibraryError::MakeLocal(Box::new(error)))?;
        Ok(self
            .store
            .call(move |sql| {
                sql.make_device_only(prepared)?;
                entry.finish_on(sql)
            })
            .await?)
    }

    /// Each release file's coven file with the path below the person's folder
    /// its copy goes to: the release's folder, then the file's names. Covers
    /// keep their coven files: they are bae's own, and every device shows
    /// them.
    ///
    /// `original_filename` and `source_folder_name` are synced columns
    /// another device may have written, so each path is its names, and coven
    /// writes each name through the rule it uses for its own copies (see
    /// [`bae_records::stored_name`]): every name is one name inside the
    /// person's folder. A release carrying a name that names no file, or no
    /// folder name, makes no file local.
    #[cfg(desktop)]
    async fn local_copy_paths(
        &self,
        release_id: &str,
    ) -> Result<Vec<(coven::FileId, coven::RelativePath)>, LibraryError> {
        let release = self.get_release_by_id(release_id).await?.ok_or_else(|| {
            LibraryError::release_missing(release_id, crate::manager::ReleaseWork::Transfer)
        })?;
        let folder = release.source_folder_name.ok_or_else(|| {
            LibraryError::Storage(format!(
                "release {release_id} has no source folder name; cannot place its folder"
            ))
        })?;
        let files = self.records.get_files_for_release(release_id).await?;
        if files.is_empty() {
            return Err(LibraryError::Storage("Release has no files".to_string()));
        }
        files
            .iter()
            .map(|f| {
                let path = bae_records::stored_name::copy_path(
                    release_id,
                    &folder,
                    &f.id,
                    &f.original_filename,
                )?;
                Ok((f.file, path))
            })
            .collect()
    }

    /// The bytes of the release's files, as the records hold their sizes.
    #[cfg(desktop)]
    async fn release_bytes(&self, release_id: &str) -> Result<u64, LibraryError> {
        let files = self.records.get_files_for_release(release_id).await?;
        files
            .iter()
            .map(|file| bae_store::query::to_u64(file.file_size, "release_files.file_size"))
            .try_fold(0u64, |total, bytes| Ok(total + bytes?))
    }

    /// Unpin the releases of a selection, every one in one write: coven moves
    /// their files out of the pinned cache and into the evictable cache, so
    /// they stay readable but become droppable. No release shows a transition
    /// on the way. A release that cannot unpin changes nothing and the rest
    /// unpin; the first such release is then the error returned.
    pub async fn unpin_releases(&self, release_ids: &[String]) -> Result<(), LibraryError> {
        let store = self.store.clone();
        let selection = release_ids.to_vec();
        let unpinned = self
            .run_file_operation("unpin releases", move |_closing| async move {
                Ok(bae_transfers::storage_moves::unpin_releases(&store, selection).await?)
            })
            .await?;
        for (release_id, file_count) in unpinned.unpinned {
            tracing::info!(%release_id, "release unpinned");
            self.record_telemetry(TelemetryEvent::StorageTransferCompleted {
                action: ReleaseStorageAction::Unpin,
                release_id: LocalId(release_id),
                file_count,
            });
        }
        let mut first = None;
        for (release_id, refusal) in unpinned.refused {
            let error = match refusal {
                UnpinRefusal::Missing => {
                    LibraryError::release_missing(&release_id, ReleaseWork::Transfer)
                }
                UnpinRefusal::NoFiles => LibraryError::Storage("Release has no files".to_string()),
                UnpinRefusal::NotInStorage => {
                    LibraryError::Storage("Cannot unpin a local release".to_string())
                }
            };
            tracing::error!(%release_id, failure = %error, "release unpin refused");
            self.record_telemetry(TelemetryEvent::StorageTransferFailed {
                action: ReleaseStorageAction::Unpin,
                release_id: LocalId(release_id),
            });
            first.get_or_insert(error);
        }
        first.map_or(Ok(()), Err)
    }

    /// Cancel the in-progress transition for a release, whatever it is. The
    /// UI calls this from the storage row and the queue pane without knowing
    /// which is running — a release is in at most one transition at a time.
    /// A no-op if nothing is in progress.
    ///
    /// A pin or a make-local is stopped through the download queue, which
    /// also holds the ones not yet started. Cancelling an upload cancels the
    /// release's move to the cloud, and its files stay on this device only,
    /// as does ending a failed move. An unpin can't be stopped.
    pub async fn cancel_release_transition(&self, release_id: &str) -> Result<(), LibraryError> {
        if self.downloads.contains(release_id).await? {
            self.cancel_download(release_id).await?;
        }
        self.cancel_release_upload(release_id).await
    }

    /// Drain a pin's progress channel until it says how the pin ended,
    /// handing its progress to the download queue. On `Failed` the pin's
    /// error is surfaced to the caller. The pin runs on a task of its own,
    /// which writes and ends its queue entry, so dropping this drain stops
    /// neither.
    pub(super) async fn drive_pin(
        &self,
        release_id: &str,
        mut rx: crate::storage::transfer::TransferProgressFeed,
    ) -> Result<(), LibraryError> {
        use crate::storage::transfer::TransferProgress;

        let outcome = loop {
            let Some(progress) = rx.recv().await else {
                break Err(LibraryError::Storage(
                    "Pin ended without completion or failure".to_string(),
                ));
            };
            match progress {
                TransferProgress::Progress { progress } => {
                    if !self.downloads.set_active_progress(
                        release_id,
                        bae_transfers::DownloadTransfer::Pin(progress),
                    ) {
                        tracing::warn!(
                            release_id,
                            "ignored download progress for missing active queue row"
                        );
                    }
                }
                TransferProgress::Complete { .. } => break Ok(()),
                TransferProgress::Failed { error, .. } => break Err(error),
            }
        };

        outcome
    }
}

#[cfg(test)]
#[path = "locality_tests.rs"]
mod tests;
