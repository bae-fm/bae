//! Release file transitions and the download queue.

use super::*;

impl LibraryManager {
    /// Attach every local file in one write, then keep the requested offline copies.
    pub async fn coven_make_remote(
        &self,
        release_id: &str,
        pin: bool,
    ) -> Result<u64, LibraryError> {
        let manager = self.clone();
        let release_id = release_id.to_string();
        // Finish attachment and pinning even if the caller leaves; committed uploads cannot be recalled.
        let task = self.runtime_handle.spawn(async move {
            manager.database.attach_release_files(&release_id).await?;
            if pin {
                let result = manager.pin_release_blobs_with_progress(&release_id, |_| {}).await;
                if let Err(error) = result {
                    manager.emit_outbox_changed().await;
                    return Err(LibraryError::Storage(format!(
                        "release {release_id} was attached for upload, but pinning failed: {error}; retry pinning the release"
                    )));
                }
            }
            Ok(manager.emit_outbox_changed().await)
        });
        match task.await {
            Ok(result) => result,
            Err(error) => Err(LibraryError::Storage(format!(
                "attaching release files failed: {error}"
            ))),
        }
    }

    /// Copy and verify the files before atomically returning the release to local paths.
    pub async fn coven_make_local(
        &self,
        release_id: &str,
        new_path: &str,
        cancel: &crate::library::CancellationToken,
    ) -> Result<(), LibraryError> {
        let destinations = self.make_local_dest(release_id, new_path).await?;
        let operation = cancel.child_token();
        let cancel_on_drop = operation.clone().drop_guard();
        let manager = self.clone();
        let release_id = release_id.to_string();
        let root = PathBuf::from(new_path);
        let task = self.runtime_handle.spawn(async move {
            manager
                .copy_release_to_local(&release_id, root, destinations, &operation)
                .await
        });
        let result = match task.await {
            Ok(result) => result,
            Err(error) => Err(LibraryError::Storage(format!(
                "making release local failed: {error}"
            ))),
        };
        cancel_on_drop.disarm();
        result
    }

    /// Each release file's destination. Covers remain attached and shared.
    ///
    /// `original_filename` is a synced column another device wrote, and coven writes
    /// wherever this map points, so each fragment is validated before it is joined
    /// onto the user's folder: a release carrying one bad name copies out no files
    /// at all, rather than one of them landing outside the folder the user chose.
    pub(super) async fn make_local_dest(
        &self,
        release_id: &str,
        new_path: &str,
    ) -> Result<HashMap<String, PathBuf>, LibraryError> {
        let files = self.database.get_files_for_release(release_id).await?;
        let mut dest = HashMap::with_capacity(files.len());
        for f in &files {
            crate::storage::path_fragment::validate_path_fragment(
                release_id,
                &format!("original_filename for file {}", f.id),
                &f.original_filename,
            )?;
            dest.insert(
                f.id.clone(),
                std::path::Path::new(new_path).join(&f.original_filename),
            );
        }
        Ok(dest)
    }

    // ── Download (pin) queue ─────────────────────────────────────────
    //
    // Pinning routes through an in-memory serial queue rather than an ephemeral
    // per-release task: one release downloads at a time, the rest wait, and the
    // user can pause/cancel/retry. The queue is transient — empty after a restart,
    // and any release that wasn't fully pinned stays cloud-only, since a pin flips
    // a release to pinned only once every file lands.

    /// Enqueue releases to pin for offline. Skips ids already in the queue (in any
    /// state), missing from the library, local, or already pinned. The entry
    /// names the release by id only; the Downloads pane reads its title, file
    /// count, and size from the library as it shows the row. Wakes the parked
    /// worker and publishes the queue's new contents.
    pub async fn enqueue_pins(&self, release_ids: Vec<String>) {
        // One timestamp for the whole batch — read the clock once, not per row.
        let enqueued_at =
            chrono::DateTime::<chrono::Utc>::from(self.clock.now()).timestamp_millis();
        let mut ops = Vec::with_capacity(release_ids.len());
        for release_id in release_ids {
            if self.downloads.contains(&release_id) {
                debug!("enqueue_pins: {release_id} already queued, skipping");
                continue;
            }
            let summary = match self.find_release_storage_summary(&release_id).await {
                Ok(Some(summary)) => summary,
                Ok(None) => {
                    warn!("enqueue_pins: release {release_id} not found, skipping");
                    continue;
                }
                Err(e) => {
                    warn!("enqueue_pins: failed to load storage summary for {release_id}: {e}");
                    continue;
                }
            };
            if summary.storage_state == ReleaseStorageState::Local {
                // Pinning keeps a *cloud* release offline; a local release is
                // already on disk, so there is nothing to download. Callers that
                // can't see storage state (the album grid's bulk pin) arrive with a
                // mixed selection, so skip rather than enqueue a doomed download.
                debug!("enqueue_pins: {release_id} is local (nothing to pin), skipping");
                continue;
            }
            if summary.pinned {
                debug!("enqueue_pins: {release_id} already pinned, skipping");
                continue;
            }

            ops.push(crate::library::DownloadOp {
                release_id: release_id.clone(),
                created_at: enqueued_at,
                payload: (),
                state: crate::library::DownloadState::Queued,
            });
        }
        self.downloads.enqueue_all(ops);
    }

    /// Pause or resume the download queue. While paused the worker parks instead
    /// of starting the next release; the in-flight one runs to completion.
    /// Resuming wakes the worker. Emits a fresh snapshot.
    pub fn set_downloads_paused(&self, paused: bool) {
        self.downloads.set_paused(paused);
    }

    /// Cancel a release's download. Drops a queued or failed entry; cancelling
    /// an active pin signals its worker to restore the previous pin state.
    pub fn cancel_download(&self, release_id: &str) {
        // An active entry needs no follow-up here: the aborted pin task closes its
        // progress channel, and the worker's drain clears the streamed transfer
        // action, sees the entry is gone, leaves the queue alone, and re-parks.
        self.downloads.cancel(release_id);
    }

    /// Cancel every download — queued, failed, and the one in flight — as
    /// [`Self::cancel_download`] cancels one. Emits a fresh snapshot.
    pub fn cancel_all_downloads(&self) {
        self.downloads.cancel_all();
    }

    /// Flip every failed download back to queued and wake the worker to retry
    /// them. Emits a fresh snapshot.
    pub fn retry_downloads(&self) {
        self.downloads.retry_failed();
    }

    /// The serial download worker: drains the pin queue one release at a time
    /// through [`run_serial_worker`], which owns the queue protocol (the
    /// activate/cancel race, cancel-is-not-a-failure, remove vs mark-failed).
    ///
    /// All this supplies is how a pin runs: resolve the release's byte totals,
    /// ask `TransferService` to compose the pin task with this worker's progress
    /// drain, and yield the resulting operation. The
    /// outcome comes from draining the transfer's progress channel — that drain
    /// also publishes the action the storage row reads — and then joining the pin
    /// task, so a panicked pin reports the panic rather than "the channel closed".
    ///
    /// Success needs no follow-up here: `pin_release_blobs_with_progress`
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
                        self.drive_transfer(
                            &drive_release_id,
                            ReleaseStorageAction::Pin,
                            progress,
                            None,
                        )
                        .await
                        .map(|_| ())
                    });
                    Ok((initial_progress, running))
                },
                |_release_id, _result| {},
            )
            .await
    }

    /// Unpin a release: coven moves its blobs out of the pinned cache and into the
    /// evictable cache, so they stay readable but become droppable.
    pub async fn unpin_release(&self, release_id: &str) -> Result<(), LibraryError> {
        let transfer_service = crate::storage::transfer::TransferService::new(self.clone());
        let rx = transfer_service.unpin_release(release_id.to_string());
        self.drive_transfer(release_id, ReleaseStorageAction::Unpin, rx, None)
            .await
    }

    /// Move Local releases to Cloud. Each release is admitted independently so
    /// coven can start draining one release while later releases are still being
    /// checked and admitted. Empty and duplicate selections are invalid commands;
    /// release-specific refusals are returned beside the durable receipt for the
    /// releases that were admitted. The stored import storage choice says
    /// whether coven keeps the blobs in the pinned cache (offline) or the
    /// evictable cache. Once the upload lands, a remote release no longer
    /// refers to the user's source path; the source file itself remains
    /// untouched.
    pub async fn make_releases_remote(
        &self,
        release_ids: &[String],
    ) -> Result<MakeReleasesRemoteOutcome, LibraryError> {
        if release_ids.is_empty() {
            return Err(LibraryError::Validation(
                "move-to-cloud batch has no releases".to_string(),
            ));
        }

        let mut selected = std::collections::HashSet::with_capacity(release_ids.len());
        for release_id in release_ids {
            if !selected.insert(release_id) {
                return Err(LibraryError::Validation(format!(
                    "release {release_id} appears more than once in the storage batch"
                )));
            }
        }

        let pin = self.get_config().prefs.import_storage.pinned;
        let mut admitted_release_ids = Vec::with_capacity(release_ids.len());
        let mut last_revision = None;
        let mut failures = Vec::new();
        for release_id in release_ids {
            // This release's transfer, from its precondition check to whichever
            // terminal state it reaches. coven's outbox reports its progress, so
            // there is no channel to announce a start on.
            let transfer = crate::storage::transfer::ReleaseTransfer::new(
                self.clone(),
                release_id.clone(),
                ReleaseStorageAction::MakeRemote,
            );
            let admission = async {
                let file_count = transfer.start().await?;
                let _value_guard = self.admit_transfer_values(
                    std::slice::from_ref(release_id),
                    ReleaseStorageAction::MakeRemote,
                )?;
                let revision = self.coven_make_remote(release_id, pin).await?;
                Ok::<_, LibraryError>((revision, file_count))
            }
            .await;

            match admission {
                Ok((revision, file_count)) => {
                    transfer.complete(file_count);
                    admitted_release_ids.push(release_id.clone());
                    last_revision = Some(revision);
                }
                Err(error) => {
                    transfer.fail(&error);
                    failures.push((release_id.clone(), error));
                }
            }
        }

        let receipt = last_revision.map(|outbox_revision| MakeRemoteReceipt {
            outbox_revision,
            release_ids: admitted_release_ids,
        });
        if failures.is_empty() {
            return Ok(MakeReleasesRemoteOutcome::Complete {
                receipt: receipt.expect("a non-empty successful command has a receipt"),
            });
        }
        Ok(MakeReleasesRemoteOutcome::Partial {
            receipt,
            failure: MakeRemoteBatchFailure::from_errors(failures),
        })
    }

    /// Make an attached release Local: verify every file at `new_path` before
    /// one write clears its attachments and records this device's paths.
    pub async fn make_release_local(
        &self,
        release_id: &str,
        new_path: &str,
    ) -> Result<(), LibraryError> {
        // The token `cancel_release_transition` fires to stop this transfer
        // between files; registered with the drive so it is reachable for
        // exactly as long as the transfer runs.
        let cancel = crate::library::CancellationToken::new();
        let transfer_service = crate::storage::transfer::TransferService::new(self.clone());
        let rx = transfer_service.make_release_local(
            release_id.to_string(),
            new_path.to_string(),
            cancel.clone(),
        );
        self.drive_transfer(
            release_id,
            ReleaseStorageAction::MakeLocal,
            rx,
            Some(cancel),
        )
        .await
    }

    /// Cancel the in-progress transition for a release, whatever it is. The
    /// UI calls this from the storage row and the queue pane without knowing
    /// which is running — a release is in at most one transition at a time.
    /// A no-op if nothing is in progress.
    ///
    /// The transitions this process registered say what they are: a
    /// make-Local transfer is stopped by its token, which the registry fires;
    /// a pin is stopped through the download queue, which also holds the pins
    /// not yet started. Committed uploads cannot be cancelled through coven's
    /// API; a pending upload therefore returns that refusal to the caller.
    pub async fn cancel_release_transition(&self, release_id: &str) -> Result<(), LibraryError> {
        match self.transitions.cancel(release_id) {
            Some(ReleaseStorageAction::MakeLocal) => return Ok(()),
            Some(ReleaseStorageAction::Pin) => {
                self.cancel_download(release_id);
                return Ok(());
            }
            Some(ReleaseStorageAction::MakeRemote | ReleaseStorageAction::Unpin) | None => {}
        }
        if self.downloads.contains(release_id) {
            self.cancel_download(release_id);
            return Ok(());
        }
        if self
            .database
            .has_pending_uploads_for_release(release_id)
            .await?
        {
            return self.cancel_release_upload(release_id).await;
        }
        Ok(())
    }

    /// Admit every foreground transfer action under one lock. Either every
    /// release becomes active in the value stream or none does.
    pub(super) fn admit_transfer_values(
        &self,
        release_ids: &[String],
        action: ReleaseStorageAction,
    ) -> Result<crate::library::storage_transitions::TransitionGuard, LibraryError> {
        self.transitions.admit(release_ids, action)
    }

    /// Drain a transfer's progress channel and publish its active action through
    /// the transfer value stream. On `Failed` the transfer channel's error string
    /// is wrapped as a `Storage` failure and surfaced to the caller.
    pub(super) async fn drive_transfer(
        &self,
        release_id: &str,
        action: ReleaseStorageAction,
        mut rx: tokio::sync::mpsc::UnboundedReceiver<crate::storage::transfer::TransferProgress>,
        cancel: Option<crate::library::CancellationToken>,
    ) -> Result<(), LibraryError> {
        use crate::storage::transfer::TransferProgress;

        // The bridge transfer future is abortable. The guard clears the
        // registration whether the channel completes or the future is dropped.
        let _registration = self.transitions.track(release_id, action, cancel);

        let outcome = loop {
            let Some(progress) = rx.recv().await else {
                break Err(LibraryError::Storage(format!(
                    "{} ended without completion or failure",
                    verb(action)
                )));
            };
            match progress {
                TransferProgress::Started => {
                    self.transitions.started(release_id, action);
                }
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

impl MakeRemoteBatchFailure {
    fn from_errors(failures: Vec<(String, LibraryError)>) -> Self {
        let mut failures = failures.into_iter();
        let (first_release_id, first_error) = failures
            .next()
            .expect("a partial make-Remote outcome has at least one failure");
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
            error: crate::ui::UiError::Diagnostic {
                category: if same_category {
                    first_category
                } else {
                    crate::ui::UiErrorCategory::Internal
                },
                detail: details.join("\n"),
            },
        }
    }
}
