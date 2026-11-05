use super::*;

/// A subscription opened while its library closes, from a task the close is
/// ending, has nothing left to read.
fn subscription_unstarted(error: &LibraryError) {
    tracing::warn!(%error, "a subscription asked for as its library closed ends unstarted");
}

/// The manager's background tasks, and the signal that ends them and the
/// subscriptions [`LibraryManager::until_closed`] made.
#[derive(Clone)]
pub(super) struct BackgroundTasks {
    tracker: tokio_util::task::TaskTracker,
    closing: tokio_util::sync::CancellationToken,
    closed: Arc<std::sync::Mutex<bool>>,
}

impl BackgroundTasks {
    /// Tasks that `closing` ends, with the subscriptions they made.
    fn new(closing: tokio_util::sync::CancellationToken) -> Self {
        Self { tracker: Default::default(), closing, closed: Default::default() }
    }

    /// End every task and wait for each to be gone, with whatever it held.
    pub(super) async fn close(&self) {
        {
            let mut closed = self.closed.lock().unwrap();
            *closed = true;
            self.closing.cancel();
            self.tracker.close();
        }
        self.tracker.wait().await;
    }
}

impl LibraryManager {
    /// Create the library manager over the library's opened store, with the
    /// services it drives, after the store's opening steps: its file
    /// transfers run as this device's preferences for it say, and what no
    /// process outlives is cleared. `closing` ends its tasks when it closes.
    pub(crate) async fn open(
        database: Database,
        app_dir: bae_app_dir::AppDir,
        clock: ClockRef,
        ids: IdSourceRef,
        diagnostics: Diagnostics,
        runtime_handle: tokio::runtime::Handle,
        remote_images: bae_catalogs::remote_image::RemoteImageCache,
        #[cfg(desktop)] catalogs: bae_import::CatalogSessions,
        sync: SyncController,
        downloads: bae_transfers::Downloads,
        #[cfg(desktop)] outputs: bae_transfers::Outputs,
        closing: tokio_util::sync::CancellationToken,
    ) -> Result<Self, bae_db::DbError> {
        // The store opens running one transfer per direction, the least any
        // preference allows, so nothing it starts before this exceeds them.
        database
            .set_transfer_limits(bae_preferences::preferences(&database).await?.transfer_limits());
        // No transfer outlives the process that ran it, so the library opens
        // with its transfer queues empty, before their workers start, and
        // with no release in a storage transition.
        database.call(bae_transfers::tables::clear_transfers_on).await?;
        // Nor does an attempt to start sync: a failed start stands only in
        // the process that made it.
        database.record_sync_start_failure(None).await?;
        let manager = LibraryManager {
            #[cfg(desktop)]
            catalogs,
            database,
            app_dir,
            remote_images,
            clock,
            ids,
            diagnostics,
            runtime_handle,
            tasks: BackgroundTasks::new(closing),
            sync,
            detach_cancels: Default::default(),
            make_local_transfers: bae_transfers::ReleaseTransfers::new(),
            downloads,
            #[cfg(desktop)]
            outputs,
            release_writes: Arc::default(),
            #[cfg(test)]
            pin_reads_hold: Arc::default(),
        };
        manager.start_queue_workers().expect("a manager being opened has not closed");
        manager.start_profile_requests().expect("a manager being opened has not closed");
        Ok(manager)
    }

    /// Follow this person's profile request while the library is open, and
    /// write their profile once sync has made its private circle (§5.4). A
    /// failed read or write is reported; the request stays, and the next
    /// open writes the profile.
    fn start_profile_requests(&self) -> Result<(), LibraryError> {
        let manager = self.clone();
        self.spawn_supervised_task(
            "profile request worker",
            async move {
                if let Err(error) = manager.database.finish_profile_requests().await {
                    manager.diagnostics.report_bug(
                        "write the requested profile",
                        &bae_util::error_text::CauseChain(&error),
                    );
                }
            },
            |_| {},
        )
    }

    pub(super) fn start_queue_workers(&self) -> Result<(), LibraryError> {
        self.spawn_queue_worker("download queue worker", |manager| async move {
            manager.run_download_worker().await;
        })?;
        #[cfg(desktop)]
        self.spawn_queue_worker("output queue worker", |manager| async move {
            manager.run_output_worker().await;
        })?;
        Ok(())
    }

    fn spawn_queue_worker<F, Fut>(
        &self,
        task_name: &'static str,
        worker: F,
    ) -> Result<(), LibraryError>
    where
        F: FnOnce(Self) -> Fut + Send + 'static,
        Fut: Future<Output = ()> + Send + 'static,
    {
        let manager = self.clone();
        // Each entry's own panic fails that entry and the worker goes on, so
        // what reaches here is a panic in the worker's reads and writes of
        // its table: reported, and the queue then waits until the library
        // opens again, which empties the table.
        self.spawn_supervised_task(task_name, worker(manager), |_| {})
    }

    /// Run `task` on the manager's runtime until it ends or the manager closes.
    /// Nothing awaits it, so a panic in it is reported as a bug here, and
    /// `on_panic` then ends what the task served, so its reader sees it end.
    /// A library that has closed starts nothing and says so to the caller.
    pub(crate) fn spawn_supervised_task<F>(
        &self,
        task_name: &'static str,
        task: F,
        on_panic: impl FnOnce(Bug) + Send + 'static,
    ) -> Result<(), LibraryError>
    where
        F: Future<Output = ()> + Send + 'static,
    {
        let closed = self.tasks.closed.lock().unwrap();
        if *closed {
            return Err(LibraryError::Closed(task_name));
        }
        let closing = self.tasks.closing.clone();
        let task = self.diagnostics.spawn_reported(
            &self.runtime_handle,
            task_name,
            async move {
                tokio::select! {
                    () = closing.cancelled() => {}
                    () = task => {}
                }
            },
            on_panic,
        );
        self.tasks.tracker.spawn_on(task.ended(), &self.runtime_handle);
        Ok(())
    }

    /// `spawn_supervised_task` for a task its owner stops too: it
    /// runs until it ends, `stop` is cancelled, or the manager closes.
    pub fn spawn_stoppable_task<F>(
        &self,
        task_name: &'static str,
        stop: crate::CancellationToken,
        task: F,
        on_panic: impl FnOnce(Bug) + Send + 'static,
    ) -> Result<(), LibraryError>
    where
        F: Future<Output = ()> + Send + 'static,
    {
        self.spawn_supervised_task(
            task_name,
            async move {
                stop.run_until_cancelled(task).await;
            },
            on_panic,
        )
    }

    /// Start the task behind a subscription to the library as one of the
    /// manager's tasks, which closing the library ends before the store
    /// closes; otherwise it runs until it ends or `stop` is cancelled. A
    /// panic in it is reported, and `reader` is told it failed. One asked for
    /// while the library closes starts nothing: the task and the sender it
    /// holds are dropped, so the subscription ends at once.
    pub fn spawn_stoppable_subscription_task<F>(
        &self,
        task_name: &'static str,
        stop: crate::CancellationToken,
        reader: impl crate::SubscriptionReader,
        task: F,
    ) where
        F: Future<Output = ()> + Send + 'static,
    {
        if let Err(error) = self.spawn_stoppable_task(task_name, stop, task, move |failed| {
            reader.fail(failed.into());
        }) {
            subscription_unstarted(&error);
        }
    }

    /// `query` as a subscription a surface reads by pulling its values,
    /// which closing the library ends before the store closes: a read waiting
    /// on it then answers that it was cancelled, and it takes no more
    /// requests. One opened while the library closes is cancelled from the
    /// start.
    pub fn until_closed<Request, Projection>(
        &self,
        query: coven::ReconfigurableLiveQuery<Request, Projection>,
    ) -> bae_db::live_query::CancellableLiveQuery<Request, Projection>
    where
        Request: Clone + PartialEq + Send + Sync + 'static,
        Projection: Clone + PartialEq + Send + 'static,
    {
        bae_db::live_query::CancellableLiveQuery::new(query, self.tasks.closing.child_token())
    }

    /// `query`'s values for a surface to pull, which closing the library ends
    /// before the store closes: a pull waiting on it then answers that it was
    /// cancelled. One opened while the library closes is cancelled from the
    /// start.
    pub fn values_until_closed<T: Clone + PartialEq + Send + 'static>(
        &self,
        query: impl bae_db::live_query::LiveSource<T> + 'static,
    ) -> bae_db::live_query::LiveValues<T> {
        bae_db::live_query::LiveValues::new(query, self.tasks.closing.child_token())
    }

    /// A file operation owns its rollback even when its caller disappears. A
    /// panic in it is reported as a bug and fails it as something that went
    /// wrong; an operation whose rollback must run on a panic catches it
    /// before the rollback, as a pin does. `operation` starts the work,
    /// handed a token of its own that closing the library cancels.
    pub(super) async fn run_file_operation<T, Fut>(
        &self,
        name: &'static str,
        operation: impl FnOnce(crate::CancellationToken) -> Fut,
    ) -> Result<T, LibraryError>
    where
        T: Send + 'static,
        Fut: Future<Output = Result<T, LibraryError>> + Send + 'static,
    {
        let task = {
            let closed = self.tasks.closed.lock().unwrap();
            if *closed {
                return Err(LibraryError::Closed(name));
            }
            let stop = crate::CancellationToken::new();
            let work = operation(stop.clone());
            let manager = self.clone();
            self.tasks.tracker.spawn_on(
                async move {
                    let work = caught(name, work);
                    tokio::pin!(work);
                    let caught = tokio::select! {
                        biased;
                        caught = &mut work => caught,
                        () = manager.tasks.closing.cancelled() => {
                            stop.cancel();
                            work.await
                        }
                    };
                    let result = match caught {
                        Ok(result) => result,
                        Err(bug) => Err(manager.panicked(bug).into()),
                    };
                    if let Err(error) = &result {
                        tracing::error!(operation = name, %error, "file operation failed");
                    }
                    result
                },
                &self.runtime_handle,
            )
        };
        joined(name, task).await.map_err(|failed| self.bug(failed))?
    }

    /// Close the library: stop the sync loop, end every background task the
    /// manager runs and every subscription `until_closed` made, then close
    /// the store, so when this returns nothing of the manager holds a file in
    /// the library directory and nothing reads the closed store. The services
    /// built on the manager (playback, import) end their own work when they
    /// are dropped.
    pub async fn close(&self) -> Result<(), LibraryError> {
        self.close_reporting(&|_| ()).await
    }

    /// [`Self::close`], telling `steps` as each step starts.
    pub async fn close_reporting(
        &self,
        steps: &(dyn Fn(crate::ClosingStep) + Send + Sync),
    ) -> Result<(), LibraryError> {
        steps(crate::ClosingStep::StoppingSync);
        if let Err(error) = self.database.stop_sync().await {
            warn!(%error, "stopping sync as the library closes failed");
        }
        steps(crate::ClosingStep::FinishingWork);
        self.tasks.close().await;
        steps(crate::ClosingStep::ClosingStore);
        self.database.close().await?;
        Ok(())
    }

    pub(crate) fn record_telemetry(&self, event: TelemetryEvent) {
        self.diagnostics.event(event);
    }

    /// Report a bug to the log and the platform's crash reporter.
    pub fn report_bug(&self, operation: &str, error: &dyn std::fmt::Display) {
        self.diagnostics.report_bug(operation, error);
    }

    /// Report work that panicked apart from its caller as a bug, and hand the
    /// failure back for the caller's own error.
    pub fn bug(
        &self,
        failed: bae_util::task_failure::TaskFailed,
    ) -> bae_util::task_failure::TaskFailed {
        self.diagnostics.bug(failed)
    }

    /// Report `bug`, work that panicked, and hand it back for the caller's
    /// own error, as [`Self::bug`] does for a join.
    pub fn panicked(&self, bug: Bug) -> Bug {
        self.diagnostics.panicked(bug)
    }

    /// Follow the sync status to log its failure and report a failed pass,
    /// each once, as it appears or changes. Call once after construction,
    /// with a tokio runtime available.
    pub fn start(&self) -> Result<(), LibraryError> {
        let mut statuses = self.subscribe_sync_status();
        let lm = self.clone();
        self.spawn_supervised_task(
            "sync status follower",
            async move {
                let mut last: Option<bae_db::SyncStatusSnapshot> = None;
                while let Some(read) = statuses.next().await {
                    let snapshot = match read {
                        Ok(snapshot) => snapshot,
                        Err(error) => {
                            warn!(%error, "could not read the sync status");
                            continue;
                        }
                    };
                    lm.follow_sync_status(last.as_ref(), &snapshot).await;
                    last = Some(snapshot);
                }
            },
            |failed| {
                tracing::error!(error = %failed, "the sync status follower ended");
            },
        )
    }

    /// Log a failure coven doesn't log itself, and report a failed pass to
    /// diagnostics, when `snapshot` brings it after `last`.
    async fn follow_sync_status(
        &self,
        last: Option<&bae_db::SyncStatusSnapshot>,
        snapshot: &bae_db::SyncStatusSnapshot,
    ) {
        if let Some(bae_diagnostics::UiError::Diagnostic { category, detail }) =
            snapshot.failure_to_log(last)
        {
            tracing::warn!(?category, "sync failure: {detail}");
        }
        let failed = |snapshot: &bae_db::SyncStatusSnapshot| match snapshot.shown_pass() {
            Some(coven::LastPass::Failed { error, .. }) => Some(error.clone()),
            _ => None,
        };
        if failed(snapshot).is_some() && last.and_then(failed) != failed(snapshot) {
            match self.database.storage_config().await {
                Ok(Some(storage)) => self.diagnostics.event(TelemetryEvent::SyncFailed {
                    provider: storage.provider(),
                    operation: SyncOperation::Cycle,
                }),
                Ok(None) => {}
                Err(error) => warn!(%error, "could not identify sync provider for diagnostics"),
            }
        }
    }

    /// Follow the sync status: coven's sync state beside one read of the
    /// sync status's rows, each value made from the latest of both.
    pub fn subscribe_sync_status(
        &self,
    ) -> bae_db::WithSyncState<bae_db::SyncStatusRows, bae_db::SyncStatusSnapshot> {
        self.database.with_sync_state(bae_db::sync_status_rows_on, bae_db::SyncStatusSnapshot::read)
    }

    /// How far and how fast each release with files being sent goes, and
    /// the files being sent together, told as each one moves.
    pub fn watch_outbox_transfers(&self) -> crate::OutboxTransfersWatch {
        crate::OutboxTransfersWatch::of(
            self.database.subscribe_sending_uploads(),
            self.database.subscribe_upload_transfers(),
            self.clock.clone(),
        )
    }

    /// The files being sent together, told each time they move.
    pub fn watch_outbox_total_transfer(&self) -> crate::OutboxTotalTransferWatch {
        crate::OutboxTotalTransferWatch::of(
            self.database.subscribe_sending_uploads(),
            self.database.subscribe_upload_transfers(),
            self.clock.clone(),
        )
    }

    /// How far each file being sent has got, by file, told as each one
    /// moves.
    pub fn watch_upload_file_transfers(&self) -> crate::FileTransfersWatch {
        crate::FileTransfersWatch::of(self.database.subscribe_upload_transfers())
    }

    /// How far each file being downloaded to keep whole has got, by file,
    /// told as each one moves.
    pub fn watch_pin_file_transfers(&self) -> crate::FileTransfersWatch {
        crate::FileTransfersWatch::of(self.database.subscribe_download_file_transfers())
    }

    /// Every entry of the download queue now, for a test that reads the
    /// whole queue.
    #[cfg(any(test, feature = "test-utils"))]
    pub async fn download_snapshot(&self) -> Result<bae_transfers::DownloadSnapshot, LibraryError> {
        Ok(self.downloads.snapshot().await?)
    }

    /// Follow every entry of the download queue, for a test that waits on
    /// the whole queue.
    #[cfg(any(test, feature = "test-utils"))]
    pub fn subscribe_download_snapshot(&self) -> coven::LiveQuery<bae_transfers::DownloadSnapshot> {
        self.downloads.subscribe()
    }

    /// Follow how many downloads the queue holds and the windows `initial`
    /// names of them, as the Downloads pane draws them.
    pub fn subscribe_download_windows(
        &self,
        initial: bae_db::LibraryPageWindows,
    ) -> coven::ReconfigurableLiveQuery<
        bae_db::LibraryPageWindows,
        bae_transfers::ReleaseQueueWindows<()>,
    > {
        self.downloads.subscribe_windows(initial)
    }

    /// How many downloads are in each phase now, and whether the queue is
    /// paused.
    pub async fn download_counts(&self) -> Result<bae_transfers::ReleaseQueueCounts, LibraryError> {
        Ok(self.downloads.counts().await?)
    }

    /// Follow how many downloads are in each phase, and whether the queue is
    /// paused: what the Downloads pane's controls show of them.
    pub fn subscribe_download_counts(&self) -> coven::LiveQuery<bae_transfers::ReleaseQueueCounts> {
        self.downloads.subscribe_counts()
    }

    /// Hear the bytes the active download has moved, by release, and each
    /// change to them.
    pub fn watch_download_transfers(
        &self,
    ) -> bae_transfers::ReleaseTransfersWatch<bae_transfers::DownloadTransferProgress> {
        self.downloads.watch_transfers()
    }

    /// Hear the bytes each release this process is making local has
    /// downloaded, by release, and each change to them.
    pub fn watch_make_local_transfers(
        &self,
    ) -> bae_transfers::ReleaseTransfersWatch<crate::MakeLocalTransferProgress> {
        self.make_local_transfers.watch()
    }

    /// The export queue as it stands now, each entry beside its release, the
    /// active one with the percent it has reached.
    #[cfg(desktop)]
    pub async fn output_snapshot(&self) -> Result<bae_transfers::OutputSnapshot, LibraryError> {
        // The table first: every export it reads as active has its percent
        // held from before it started until after it stops.
        let snapshot = self.outputs.snapshot().await?;
        snapshot.with_progress(&self.outputs.active_progress()).ok_or_else(|| {
            LibraryError::Internal("an export the queue reads as active has no percent".to_string())
        })
    }

    /// Follow every entry of the export queue, for a test that waits on the
    /// whole queue.
    #[cfg(all(desktop, test))]
    pub(crate) fn subscribe_output_snapshot(
        &self,
    ) -> coven::LiveQuery<bae_transfers::OutputPhaseSnapshot> {
        self.outputs.subscribe()
    }

    /// Hear the percent the active export has reached, by release, and each
    /// change to it.
    #[cfg(desktop)]
    pub fn watch_output_transfers(&self) -> bae_transfers::ReleaseTransfersWatch<u8> {
        self.outputs.watch_transfers()
    }

    /// A transfer queue's own table failed to read or write in its worker,
    /// which has nobody to tell: the worker waits for the next command.
    pub(super) fn report_queue_store_failure(&self, queue: &str, error: &bae_db::DbError) {
        self.diagnostics.report_bug(
            &format!("read or write the {queue} queue's table"),
            &bae_util::error_text::CauseChain(error),
        );
    }
}

#[cfg(test)]
#[path = "service_tests.rs"]
mod tests;
