//! coven-owned locality transitions (make-Remote / make-Local / cancel) and
//! membership, plus the download/pin queue and transfers.

use super::*;

impl LibraryManager {
    /// Make a release Remote (Local → Remote) through coven: coven enqueues an
    /// upload per user-provided blob from its external file, uploads each, and on
    /// the last flips the `remote` gate true, drops coven's external references,
    /// and re-emits the subtree. The user's source files remain untouched.
    /// Returns once enqueued; durable make-Remote state reports completion.
    pub async fn coven_make_remote(
        &self,
        release_id: &str,
        pin: bool,
    ) -> Result<u64, LibraryError> {
        self.database
            .make_remote("releases", release_id, pin)
            .await
            .map_err(|e| LibraryError::Storage(format!("make release {release_id} remote: {e}")))?;
        // Publish the same canonical projection the durable live query emits
        // before the initiating command can clear its foreground label or emit
        // RemoteUploadQueued. This gives both entrypoints one continuous state
        // path independently of live-query delivery timing.
        Ok(self.emit_outbox_changed().await)
    }

    /// Cancel an in-flight make-Remote of `release_id` through coven: clears the
    /// intent and pending uploads and tombstones any blob already in the cloud.
    /// The gate never flips, so the release stays Local.
    pub(crate) async fn coven_cancel_make_remote(
        &self,
        release_id: &str,
    ) -> Result<(), LibraryError> {
        self.database
            .cancel_make_remote("releases", release_id)
            .await
            .map_err(|e| {
                LibraryError::Storage(format!("cancel make release {release_id} remote: {e}"))
            })?;
        // `cancel_make_remote` records the intent to unwind; the drain carries it
        // out — dropping the still-queued uploads and deleting whatever already
        // landed. The operation has not completed until that drain succeeds.
        self.database.drain_uploads().await.map_err(|error| {
            LibraryError::Storage(format!(
                "finish cancelling make release {release_id} remote: {error}"
            ))
        })?;
        if self
            .database
            .make_remote_progress_for_release(release_id)
            .await?
            .is_some()
        {
            return Err(LibraryError::Storage(format!(
                "cancel make release {release_id} remote did not finish"
            )));
        }
        Ok(())
    }

    /// Make a release Local (Remote → Local) through coven: coven materializes each
    /// blob back to a local file durability-first — every release file (a
    /// user-provided blob) to `new_path/{original_filename}`, the host-provided
    /// cover to coven's local store (no dest) — then flips the `remote` gate false,
    /// registers the external refs, and enqueues the cloud deletes in one atomic
    /// commit. `cancel` aborts before the commit (the release stays Remote).
    pub async fn coven_make_local(
        &self,
        release_id: &str,
        new_path: &str,
        cancel: &crate::library::CancellationToken,
    ) -> Result<(), LibraryError> {
        let dest = self.make_local_dest(release_id, new_path).await?;
        let (cancel_rx, bridge) =
            crate::library::cancel_token_to_watch(&self.runtime_handle, cancel.clone());
        let result = self
            .database
            .make_local("releases", release_id, &dest, &cancel_rx)
            .await;
        bridge.abort();
        Self::map_make_local_result(release_id, result)
    }

    /// The make-Local destination map: each release file's blob id → the user path
    /// (`new_path/original_filename`) its bytes go back to. Host-provided blobs (the
    /// cover) take no dest — coven restores them to its local store.
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

    /// Map a coven `make_local` result to bae's: a cancel before the commit is a
    /// successful return after coven rolled back the partial copies and left the
    /// release Remote; every other error is surfaced.
    fn map_make_local_result(
        release_id: &str,
        result: Result<(), coven::MakeLocalError>,
    ) -> Result<(), LibraryError> {
        match result {
            Ok(()) => Ok(()),
            Err(coven::MakeLocalError::Cancelled) => {
                debug!(
                    release_id,
                    "make-local cancelled before commit; release stays Remote"
                );
                Ok(())
            }
            Err(e) => Err(LibraryError::Storage(format!(
                "make release {release_id} local: {e}"
            ))),
        }
    }

    /// Mint a restore code, seeded with the store's current membership-head
    /// floor read from the cloud — a network round trip, hence async (coven's
    /// own `CovenHandle::generate_restore_code` is the same way).
    pub async fn generate_restore_code(&self) -> Result<String, LibraryError> {
        Ok(self.database.generate_restore_code().await?)
    }

    /// The library's membership: its devices (with this device flagged, each
    /// member's fingerprint, and whether it can be removed) and whether the
    /// running device is an owner.
    pub async fn get_members(&self) -> Result<crate::sync::membership::Membership, LibraryError> {
        self.sync.get_members().await
    }

    pub async fn start_device_pairing(
        &self,
    ) -> Result<crate::library::DevicePairingSession, LibraryError> {
        self.sync.start_device_pairing().await
    }

    /// Remove a device from the library and rotate the library key so the removed
    /// device can no longer read new data. Records the rotated key's fingerprint
    /// in this device's config.
    pub async fn remove_member(&self, public_key_hex: &str) -> Result<(), LibraryError> {
        self.sync.remove_member(public_key_hex).await
    }

    // ── Download (pin) queue ─────────────────────────────────────────
    //
    // Pinning routes through an in-memory serial queue rather than an ephemeral
    // per-release task: one release downloads at a time, the rest wait, and the
    // user can pause/cancel/retry. The queue is transient — empty after a restart,
    // and any release that wasn't fully pinned stays cloud-only, since a pin flips
    // a release to pinned only once every file lands.

    /// Enqueue releases to pin for offline. Skips ids already in the queue (in any
    /// state) or already pinned; for each new one, resolves its title / file_count /
    /// total_size from its storage summary so the Downloads pane can render the row
    /// without a re-query. Wakes the parked worker and emits a fresh
    /// `DownloadQueueChanged`.
    pub async fn enqueue_pins(&self, release_ids: Vec<String>) {
        // One timestamp for the whole batch — read the clock once, not per row.
        let enqueued_at = self.clock.now().timestamp_millis();
        let mut added_any = false;
        for release_id in release_ids {
            if self.download_queue.contains(&release_id) {
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

            let op = crate::library::DownloadOp {
                release_id: release_id.clone(),
                title: summary.album_title,
                file_count: summary.file_count,
                total_size: summary.total_size,
                created_at: enqueued_at,
                payload: (),
                state: crate::library::DownloadState::Queued,
            };
            if self.download_queue.enqueue(op) {
                added_any = true;
            }
        }

        if added_any {
            self.download_queue.wake();
            self.emit_download_queue_changed();
        }
    }

    /// Pause or resume the download queue. While paused the worker parks instead
    /// of starting the next release; the in-flight one runs to completion.
    /// Resuming wakes the worker. Emits a fresh `DownloadQueueChanged`.
    pub fn set_downloads_paused(&self, paused: bool) {
        let was_paused = self.download_queue.set_paused(paused);
        if was_paused && !paused {
            // Resume: wake the parked worker so it picks up the next release
            // immediately rather than waiting for the next enqueue.
            self.download_queue.wake();
        }
        self.emit_download_queue_changed();
    }

    /// Cancel a release's download. Drops a queued/failed entry; for the active
    /// one, aborts its in-flight pin task (the `.part` temp file it was writing
    /// is left behind — a partial never renames into place, so the release stays
    /// cloud-only). Emits a fresh snapshot.
    pub fn cancel_download(&self, release_id: &str) {
        // An active entry needs no follow-up here: the aborted pin task closes its
        // progress channel, and the worker's drain clears the streamed transfer
        // action, sees the entry is gone, leaves the queue alone, and re-parks.
        self.download_queue.cancel(release_id);
        self.emit_download_queue_changed();
    }

    /// Flip every failed download back to queued and wake the worker to retry
    /// them. Emits a fresh `DownloadQueueChanged`.
    pub fn retry_downloads(&self) {
        if self.download_queue.retry_failed() {
            self.download_queue.wake();
        }
        self.emit_download_queue_changed();
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
    /// Success needs no follow-up here: `pin_release_blobs` committed the new
    /// state, so the release subscription updates its `pinned` flag.
    /// Completion and failure reach diagnostics from inside the transfer
    /// (`StorageTransferCompleted` / `StorageTransferFailed`), so there is nothing
    /// to report on the way out.
    pub(super) async fn run_download_worker(&self) {
        use crate::library::release_queue::run_serial_worker;
        use crate::storage::transfer::TransferService;

        run_serial_worker(
            &self.download_queue,
            "Pin",
            |op| async move {
                let release_id = op.release_id;
                let initial_progress = self.initial_download_progress(&release_id).await?;
                let transfer = TransferService::new(self.clone());
                let drive_release_id = release_id.clone();
                let running = transfer.pin_release(release_id, move |progress| async move {
                    self.drive_transfer(&drive_release_id, ReleaseStorageAction::Pin, progress)
                        .await
                        .map(|_| ())
                });
                Ok((initial_progress, running))
            },
            || self.emit_download_queue_changed(),
            |_release_id, _result| {},
        )
        .await
    }

    /// Unpin a release: coven moves its blobs out of `storage/pinned/` and into the
    /// evictable cache, so they stay readable but become droppable.
    pub async fn unpin_release(&self, release_id: &str) -> Result<(), LibraryError> {
        let transfer_service = crate::storage::transfer::TransferService::new(self.clone());
        let rx = transfer_service.unpin_release(release_id.to_string());
        let outcome = self
            .drive_transfer(release_id, ReleaseStorageAction::Unpin, rx)
            .await?;
        assert_eq!(outcome, crate::storage::transfer::TransferOutcome::Complete);
        Ok(())
    }

    /// Move a Local release to Cloud: upload its files to the cloud home. `pin`
    /// chooses whether coven keeps the blobs in `storage/pinned/` (offline) vs the
    /// evictable cache. Once the upload lands, the remote release no longer refers
    /// to the user's source path; the source file itself remains untouched.
    pub async fn make_release_remote(
        &self,
        release_id: &str,
        pin: bool,
    ) -> Result<u64, LibraryError> {
        let transfer_service = crate::storage::transfer::TransferService::new(self.clone());
        let rx = transfer_service.make_release_remote(release_id.to_string(), pin);
        match self
            .drive_transfer(release_id, ReleaseStorageAction::MakeRemote, rx)
            .await?
        {
            crate::storage::transfer::TransferOutcome::CloudUploadQueued { outbox_revision } => {
                Ok(outbox_revision)
            }
            crate::storage::transfer::TransferOutcome::Complete => {
                panic!("make-Remote completed without its durable outbox revision")
            }
        }
    }

    /// Make a Cloud release Local: copy its files back out to `new_path` and
    /// drop the remote copies. coven owns the durability-first ordering: every
    /// copy is verified at the new path before any delete is queued.
    pub async fn make_release_local(
        &self,
        release_id: &str,
        new_path: &str,
    ) -> Result<(), LibraryError> {
        // Register a cancellation token so `cancel_release_transition` can stop
        // this transfer; the guard deregisters even if this future is dropped.
        let cancel = crate::library::CancellationToken::new();
        self.transfer_cancels
            .lock()
            .unwrap()
            .insert(release_id.to_string(), cancel.clone());
        let _dereg = TransferCancelGuard {
            registry: self.transfer_cancels.clone(),
            release_id: release_id.to_string(),
        };

        let transfer_service = crate::storage::transfer::TransferService::new(self.clone());
        let rx = transfer_service.make_release_local(
            release_id.to_string(),
            new_path.to_string(),
            cancel,
        );
        let outcome = self
            .drive_transfer(release_id, ReleaseStorageAction::MakeLocal, rx)
            .await?;
        assert_eq!(outcome, crate::storage::transfer::TransferOutcome::Complete);
        Ok(())
    }

    /// Cancel the in-progress transition for a release, whatever it is: a pin
    /// (download), a remote upload, or a make-Local transfer. The UI calls this from the
    /// storage row and the queue pane without knowing which is running — a
    /// release is in at most one transition at a time. A no-op if nothing is in
    /// progress. Each branch is gated on the transition actually running:
    /// `cancel_release_upload` on a settled release would delete its blobs, so it
    /// fires only when uploads are genuinely pending.
    pub async fn cancel_release_transition(&self, release_id: &str) -> Result<(), LibraryError> {
        if self.cancel_transfer(release_id) {
            return Ok(());
        }
        if self.download_queue.contains(release_id) {
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

    /// Fire the cancellation token for a release's in-progress foreground
    /// make-Local transfer, if one is registered; returns whether it fired. The
    /// transfer observes the token between files, deletes its partial copies, and
    /// leaves the release remote. A missing token is not an error — it means no
    /// transfer is running, so the caller falls through to the other transition
    /// kinds. The lookup and fire share one lock, so there's no check-then-act
    /// race with the deregistering drop guard.
    fn cancel_transfer(&self, release_id: &str) -> bool {
        match self.transfer_cancels.lock().unwrap().get(release_id) {
            Some(token) => {
                token.cancel();
                true
            }
            None => false,
        }
    }

    /// Drain a transfer's progress channel and publish its active action through
    /// the transfer value stream. On `Failed` the transfer channel's error string
    /// is wrapped as a `Storage` failure and surfaced to the caller.
    pub(super) async fn drive_transfer(
        &self,
        release_id: &str,
        action: ReleaseStorageAction,
        mut rx: tokio::sync::mpsc::UnboundedReceiver<crate::storage::transfer::TransferProgress>,
    ) -> Result<crate::storage::transfer::TransferOutcome, LibraryError> {
        use crate::storage::transfer::TransferProgress;

        // The bridge transfer future is abortable. The guard clears the streamed
        // action whether the channel completes or the future is dropped.
        let _value_guard = TransferValueGuard {
            transfer_actions: self.transfer_actions.clone(),
            transfer_values: self.transfer_values.clone(),
            release_id: release_id.to_string(),
        };

        let outcome = loop {
            let Some(progress) = rx.recv().await else {
                break Err(LibraryError::Storage(format!(
                    "{} ended without completion or failure",
                    verb(action)
                )));
            };
            match progress {
                TransferProgress::Started => {
                    let actions = {
                        let mut actions = self.transfer_actions.lock().unwrap();
                        actions.insert(release_id.to_string(), action);
                        actions.clone()
                    };
                    self.transfer_values.send_replace(actions);
                }
                TransferProgress::Progress { progress } => {
                    if matches!(action, ReleaseStorageAction::Pin) {
                        if self
                            .download_queue
                            .set_active_progress(release_id, progress)
                        {
                            self.emit_download_queue_changed();
                        } else {
                            tracing::warn!(
                                release_id,
                                "ignored download progress for missing active queue row"
                            );
                        }
                    }
                }
                TransferProgress::Complete { outcome, .. } => break Ok(outcome),
                TransferProgress::Failed { error, .. } => break Err(LibraryError::Storage(error)),
            }
        };

        outcome
    }
}
