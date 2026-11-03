//! Release file transitions and the download queue.

use super::*;

impl LibraryManager {
    /// Move every file of a release on this device only to the cloud, as one
    /// move; `pin` keeps each file whole on this device too once stored. The
    /// release shows the move as a transition from the write that admits it
    /// until the write that queues its files, which ends it; a failure ends
    /// it in a write of its own. Refused while the release is in another.
    pub async fn upload_release(&self, release_id: &str, pin: bool) -> Result<(), LibraryError> {
        let manager = self.clone();
        let release_id = release_id.to_string();
        // The admission runs to its end on a task of its own, so a dismissed
        // view leaves neither the move half made nor its transition behind;
        // coven keeps the files here once stored when `pin` asks it to.
        self.run_file_operation("attach release", move |_closing| async move {
            manager
                .run_storage_action(&release_id, bae_transfers::StoredTransition::Upload, async {
                    Ok(bae_transfers::release_storage_action::attach_release_files(
                        &manager.database,
                        &release_id,
                        pin,
                    )
                    .await?)
                })
                .await
        })
        .await
    }

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
            .unwrap_or_else(|failed| Err(self.bug(failed).into()));
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
            &self.database,
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
            &self.database,
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
        cancel: &crate::library::CancellationToken,
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
                            let call = manager.database.prepare_device_only(
                                &folder,
                                files,
                                &stopped,
                                |progress| report.progress(progress),
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
                                &manager.database,
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
    /// [`bae_db::stored_name`]): every name is one name inside the
    /// person's folder. A release carrying a name that names no file makes
    /// no file local.
    async fn local_copy_paths(
        &self,
        release_id: &str,
    ) -> Result<Vec<(coven::FileId, coven::RelativePath)>, LibraryError> {
        let files = self.database.get_files_for_release(release_id).await?;
        files
            .iter()
            .map(|f| {
                let path = bae_db::stored_name::file_path(release_id, &f.id, &f.original_filename)?;
                Ok((f.file, path))
            })
            .collect()
    }

    // ── Download (pin) queue ─────────────────────────────────────────
    //
    // Pinning routes through a serial queue rather than an ephemeral
    // per-release task: one release downloads at a time, the rest wait, and the
    // user can pause/cancel/retry. The queue is the `download_queue` table, which
    // the library empties as it opens: after a restart it is empty, and any
    // release that wasn't fully pinned stays cloud-only, since a pin flips a
    // release to pinned only once every file lands.

    /// Enqueue releases to pin for offline, in one write that passes over ids
    /// already waiting or downloading, missing from the library (deleted
    /// meanwhile), local, or already pinned, and queues one whose download
    /// failed again in its place. A write that fails queues nothing
    /// and is reported as a bug: the library's own rows failed to read. The
    /// entry names the release by id only; the Downloads pane reads its title,
    /// file count, and size from the library as it shows the row. Wakes the
    /// parked worker.
    pub async fn enqueue_pins(&self, release_ids: Vec<String>) -> Result<(), LibraryError> {
        let enqueued_at =
            chrono::DateTime::<chrono::Utc>::from(self.clock.now()).timestamp_millis();
        match self.downloads.enqueue_pinnable(release_ids, enqueued_at).await {
            Ok(true) => self.downloads.queued(),
            Ok(false) => debug!("enqueue_pins: nothing to pin"),
            Err(error) => {
                self.diagnostics
                    .report_bug("queue releases to pin", &bae_util::error_text::CauseChain(&error));
                return Err(error.into());
            }
        }
        Ok(())
    }

    /// End a release's failed move to the cloud, `id`, as its pending
    /// record offers: coven retires the move, and the release stays on this
    /// device only, playing from its originals.
    pub async fn cancel_failed_release_move(&self, id: coven::MoveId) -> Result<(), LibraryError> {
        Ok(self.database.cancel_failed_release_move(id).await?)
    }

    /// Pause or resume the download queue. While paused the worker parks instead
    /// of starting the next release; the in-flight one runs to completion.
    /// Resuming wakes the worker.
    pub async fn set_downloads_paused(&self, paused: bool) -> Result<(), LibraryError> {
        Ok(self.downloads.set_paused(paused).await?)
    }

    /// Cancel a release's download. Drops a queued or failed entry; cancelling
    /// an active pin signals its worker to restore the previous pin state.
    pub async fn cancel_download(&self, release_id: &str) -> Result<(), LibraryError> {
        // An active entry needs no follow-up here: the aborted pin task closes its
        // progress channel, and the worker's drain clears the streamed transfer
        // action, finds the entry gone, leaves the queue alone, and re-parks.
        Ok(self.downloads.cancel(release_id).await?)
    }

    /// Flip every failed download back to queued and wake the worker to retry
    /// them.
    pub async fn retry_downloads(&self) -> Result<(), LibraryError> {
        Ok(self.downloads.retry_failed().await?)
    }

    /// The serial download worker: drains the pin queue one release at a time
    /// through [`QueuedReleases::run_serial`], which owns the queue protocol
    /// (the activate/cancel race, cancel-is-not-a-failure, remove vs
    /// mark-failed).
    ///
    /// [`QueuedReleases::run_serial`]: bae_transfers::QueuedReleases::run_serial
    ///
    /// All this supplies is how a pin runs: resolve the release's byte totals,
    /// ask `TransferService` to compose the pin task with this worker's progress
    /// drain, and yield the resulting operation. The
    /// outcome comes from draining the transfer's progress channel — that drain
    /// also publishes the action the storage row reads — and then joining the pin
    /// task, so a panicked pin reports the panic rather than "the channel closed".
    ///
    /// Success needs no follow-up here: `pin_release_files_with_progress`
    /// committed the new state, so the release subscription updates its
    /// `pinned` flag.
    /// Completion and failure reach diagnostics from inside the transfer
    /// (`StorageTransferCompleted` / `StorageTransferFailed`), so there is nothing
    /// to report on the way out.
    pub(super) async fn run_download_worker(&self) {
        use crate::storage::transfer::TransferService;

        self.downloads
            .run_serial(
                "Pin",
                |op| async move {
                    let release_id = op.release_id;
                    let initial_progress = self.initial_download_progress(&release_id).await?;
                    let transfer = TransferService::new(self.clone());
                    let drive_release_id = release_id.clone();
                    let running = transfer.pin_release(release_id, move |progress| async move {
                        self.drive_transfer(&drive_release_id, ReleaseStorageAction::Pin, progress)
                            .await
                    });
                    Ok((initial_progress, running))
                },
                |_release_id, _result| {},
                |label, error| self.report_queue_store_failure(label, error),
                |failed| self.bug(failed),
            )
            .await
    }

    /// Unpin a release: coven moves its files out of the pinned cache and into the
    /// evictable cache, so they stay readable but become droppable.
    pub async fn unpin_release(&self, release_id: &str) -> Result<(), LibraryError> {
        let transfer_service = crate::storage::transfer::TransferService::new(self.clone());
        let rx = transfer_service.unpin_release(release_id.to_string());
        self.drive_transfer(release_id, ReleaseStorageAction::Unpin, rx).await
    }

    /// Move Local releases to Cloud. Each release is admitted independently so
    /// coven can start draining one release while later releases are still being
    /// checked and admitted. Empty and duplicate selections are invalid commands;
    /// release-specific refusals are returned beside the durable receipt for the
    /// releases that were admitted. The stored import storage choice says
    /// whether coven keeps the files whole here once stored (offline) or only
    /// in the evictable cache. Once the move finishes, coven no longer reads
    /// the user's original; the source file itself remains untouched.
    pub async fn upload_releases(
        &self,
        release_ids: &[String],
    ) -> Result<UploadReleasesOutcome, LibraryError> {
        if release_ids.is_empty() {
            return Err(LibraryError::Validation("upload batch has no releases".to_string()));
        }

        let mut selected = std::collections::HashSet::with_capacity(release_ids.len());
        for release_id in release_ids {
            if !selected.insert(release_id) {
                return Err(LibraryError::Validation(format!(
                    "release {release_id} appears more than once in the storage batch"
                )));
            }
        }

        let pin = self.preferences().await?.import_storage.pinned;
        let mut admitted_release_ids = Vec::with_capacity(release_ids.len());
        let mut failures = Vec::new();
        for release_id in release_ids {
            // This release's transfer, from its precondition check to whichever
            // terminal state it reaches. coven's outbox reports its progress, so
            // there is no channel to announce a start on.
            let transfer = crate::storage::transfer::ReleaseTransfer::new(
                self.clone(),
                release_id.clone(),
                ReleaseStorageAction::Upload,
            );
            let admission = async {
                let file_count = transfer.start().await?;
                self.upload_release(release_id, pin).await?;
                Ok::<_, LibraryError>(file_count)
            }
            .await;

            match admission {
                Ok(file_count) => {
                    transfer.complete(file_count);
                    admitted_release_ids.push(release_id.clone());
                }
                Err(error) => {
                    transfer.fail(&error);
                    failures.push((release_id.clone(), error));
                }
            }
        }

        let receipt = (!admitted_release_ids.is_empty())
            .then_some(UploadReceipt { release_ids: admitted_release_ids });
        if failures.is_empty() {
            return Ok(UploadReleasesOutcome::Complete {
                receipt: receipt.expect("a non-empty successful command has a receipt"),
            });
        }
        Ok(UploadReleasesOutcome::Partial {
            receipt,
            failure: UploadBatchFailure::from_errors(failures),
        })
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
        let cancel = crate::library::CancellationToken::new();
        let registration =
            self.detach_cancels.register(release_id, cancel.clone()).ok_or_else(|| {
                LibraryError::Storage(format!(
                    "release {release_id} already has an active storage transition"
                ))
            })?;
        let transfer_service = crate::storage::transfer::TransferService::new(self.clone());
        let rx = transfer_service.detach_release_audio(
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
        if self.database.has_pending_uploads_for_release(release_id).await? {
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
        mut rx: tokio::sync::mpsc::UnboundedReceiver<crate::storage::transfer::TransferProgress>,
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

impl UploadBatchFailure {
    fn from_errors(failures: Vec<(String, LibraryError)>) -> Self {
        let mut failures = failures.into_iter();
        let (first_release_id, first_error) =
            failures.next().expect("a partial upload outcome has at least one failure");
        let first_category = first_error.category();
        let mut same_category = true;
        let mut release_ids = vec![first_release_id.clone()];
        let mut details = vec![format!("release {first_release_id}: {first_error}")];
        for (release_id, error) in failures {
            same_category &= error.category() == first_category;
            details.push(format!("release {release_id}: {error}"));
            release_ids.push(release_id);
        }
        Self {
            release_ids,
            error: bae_diagnostics::UiError::Diagnostic {
                category: if same_category {
                    first_category
                } else {
                    bae_diagnostics::UiErrorCategory::Internal
                },
                detail: details.join("\n"),
            },
        }
    }
}
