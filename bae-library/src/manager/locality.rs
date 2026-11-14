//! Release file transitions: making uploaded releases local through the
//! download queue, unpinning, and cancelling whichever transition runs.

use super::*;
use bae_diagnostics::{LocalId, TelemetryEvent};
use bae_transfers::storage_moves::UnpinRefusal;

impl LibraryManager {
    /// Queue the uploaded releases of a selection to be moved to this device into
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
    pub async fn move_releases_to_this_device(
        &self,
        release_ids: Vec<String>,
        folder: &std::path::Path,
    ) -> Result<(), LibraryError> {
        let enqueued_at = bae_store::utc_now(self.clock.as_ref()).timestamp_millis();
        match self
            .downloads
            .enqueue_moves_to_this_device(release_ids, folder.to_path_buf(), enqueued_at)
            .await
        {
            Ok(true) => {}
            Ok(false) => {
                tracing::debug!("move_releases_to_this_device: nothing to move to this device")
            }
            Err(error) => {
                self.diagnostics.report_failure("queue releases to move to this device", &error);
                return Err(error.into());
            }
        }
        Ok(())
    }

    /// Start the move-to-this-device the download queue took up: list the copies it
    /// downloads, and run the download and the write that switches the files
    /// on a task of its own, which a cancel or a pause aborts. The abort drops
    /// coven's download, which removes every copy and folder it made.
    #[cfg(desktop)]
    pub(super) async fn start_move_to_this_device(
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
        let file_count = u32::try_from(files.len()).expect("a release's files fit a u32");
        let local_release_id = release_id.clone();
        let running = self.spawn_ending(
            "make a release local",
            async move { manager.move_to_this_device_into(&local_release_id, &folder, files, entry).await },
            {
                let manager = self.clone();
                move |made| {
                    let made = made.unwrap_or_else(|bug| Err(bug.into()));
                    match &made {
                        Ok(_) => {
                            tracing::info!(%release_id, "release moved to this device");
                            manager.record_telemetry(TelemetryEvent::StorageTransferCompleted {
                                action: ReleaseStorageAction::MoveToThisDevice,
                                release_id: LocalId(release_id),
                                file_count,
                            });
                        }
                        Err(error) => {
                            tracing::error!(%release_id, failure = %error, "making a release local failed");
                            manager.record_telemetry(TelemetryEvent::StorageTransferFailed {
                                action: ReleaseStorageAction::MoveToThisDevice,
                                release_id: LocalId(release_id),
                            });
                        }
                    }
                    made
                }
            },
        );
        let running = running.then(|ended| async move {
            match ended.await {
                Ok(made) => made,
                Err(bae_diagnostics::Unfinished::Stopped) => {
                    Err(LibraryError::Storage("making the release local was cancelled".to_string()))
                }
                // A panic recording how the release ended, reported as it
                // happened.
                Err(bae_diagnostics::Unfinished::Panicked(bug)) => Err(bug.into()),
            }
        });
        Ok((
            bae_transfers::DownloadTransfer::MoveToThisDevice(
                bae_transfers::MoveToThisDeviceTransferProgress::begin(total),
            ),
            running.boxed(),
        ))
    }

    /// Download `files` into `folder`, then switch every file of the release
    /// to this device only and take its queue entry out, in one write.
    #[cfg(desktop)]
    async fn move_to_this_device_into(
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
                    bae_transfers::DownloadTransfer::MoveToThisDevice(
                        bae_transfers::MoveToThisDeviceTransferProgress::of(progress),
                    ),
                );
            })
            .await
            .map_err(|error| LibraryError::MoveToThisDevice(Box::new(error)))?;
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
    /// person's folder. A release with no folder name makes no file local.
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
        Ok(files
            .iter()
            .map(|f| (f.file, bae_records::stored_name::copy_path(&folder, &f.original_filename)))
            .collect())
    }

    /// The bytes of the release's files, as the records hold their sizes.
    #[cfg(desktop)]
    async fn release_bytes(&self, release_id: &str) -> Result<u64, LibraryError> {
        let files = self.records.get_files_for_release(release_id).await?;
        Ok(files.iter().map(|file| file.file_size).sum())
    }

    /// Unpin the releases of a selection, every one in one write: their files
    /// leave this device in that write, and play from the cloud again. No
    /// release shows a transition on the way. A release that cannot unpin
    /// changes nothing and the rest unpin; the first such release is then the
    /// error returned.
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
    /// A pin or a move-to-this-device is stopped through the download queue, which
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
