//! coven-owned locality transitions (make-Remote / make-Local / cancel) and
//! membership, plus the download/pin queue and transfers.

use super::*;

impl LibraryManager {
    /// Make a release Remote (Local → Remote) through coven: coven enqueues an
    /// upload per user-provided blob from its external file, uploads each, and on
    /// the last flips the `remote` gate true, drops the external refs, deletes the
    /// source files, and re-emits the subtree (the host-provided cover then rides
    /// along). Returns once enqueued; completion fires `on_root_made_remote`.
    pub async fn coven_make_remote(&self, release_id: &str, pin: bool) -> Result<(), LibraryError> {
        self.handle
            .make_remote("releases", release_id, pin)
            .await
            .map_err(|e| LibraryError::Storage(format!("make release {release_id} remote: {e}")))
    }

    /// Cancel an in-flight make-Remote of `release_id` through coven: clears the
    /// intent and pending uploads and tombstones any blob already in the cloud.
    /// The gate never flips, so the release stays Local.
    pub(crate) async fn coven_cancel_make_remote(
        &self,
        release_id: &str,
    ) -> Result<(), LibraryError> {
        self.handle
            .cancel_make_remote("releases", release_id)
            .await
            .map_err(|e| {
                LibraryError::Storage(format!("cancel make release {release_id} remote: {e}"))
            })
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
            .handle
            .make_local("releases", release_id, &dest, &cancel_rx)
            .await;
        bridge.abort();
        Self::map_make_local_result(release_id, result)
    }

    /// The make-Local destination map: each release file's blob id → the user path
    /// (`new_path/original_filename`) its bytes go back to. Host-provided blobs (the
    /// cover) take no dest — coven restores them to its local store.
    async fn make_local_dest(
        &self,
        release_id: &str,
        new_path: &str,
    ) -> Result<HashMap<String, PathBuf>, LibraryError> {
        let files = self.database.get_files_for_release(release_id).await?;
        Ok(files
            .iter()
            .map(|f| {
                (
                    f.id.clone(),
                    std::path::Path::new(new_path).join(&f.original_filename),
                )
            })
            .collect())
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

    pub fn generate_restore_code(&self) -> Result<String, LibraryError> {
        Ok(self.handle.generate_restore_code()?)
    }

    /// The library's membership: its devices (with this device flagged, each
    /// member's fingerprint, and whether it can be removed) and whether the
    /// running device is an owner.
    pub async fn get_members(&self) -> Result<crate::sync::membership::Membership, LibraryError> {
        self.sync.get_members().await
    }

    /// Approve a device into the library by its public key, wrapping the library
    /// key to it and signing a membership entry. Returns the invite code to hand
    /// back to the joining device. bae adds every device as a `Member`; the
    /// founding device is the `Owner`.
    pub async fn invite_member(
        &self,
        public_key_hex: &str,
        provider_account_email: Option<&str>,
    ) -> Result<String, LibraryError> {
        self.sync
            .invite_member(public_key_hex, provider_account_email)
            .await
    }

    /// Remove a device from the library and rotate the library key so the removed
    /// device can no longer read new data. Records the rotated key's fingerprint
    /// in this device's config.
    pub async fn remove_member(&self, public_key_hex: &str) -> Result<(), LibraryError> {
        self.sync.remove_member(public_key_hex).await
    }

    // ── Download (pin) queue ─────────────────────────────────────────
    //
    // Pinning routes through an in-memory serial queue instead of an ephemeral
    // per-release task: one release downloads at a time, the rest wait, and the
    // user can pause/cancel/retry. The queue is transient — on restart it's
    // empty and any release that wasn't fully pinned stays cloud-only (a pin
    // flips a release to pinned only after every file lands).

    /// Enqueue releases to pin for offline. Skips ids already in the queue (any
    /// state) or already pinned; for each new one, resolves its title /
    /// file_count / total_size from its storage summary so the Downloads pane
    /// can render the row without a re-query. Wakes the parked worker and emits
    /// a fresh `DownloadQueueChanged`.
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
                // already fully on disk, so there is nothing to download. Callers
                // that can't see storage state (the album grid's bulk pin) reach
                // here with a mixed selection — skip the local ones instead of
                // enqueueing a download that would only fail.
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
        let was_active = self.download_queue.cancel(release_id);
        if was_active {
            // The aborted pin task closes its progress channel; the worker's
            // drain sees the close and emits the terminal `ReleaseTransferEnded`
            // that clears the inline storage-row bar, then sees the entry is
            // gone and leaves the queue as-is. So we don't emit it here — and
            // the worker re-parks on its own once the drain returns, so no wake
            // is needed for the active case.
        }
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

    /// The serial download worker loop. Parks on the queue's `Notify` whenever
    /// the queue is paused or holds nothing queued; otherwise takes the next
    /// queued release, runs its pin, and repeats. Process strictly one release
    /// at a time.
    pub(super) async fn run_download_worker(&self) {
        loop {
            let Some(op) = self.download_queue.next_queued() else {
                self.download_queue.wait().await;
                continue;
            };
            self.run_queued_pin(&op).await;
        }
    }

    /// Run one queued release's pin: spawn `TransferService::pin_release_task`,
    /// flip the entry to `Active` and register its abort handle atomically, then
    /// drive its progress and re-emit the inline `ReleaseTransferProgress` the
    /// storage row reads. On success drop the entry; on failure mark it `Failed`
    /// (it stays in the queue for retry).
    ///
    /// `cancel_download` aborts the in-flight task via the registered handle. A
    /// cancel removes the queue entry; on its way out the drain sees the channel
    /// close, and we check whether the entry is still present before recording a
    /// failure — a cancelled download isn't a failure.
    async fn run_queued_pin(&self, op: &crate::library::DownloadOp) {
        use crate::storage::transfer::TransferService;

        let release_id = op.release_id.as_str();
        let initial_progress = match self.initial_download_progress(release_id).await {
            Ok(progress) => progress,
            Err(error) => {
                error!("Pin failed for release {release_id}: {error}");
                self.download_queue
                    .mark_failed(release_id, error.to_string());
                self.emit_download_queue_changed();
                return;
            }
        };
        let transfer = TransferService::new(self.clone());
        let (rx, pin_task) = transfer.pin_release_task(release_id.to_string());
        let abort = pin_task.abort_handle();
        // Flip to Active and register the abort handle atomically. If a cancel
        // removed the entry in the gap since we picked it, abort the task we
        // just spawned and bail — the release stays cloud-only.
        if !self
            .download_queue
            .activate(release_id, abort.clone(), initial_progress)
        {
            abort.abort();
            debug!("Pin for {release_id} cancelled before it started; aborting");
            return;
        }
        self.emit_download_queue_changed();
        // Drive the pin through the shared transfer driver; the inline
        // `ReleaseTransferProgress` bar is emitted by `drive_transfer` itself.
        let outcome = self
            .drive_transfer(release_id, ReleaseStorageAction::Pin, rx)
            .await;
        self.download_queue.clear_active_abort();

        // A cancel removed the entry while the pin was in flight. The drain
        // ended with an Err (the aborted pin task closed its channel) and
        // already emitted the terminal `ReleaseTransferEnded` that clears the
        // inline bar; `cancel_download` emitted the fresh snapshot. This isn't
        // a failure — don't re-add the entry or mark it Failed.
        if !self.download_queue.contains(release_id) {
            debug!("Pin for {release_id} ended after cancel; leaving queue as-is");
            return;
        }

        match outcome {
            Ok(()) => {
                // The release is pinned. `pin_release_blobs` already emitted
                // `ReleaseUpdated`, so its `pinned` flag flips true reactively —
                // just drop the queue entry.
                self.download_queue.remove(release_id);
                self.emit_download_queue_changed();
            }
            Err(error) => {
                error!("Pin failed for release {release_id}: {error}");
                self.download_queue
                    .mark_failed(release_id, error.to_string());
                self.emit_download_queue_changed();
            }
        }
    }

    /// Unpin a release: delete local copies, mark as cloud-only.
    pub async fn unpin_release(&self, release_id: &str) -> Result<(), LibraryError> {
        let transfer_service = crate::storage::transfer::TransferService::new(self.clone());
        let rx = transfer_service.unpin_release(release_id.to_string());
        self.drive_transfer(release_id, ReleaseStorageAction::Unpin, rx)
            .await
    }

    /// Manage a local release: upload its files to the cloud home. `pin`
    /// chooses whether coven keeps the blobs in `storage/pinned/` (offline) vs the
    /// evictable cache. The in-place source is always deleted once the upload lands
    /// (a remote release has no local path).
    pub async fn make_release_remote(
        &self,
        release_id: &str,
        pin: bool,
    ) -> Result<(), LibraryError> {
        let transfer_service = crate::storage::transfer::TransferService::new(self.clone());
        let rx = transfer_service.make_release_remote(release_id.to_string(), pin);
        self.drive_transfer(release_id, ReleaseStorageAction::MakeRemote, rx)
            .await
    }

    /// Unmanage a remote release: copy its files back out to `new_path` and
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
        let result = self
            .drive_transfer(release_id, ReleaseStorageAction::MakeLocal, rx)
            .await;
        result
    }

    /// Cancel the in-progress transition for a release, whatever it is: a pin
    /// (download), a remote upload, or an unmanage. The UI calls this from the
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
    /// transfer (unmanage), if one is registered; returns whether it fired. The
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

    /// Drain a transfer's progress channel, translating start into a
    /// `ReleaseTransferProgress` UI event and emitting `ReleaseTransferEnded` on
    /// completion or failure. On `Failed` the transfer channel's error string is
    /// wrapped as a `Storage` failure and surfaced to the caller.
    pub(super) async fn drive_transfer(
        &self,
        release_id: &str,
        action: ReleaseStorageAction,
        mut rx: tokio::sync::mpsc::UnboundedReceiver<crate::storage::transfer::TransferProgress>,
    ) -> Result<(), LibraryError> {
        use crate::storage::transfer::TransferProgress;

        // The bridge transfer future is abortable: a view dismiss / re-trigger
        // can drop this future between progress events, before its terminal
        // `ReleaseTransferEnded` emits. The guard fires that event on drop so a
        // cancelled transfer never freezes the progress bar on the release row;
        // the normal exit defuses it after emitting the event itself.
        let mut ended_guard = TransferEndedGuard {
            event_tx: self.event_tx.clone(),
            transfer_actions: self.transfer_actions.clone(),
            release_id: release_id.to_string(),
            armed: true,
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
                    self.transfer_actions
                        .lock()
                        .unwrap()
                        .insert(release_id.to_string(), action);
                    self.emit(LibraryEvent::ReleaseTransferProgress {
                        release_id: release_id.to_string(),
                        action,
                    });
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
                TransferProgress::Complete { .. } => break Ok(()),
                TransferProgress::Failed { error, .. } => break Err(LibraryError::Storage(error)),
            }
        };

        // Normal exit: emit the terminal event ourselves and defuse the guard so
        // its drop doesn't emit a second one.
        self.transfer_actions.lock().unwrap().remove(release_id);
        self.emit(LibraryEvent::ReleaseTransferEnded {
            release_id: release_id.to_string(),
        });
        ended_guard.defuse();
        outcome
    }
}
