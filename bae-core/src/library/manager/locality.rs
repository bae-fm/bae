//! Release file transitions and the download queue.

use super::*;

impl LibraryManager {
    /// Attach every local file in one write, then keep the requested offline copies.
    pub async fn upload_release(&self, release_id: &str, pin: bool) -> Result<(), LibraryError> {
        let manager = self.clone();
        let release_id = release_id.to_string();
        let guard = self.admit_transfer_values(
            std::slice::from_ref(&release_id),
            ReleaseStorageAction::Upload,
        )?;
        // A committed attachment survives a dismissed view. Its optional offline
        // copy has its own Downloads outcome and retry, never a second attachment.
        self.run_file_operation("attach release", move |_closing| async move {
            manager.database.attach_release_files(&release_id).await?;
            drop(guard);
            if pin {
                manager.enqueue_attached_release_pin(release_id).await?;
            }
            Ok(())
        })
        .await
    }

    /// Copy and verify the files before atomically returning the release to local paths.
    pub async fn detach_release_audio_into(
        &self,
        release_id: &str,
        new_path: &str,
        cancel: &crate::library::CancellationToken,
    ) -> Result<(), LibraryError> {
        let destinations = self
            .detached_audio_destinations(release_id, new_path)
            .await?;
        let operation = cancel.child_token();
        let cancel_on_drop = operation.clone().drop_guard();
        let manager = self.clone();
        let release_id = release_id.to_string();
        let root = PathBuf::from(new_path);
        let result = self
            .run_file_operation("make release local", move |closing| async move {
                let copy =
                    manager.copy_release_to_local(&release_id, root, destinations, &operation);
                tokio::pin!(copy);
                tokio::select! {
                    result = &mut copy => result,
                    _ = closing.cancelled() => {
                        operation.cancel();
                        copy.await
                    }
                }
            })
            .await;
        cancel_on_drop.disarm();
        result
    }

    /// Each release file's destination. Covers remain attached and shared.
    ///
    /// `original_filename` is a synced column another device wrote, and copying writes
    /// wherever this map points, so each fragment is validated before it is joined
    /// onto the user's folder: a release carrying one bad name copies out no files
    /// at all, rather than one of them landing outside the folder the user chose.
    pub(super) async fn detached_audio_destinations(
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
    // Pinning routes through a serial queue rather than an ephemeral
    // per-release task: one release downloads at a time, the rest wait, and the
    // user can pause/cancel/retry. The queue is the `download_queue` table, which
    // the library empties as it opens: after a restart it is empty, and any
    // release that wasn't fully pinned stays cloud-only, since a pin flips a
    // release to pinned only once every file lands.

    /// Attachment has committed; retrying this queue item only keeps its bytes offline.
    pub(super) async fn enqueue_attached_release_pin(
        &self,
        release_id: String,
    ) -> Result<(), LibraryError> {
        self.downloads
            .enqueue_all(vec![crate::library::DownloadEntry {
                release_id,
                created_at: chrono::DateTime::<chrono::Utc>::from(self.clock.now())
                    .timestamp_millis(),
                payload: (),
            }])
            .await?;
        Ok(())
    }

    /// Enqueue releases to pin for offline. Skips ids already in the queue (in any
    /// state), missing from the library (deleted meanwhile), local, or already
    /// pinned. A release whose storage can't be read fails the whole request
    /// before anything is queued, reported as a bug: the library's own rows
    /// failed to read. The entry names the release by id only; the Downloads
    /// pane reads its title, file count, and size from the library as it shows
    /// the row. Wakes the parked worker.
    pub async fn enqueue_pins(&self, release_ids: Vec<String>) -> Result<(), LibraryError> {
        // One timestamp for the whole batch — read the clock once, not per row.
        let enqueued_at =
            chrono::DateTime::<chrono::Utc>::from(self.clock.now()).timestamp_millis();
        let mut entries = Vec::with_capacity(release_ids.len());
        for release_id in release_ids {
            if self.downloads.contains(&release_id).await? {
                debug!("enqueue_pins: {release_id} already queued, skipping");
                continue;
            }
            let summary = match self.find_release_storage_summary(&release_id).await {
                Ok(Some(summary)) => summary,
                Ok(None) => {
                    debug!("enqueue_pins: release {release_id} is gone from the library, skipping");
                    continue;
                }
                Err(error) => {
                    self.diagnostics.report_bug(
                        &format!("read release {release_id}'s storage to pin it"),
                        &bae_util::error_text::CauseChain(&error),
                    );
                    return Err(error);
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

            entries.push(crate::library::DownloadEntry {
                release_id: release_id.clone(),
                created_at: enqueued_at,
                payload: (),
            });
        }
        self.downloads.enqueue_all(entries).await?;
        Ok(())
    }

    /// Pause or resume the download queue. While paused the worker parks instead
    /// of starting the next release; the in-flight one runs to completion.
    /// Resuming wakes the worker.
    pub async fn set_downloads_paused(&self, paused: bool) -> Result<(), LibraryError> {
        self.downloads.set_paused(paused).await
    }

    /// Cancel a release's download. Drops a queued or failed entry; cancelling
    /// an active pin signals its worker to restore the previous pin state.
    pub async fn cancel_download(&self, release_id: &str) -> Result<(), LibraryError> {
        // An active entry needs no follow-up here: the aborted pin task closes its
        // progress channel, and the worker's drain clears the streamed transfer
        // action, finds the entry gone, leaves the queue alone, and re-parks.
        self.downloads.cancel(release_id).await
    }

    /// Flip every failed download back to queued and wake the worker to retry
    /// them.
    pub async fn retry_downloads(&self) -> Result<(), LibraryError> {
        self.downloads.retry_failed().await
    }

    /// The serial download worker: drains the pin queue one release at a time
    /// through [`QueuedReleases::run_serial`], which owns the queue protocol
    /// (the activate/cancel race, cancel-is-not-a-failure, remove vs
    /// mark-failed).
    ///
    /// [`QueuedReleases::run_serial`]: crate::library::QueuedReleases::run_serial
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
                |label, error| self.report_queue_store_failure(label, error),
            )
            .await
    }

    /// Unpin a release: coven moves its files out of the pinned cache and into the
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
    /// whether coven keeps the files in the pinned cache (offline) or the
    /// evictable cache. Once the upload lands, a remote release no longer
    /// refers to the user's source path; the source file itself remains
    /// untouched.
    pub async fn upload_releases(
        &self,
        release_ids: &[String],
    ) -> Result<UploadReleasesOutcome, LibraryError> {
        if release_ids.is_empty() {
            return Err(LibraryError::Validation(
                "upload batch has no releases".to_string(),
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

        let receipt = (!admitted_release_ids.is_empty()).then_some(UploadReceipt {
            release_ids: admitted_release_ids,
        });
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

    /// Make an attached release Local: verify every file at `new_path` before
    /// one write clears its attachments and records this device's paths.
    pub async fn detach_release_audio(
        &self,
        release_id: &str,
        new_path: &str,
    ) -> Result<(), LibraryError> {
        // The token `cancel_release_transition` fires to stop this transfer
        // between files; registered with the drive so it is reachable for
        // exactly as long as the transfer runs.
        let cancel = crate::library::CancellationToken::new();
        let transfer_service = crate::storage::transfer::TransferService::new(self.clone());
        let rx = transfer_service.detach_release_audio(
            release_id.to_string(),
            new_path.to_string(),
            cancel.clone(),
        );
        self.drive_transfer(
            release_id,
            ReleaseStorageAction::DetachAudio,
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
    /// audio detachment transfer is stopped by its token, which the registry fires;
    /// a pin is stopped through the download queue, which also holds the pins
    /// not yet started. Cancelling an upload detaches every audio file and
    /// restores its original path in the same write.
    pub async fn cancel_release_transition(&self, release_id: &str) -> Result<(), LibraryError> {
        match self.transitions.cancel(release_id) {
            Some(ReleaseStorageAction::DetachAudio) => return Ok(()),
            Some(ReleaseStorageAction::Pin) => {
                self.cancel_download(release_id).await?;
            }
            Some(ReleaseStorageAction::Upload | ReleaseStorageAction::Unpin) | None => {}
        }
        if self.downloads.contains(release_id).await? {
            self.cancel_download(release_id).await?;
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

impl UploadBatchFailure {
    fn from_errors(failures: Vec<(String, LibraryError)>) -> Self {
        let mut failures = failures.into_iter();
        let (first_release_id, first_error) = failures
            .next()
            .expect("a partial upload outcome has at least one failure");
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
