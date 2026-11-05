//! Release file transitions: a release's storage transition, making an
//! uploaded release local again, unpinning, and cancelling whichever
//! transition runs.

use super::*;

impl LibraryManager {
    /// Run `work` as `release_id`'s transition `action`: its row is written
    /// before the work starts, refused while the release is in another. The
    /// write that finishes the work deletes the row, so the release shows the
    /// transition exactly until its files have changed; a work that fails or
    /// panics has its row deleted in a write of its own.
    pub(crate) async fn run_storage_action<T>(
        &self,
        release_id: &str,
        action: bae_transfers::StoredTransition,
        work: impl std::future::Future<Output = Result<T, LibraryError>>,
    ) -> Result<T, LibraryError> {
        self.begin_storage_action(release_id, action).await?;
        let outcome = caught("run a release's storage transition", work)
            .await
            .unwrap_or_else(|bug| Err(self.panicked(bug).into()));
        match outcome {
            Ok(done) => Ok(done),
            Err(error) => Err(self.end_failed_storage_action(release_id, error).await),
        }
    }

    async fn begin_storage_action(
        &self,
        release_id: &str,
        action: bae_transfers::StoredTransition,
    ) -> Result<(), LibraryError> {
        match bae_transfers::release_storage_action::begin_release_storage_action(
            &self.store,
            release_id,
            action,
        )
        .await?
        {
            true => Ok(()),
            false => Err(LibraryError::Storage(format!(
                "release {release_id} already has an active storage transition"
            ))),
        }
    }

    /// End `release_id`'s transition, whose work failed with `operation`,
    /// and return the failure the call reports. A write that fails is
    /// reported as a bug and joins the work's own failure: the row stays
    /// until the library next opens.
    async fn end_failed_storage_action(
        &self,
        release_id: &str,
        operation: LibraryError,
    ) -> LibraryError {
        let Err(error) = bae_transfers::release_storage_action::end_release_storage_action(
            &self.store,
            release_id,
        )
        .await
        else {
            return operation;
        };
        let cleanup = LibraryError::from(error);
        self.report_bug(
            "end a release's storage transition",
            &bae_util::error_text::CauseChain(&cleanup),
        );
        LibraryError::Cleanup { operation: Box::new(operation), cleanup: Box::new(cleanup) }
    }

    /// Make an uploaded release local again into `new_path`, a folder the
    /// person picked, as the release's make-local transition: coven
    /// downloads each file there, at the path its names give, then one write
    /// switches every file to this device only, each copy becoming the
    /// original it records, and ends the transition. A failure or a cancel
    /// before that write removes every copy and folder the call made, and
    /// the release stays in the cloud.
    pub async fn detach_release_audio_into(
        &self,
        release_id: &str,
        new_path: &std::path::Path,
        cancel: &crate::CancellationToken,
    ) -> Result<(), LibraryError> {
        let files = self.local_copy_paths(release_id).await?;
        let operation = cancel.child_token();
        let cancel_on_drop = operation.clone().drop_guard();
        let manager = self.clone();
        let folder = new_path.to_path_buf();
        let release_id = release_id.to_string();
        let result = self
            .run_file_operation("make release local", move |closing| async move {
                manager
                    .run_storage_action(
                        &release_id,
                        bae_transfers::StoredTransition::DetachAudio,
                        async {
                            let (stop, stopped) = tokio::sync::watch::channel(false);
                            // Listed until the transition ends, however it
                            // ends.
                            let report = manager.make_local_transfers.report(&release_id);
                            let call = manager.store.prepare_device_only(
                                &folder,
                                files,
                                &stopped,
                                |progress| {
                                    report.progress(crate::MakeLocalTransferProgress::of(progress))
                                },
                            );
                            tokio::pin!(call);
                            // A cancel that came first stops the call before
                            // it starts.
                            let prepared = tokio::select! {
                                biased;
                                _ = operation.cancelled() => {
                                    stop.send_replace(true);
                                    call.await
                                }
                                _ = closing.cancelled() => {
                                    stop.send_replace(true);
                                    call.await
                                }
                                result = &mut call => result,
                            }
                            .map_err(|error| LibraryError::MakeLocal(Box::new(error)))?;
                            Ok(bae_transfers::release_storage_action::make_release_local(
                                &manager.store,
                                &release_id,
                                prepared,
                            )
                            .await?)
                        },
                    )
                    .await
            })
            .await;
        cancel_on_drop.disarm();
        result
    }

    /// Each release file's coven file with the path below the person's folder
    /// its copy goes to. Covers keep their coven files: they are bae's own,
    /// and every device shows them.
    ///
    /// `original_filename` is a synced column another device may have
    /// written, so each path is its names, split at `/`, and coven writes
    /// each name through the rule it uses for its own copies (see
    /// [`bae_records::stored_name`]): every name is one name inside the
    /// person's folder. A release carrying a name that names no file makes
    /// no file local.
    async fn local_copy_paths(
        &self,
        release_id: &str,
    ) -> Result<Vec<(coven::FileId, coven::RelativePath)>, LibraryError> {
        let files = self.records.get_files_for_release(release_id).await?;
        files
            .iter()
            .map(|f| {
                let path =
                    bae_records::stored_name::file_path(release_id, &f.id, &f.original_filename)?;
                Ok((f.file, path))
            })
            .collect()
    }

    /// Unpin a release: coven moves its files out of the pinned cache and into the
    /// evictable cache, so they stay readable but become droppable.
    pub async fn unpin_release(&self, release_id: &str) -> Result<(), LibraryError> {
        let rx = self.start_unpin_transfer(release_id.to_string());
        self.drive_transfer(release_id, ReleaseStorageAction::Unpin, rx).await
    }

    /// Make an uploaded release Local again into the folder `new_path`, as
    /// a transfer the storage views show and `cancel_release_transition` can
    /// stop until its files switch.
    pub async fn detach_release_audio(
        &self,
        release_id: &str,
        new_path: &std::path::Path,
    ) -> Result<(), LibraryError> {
        // The token `cancel_release_transition` fires to stop this transfer
        // between files, reachable for exactly as long as the transfer runs:
        // its task holds the registration.
        let cancel = crate::CancellationToken::new();
        let registration =
            self.detach_cancels.register(release_id, cancel.clone()).ok_or_else(|| {
                LibraryError::Storage(format!(
                    "release {release_id} already has an active storage transition"
                ))
            })?;
        let rx = self.start_detach_transfer(
            release_id.to_string(),
            new_path.to_path_buf(),
            cancel,
            registration,
        );
        self.drive_transfer(release_id, ReleaseStorageAction::DetachAudio, rx).await
    }

    /// Cancel the in-progress transition for a release, whatever it is. The
    /// UI calls this from the storage row and the queue pane without knowing
    /// which is running — a release is in at most one transition at a time.
    /// A no-op if nothing is in progress.
    ///
    /// Making a release local is stopped by its token, which fires here; a
    /// pin is stopped through the download queue, which also holds the pins
    /// not yet started. Cancelling an upload cancels the release's move to
    /// the cloud, and its files stay on this device only. An unpin can't be
    /// stopped.
    pub async fn cancel_release_transition(&self, release_id: &str) -> Result<(), LibraryError> {
        if self.detach_cancels.cancel(release_id) {
            return Ok(());
        }
        if self.downloads.contains(release_id).await? {
            self.cancel_download(release_id).await?;
        }
        if self.records.has_pending_uploads_for_release(release_id).await? {
            return self.cancel_release_upload(release_id).await;
        }
        Ok(())
    }

    /// Drain a transfer's progress channel until it says how the transfer
    /// ended, handing a pin's progress to the download queue. On `Failed` the
    /// transfer's error is surfaced to the caller. The transfer runs on a
    /// task of its own, which writes and ends the transition its rows show,
    /// so dropping this drain stops neither.
    pub(super) async fn drive_transfer(
        &self,
        release_id: &str,
        action: ReleaseStorageAction,
        mut rx: crate::storage::transfer::TransferProgressFeed,
    ) -> Result<(), LibraryError> {
        use crate::storage::transfer::TransferProgress;

        let outcome = loop {
            let Some(progress) = rx.recv().await else {
                break Err(LibraryError::Storage(format!(
                    "{} ended without completion or failure",
                    verb(action)
                )));
            };
            match progress {
                TransferProgress::Progress { progress } => {
                    if matches!(action, ReleaseStorageAction::Pin)
                        && !self.downloads.set_active_progress(release_id, progress)
                    {
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
