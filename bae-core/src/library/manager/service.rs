use super::*;

/// Where an open of a library reports each step it runs, with how much of
/// it is done: rebuilding indexes, then migrating. Most opens run neither.
pub type OpenProgressReport = Box<dyn Fn(coven::Progress<coven::OpenStep>) + Send>;

/// Either open an existing store or retain a handle returned by creation or restore.
pub enum LibraryOpening {
    /// Open the registered store `store`, reporting the open's steps to
    /// `on_progress`.
    Registered {
        builder: coven::CovenBuilder,
        store: coven::StoreId,
        on_progress: OpenProgressReport,
    },
    Open(coven::CovenHandle),
}

/// A subscription opened while its library closes, from a task the close is
/// ending, has nothing left to read.
fn subscription_unstarted(error: &LibraryError) {
    tracing::warn!(%error, "a subscription asked for as its library closed ends unstarted");
}

/// The manager's background tasks, and the signal that ends them and the
/// subscriptions [`LibraryManager::until_closed`] made.
#[derive(Clone, Default)]
pub(super) struct BackgroundTasks {
    tracker: tokio_util::task::TaskTracker,
    closing: tokio_util::sync::CancellationToken,
    closed: Arc<std::sync::Mutex<bool>>,
}

impl BackgroundTasks {
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
    /// Open coven through the top-level builder and create the library manager
    /// over the resulting handle, running its file transfers as this device's
    /// preferences for it say.
    pub async fn open(
        app_dir: crate::config::AppDir,
        clock: ClockRef,
        ids: IdSourceRef,
        diagnostics: Diagnostics,
        runtime_handle: tokio::runtime::Handle,
        opening: LibraryOpening,
        remote_images: bae_catalogs::remote_image::RemoteImageCache,
        #[cfg(desktop)] providers: crate::catalog_model::providers::Providers,
    ) -> Result<Self, crate::db::DbError> {
        let handle = match opening {
            LibraryOpening::Registered { builder, store, on_progress } => {
                crate::library::configure_store(builder)
                    .clock(clock.clone())
                    .open(store, on_progress)
                    .await?
            }
            LibraryOpening::Open(handle) => handle,
        };
        let database = Database::from_handle(handle, clock.clone(), ids.clone());
        // The store opens running one transfer per direction, the least any
        // preference allows, so nothing it starts before this exceeds them.
        database.set_transfer_limits(database.preferences().await?.transfer_limits());
        // No transfer outlives the process that ran it, so the library opens
        // with its transfer queues empty, before their workers start, and
        // with no release in a storage transition.
        database.clear_transfers().await?;
        // Nor does discovery: the cast picker's list starts empty.
        database.clear_cast_devices().await?;
        // Nor does an attempt to start sync: a failed start stands only in
        // the process that made it.
        database.record_sync_start_failure(None).await?;
        #[cfg(desktop)]
        let catalogs = crate::import::CatalogSessions::new(
            providers,
            database.clone(),
            diagnostics.clone(),
            runtime_handle.clone(),
        );
        Ok(Self::assemble(
            database,
            app_dir,
            clock,
            ids,
            diagnostics,
            runtime_handle,
            remote_images,
            #[cfg(desktop)]
            catalogs,
        ))
    }

    /// Create a library manager over an already-open database handle, asking
    /// the catalogs through `catalogs`, which the caller built over the same
    /// database and may keep. Production uses [`Self::open`]; tests can supply
    /// a handle built with `TestCoven`.
    pub fn new(
        database: Database,
        app_dir: crate::config::AppDir,
        clock: ClockRef,
        ids: IdSourceRef,
        diagnostics: Diagnostics,
        runtime_handle: tokio::runtime::Handle,
        remote_images: bae_catalogs::remote_image::RemoteImageCache,
        #[cfg(desktop)] catalogs: crate::import::CatalogSessions,
    ) -> Self {
        Self::assemble(
            database,
            app_dir,
            clock,
            ids,
            diagnostics,
            runtime_handle,
            remote_images,
            #[cfg(desktop)]
            catalogs,
        )
    }

    /// Assemble the services that share this open library.
    fn assemble(
        database: Database,
        app_dir: crate::config::AppDir,
        clock: ClockRef,
        ids: IdSourceRef,
        diagnostics: Diagnostics,
        runtime_handle: tokio::runtime::Handle,
        remote_images: bae_catalogs::remote_image::RemoteImageCache,
        #[cfg(desktop)] catalogs: crate::import::CatalogSessions,
    ) -> Self {
        let sync = SyncController::new(database.clone(), diagnostics.clone());
        let downloads = crate::library::Downloads::new(database.clone());
        #[cfg(desktop)]
        let outputs = crate::library::Outputs::new(database.clone());
        let manager = LibraryManager {
            #[cfg(desktop)]
            preparations: crate::import::CandidatePreparations::new(database.clone()),
            #[cfg(desktop)]
            catalogs,
            database,
            app_dir,
            remote_images,
            clock,
            ids,
            diagnostics,
            runtime_handle,
            tasks: BackgroundTasks::default(),
            track_deletions: Arc::default(),
            sync,
            detach_cancels: Default::default(),
            make_local_transfers: crate::library::make_local_transfers::MakeLocalTransfers::new(),
            downloads,
            #[cfg(desktop)]
            outputs,
            release_writes: Arc::default(),
            #[cfg(test)]
            pin_reads_hold: Arc::default(),
        };
        manager.start_queue_workers().expect("a manager being assembled has not closed");
        manager.start_profile_requests().expect("a manager being assembled has not closed");
        manager
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
        on_panic: impl FnOnce(TaskFailed) + Send + 'static,
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

    /// [`Self::spawn_supervised_task`] for a task its owner stops too: it
    /// runs until it ends, `stop` is cancelled, or the manager closes.
    pub(crate) fn spawn_stoppable_task<F>(
        &self,
        task_name: &'static str,
        stop: crate::library::CancellationToken,
        task: F,
        on_panic: impl FnOnce(TaskFailed) + Send + 'static,
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
    pub(crate) fn spawn_stoppable_subscription_task<F>(
        &self,
        task_name: &'static str,
        stop: crate::library::CancellationToken,
        reader: impl crate::library::SubscriptionReader,
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

    /// Start the import service over this library. Its live reads end as
    /// the library closes, before the store does.
    #[cfg(desktop)]
    pub async fn start_import_service(
        &self,
        runtime_handle: tokio::runtime::Handle,
    ) -> Result<crate::import::ImportServiceHandle, crate::import::ImportError> {
        crate::import::ImportService::start(
            runtime_handle,
            self.database.clone(),
            self.catalogs.clone(),
            self.remote_images.clone(),
            self.diagnostics.clone(),
            self.preparations.clone(),
            self.clock.clone(),
            self.ids.clone(),
            self.tasks.closing.child_token(),
        )
        .await
    }

    /// [`Self::start_import_service`] reading folders' tags through
    /// `file_tags`.
    #[cfg(all(test, desktop))]
    pub(crate) fn start_import_service_reading_tags_with(
        &self,
        runtime_handle: tokio::runtime::Handle,
        file_tags: std::sync::Arc<dyn crate::import::file_tag_snapshot::FileTagReader>,
    ) -> crate::import::ImportServiceHandle {
        crate::import::ImportService::start_reading_tags_with(
            runtime_handle,
            self.database.clone(),
            self.catalogs.clone(),
            self.remote_images.clone(),
            self.diagnostics.clone(),
            self.preparations.clone(),
            self.clock.clone(),
            self.ids.clone(),
            self.tasks.closing.child_token(),
            file_tags,
        )
    }

    /// `query` as a subscription a surface reads by pulling its values,
    /// which closing the library ends before the store closes: a read waiting
    /// on it then answers that it was cancelled, and it takes no more
    /// requests. One opened while the library closes is cancelled from the
    /// start.
    pub(crate) fn until_closed<Request, Projection>(
        &self,
        query: coven::ReconfigurableLiveQuery<Request, Projection>,
    ) -> crate::live_query::CancellableLiveQuery<Request, Projection>
    where
        Request: Clone + PartialEq + Send + Sync + 'static,
        Projection: Clone + PartialEq + Send + 'static,
    {
        crate::live_query::CancellableLiveQuery::new(query, self.tasks.closing.child_token())
    }

    /// `query`'s values for a surface to pull, which closing the library ends
    /// before the store closes: a pull waiting on it then answers that it was
    /// cancelled. One opened while the library closes is cancelled from the
    /// start.
    pub(crate) fn values_until_closed<T: Clone + PartialEq + Send + 'static>(
        &self,
        query: coven::LiveQuery<T>,
    ) -> crate::live_query::LiveValues<T> {
        crate::live_query::LiveValues::new(query, self.tasks.closing.child_token())
    }

    /// A file operation owns its rollback even when its caller disappears. A
    /// panic in it is reported as a bug and fails it as something that went
    /// wrong; an operation whose rollback must run on a panic catches it
    /// before the rollback, as a pin does.
    pub(super) async fn run_file_operation<T, F, Fut>(
        &self,
        name: &'static str,
        operation: F,
    ) -> Result<T, LibraryError>
    where
        T: Send + 'static,
        F: FnOnce(crate::library::CancellationToken) -> Fut,
        Fut: Future<Output = Result<T, LibraryError>> + Send + 'static,
    {
        let task = {
            let closed = self.tasks.closed.lock().unwrap();
            if *closed {
                return Err(LibraryError::Closed(name));
            }
            let work = operation(self.tasks.closing.clone());
            let manager = self.clone();
            self.tasks.tracker.spawn_on(
                async move {
                    let result = match caught(name, work).await {
                        Ok(result) => result,
                        Err(failed) => Err(manager.bug(failed).into()),
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
        steps: &(dyn Fn(crate::library::ClosingStep) + Send + Sync),
    ) -> Result<(), LibraryError> {
        steps(crate::library::ClosingStep::StoppingSync);
        if let Err(error) = self.database.stop_sync().await {
            warn!(%error, "stopping sync as the library closes failed");
        }
        steps(crate::library::ClosingStep::FinishingWork);
        self.tasks.close().await;
        steps(crate::library::ClosingStep::ClosingStore);
        self.database.close().await?;
        Ok(())
    }

    pub(crate) fn record_telemetry(&self, event: TelemetryEvent) {
        self.diagnostics.event(event);
    }

    /// Report a bug found outside the library's own operations — playback's —
    /// to the log and the platform's crash reporter.
    pub(crate) fn report_bug(&self, operation: &str, error: &dyn std::fmt::Display) {
        self.diagnostics.report_bug(operation, error);
    }

    /// Report work that panicked apart from its caller as a bug, and hand the
    /// failure back for the caller's own error.
    pub(crate) fn bug(
        &self,
        failed: bae_util::task_failure::TaskFailed,
    ) -> bae_util::task_failure::TaskFailed {
        self.diagnostics.bug(failed)
    }

    /// Ship one `audio_format_orphaned` anomaly per orphan the release-detail
    /// projection reported (a pure layer that can only detect and log them). A
    /// zero count ships nothing.
    pub(super) fn report_audio_format_orphans(&self, count: u32) {
        for _ in 0..count {
            self.diagnostics.event(TelemetryEvent::Anomaly {
                kind: crate::diagnostics::AnomalyKind::AudioFormatOrphaned,
            });
        }
    }

    /// Follow the sync status to log its failure and report a failed pass,
    /// each once, as it appears or changes. Call once after construction,
    /// with a tokio runtime available.
    pub fn start(&self) -> Result<(), LibraryError> {
        let mut statuses = self.database.subscribe_sync_status();
        let lm = self.clone();
        self.spawn_supervised_task(
            "sync status follower",
            async move {
                let mut last: Option<SyncStatusSnapshot> = None;
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
        last: Option<&SyncStatusSnapshot>,
        snapshot: &SyncStatusSnapshot,
    ) {
        if let Some(crate::diagnostics::UiError::Diagnostic { category, detail }) =
            snapshot.failure_to_log(last)
        {
            tracing::warn!(?category, "sync failure: {detail}");
        }
        let failed = |snapshot: &SyncStatusSnapshot| match &snapshot.status {
            coven::SyncStatus::Failed { error, .. } => Some(error.clone()),
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

    /// Every deletion of tracks from now on, none dropped, as the ids each
    /// deleted.
    pub(crate) fn subscribe_track_deletions(
        &self,
    ) -> tokio::sync::mpsc::UnboundedReceiver<Vec<String>> {
        let (reader, deletions) = tokio::sync::mpsc::unbounded_channel();
        self.track_deletions.lock().unwrap().push(reader);
        deletions
    }

    /// Follow the sync status, each value read from its tables in one go.
    pub fn subscribe_sync_status(&self) -> coven::LiveQuery<SyncStatusSnapshot> {
        self.database.subscribe_sync_status()
    }

    /// Tell every reader these tracks are deleted.
    pub(super) fn tracks_deleted(&self, track_ids: Vec<String>) {
        self.track_deletions
            .lock()
            .unwrap()
            .retain(|reader| reader.send(track_ids.clone()).is_ok());
    }

    /// How far and how fast each release with files being sent goes, and
    /// the files being sent together, told as each one moves.
    pub fn watch_outbox_transfers(&self) -> crate::library::OutboxTransfersWatch {
        crate::library::OutboxTransfersWatch::of(
            self.database.subscribe_sending_uploads(),
            self.database.subscribe_upload_transfers(),
            self.clock.clone(),
        )
    }

    /// The files being sent together, told each time they move.
    pub fn watch_outbox_total_transfer(&self) -> crate::library::OutboxTotalTransferWatch {
        crate::library::OutboxTotalTransferWatch::of(
            self.database.subscribe_sending_uploads(),
            self.database.subscribe_upload_transfers(),
            self.clock.clone(),
        )
    }

    /// How far each file being sent has got, by file, told as each one
    /// moves.
    pub fn watch_upload_file_transfers(&self) -> crate::library::FileTransfersWatch {
        crate::library::FileTransfersWatch::of(self.database.subscribe_upload_transfers())
    }

    /// How far each file being downloaded to keep whole has got, by file,
    /// told as each one moves.
    pub fn watch_pin_file_transfers(&self) -> crate::library::FileTransfersWatch {
        crate::library::FileTransfersWatch::of(self.database.subscribe_download_file_transfers())
    }

    /// Every entry of the download queue now, for a test that reads the
    /// whole queue.
    #[cfg(any(test, feature = "test-utils"))]
    pub async fn download_snapshot(
        &self,
    ) -> Result<crate::library::DownloadSnapshot, LibraryError> {
        self.downloads.snapshot().await
    }

    /// Follow every entry of the download queue, for a test that waits on
    /// the whole queue.
    #[cfg(any(test, feature = "test-utils"))]
    pub fn subscribe_download_snapshot(
        &self,
    ) -> coven::LiveQuery<crate::library::DownloadSnapshot> {
        self.downloads.subscribe()
    }

    /// Follow how many downloads the queue holds and the windows `initial`
    /// names of them, as the Downloads pane draws them.
    pub(crate) fn subscribe_download_windows(
        &self,
        initial: crate::library::LibraryPageWindows,
    ) -> coven::ReconfigurableLiveQuery<
        crate::library::LibraryPageWindows,
        crate::library::ReleaseQueueWindows<()>,
    > {
        self.downloads.subscribe_windows(initial)
    }

    /// How many downloads are in each phase now, and whether the queue is
    /// paused.
    pub async fn download_counts(
        &self,
    ) -> Result<crate::library::ReleaseQueueCounts, LibraryError> {
        self.downloads.counts().await
    }

    /// Follow everything sync is doing: the sync status, the upload queue's
    /// counts and the offline downloads, read in one go.
    pub(crate) fn subscribe_sync_activity(&self) -> coven::LiveQuery<crate::library::SyncActivity> {
        self.database.subscribe_sync_activity()
    }

    /// Follow how many downloads are in each phase, and whether the queue is
    /// paused: what the Downloads pane's controls show of them.
    pub(crate) fn subscribe_download_counts(
        &self,
    ) -> coven::LiveQuery<crate::library::ReleaseQueueCounts> {
        self.downloads.subscribe_counts()
    }

    /// The bytes the active download has moved, by release, and each change
    /// to them.
    pub(crate) fn subscribe_download_transfers(
        &self,
    ) -> tokio::sync::watch::Receiver<
        std::collections::BTreeMap<String, crate::library::DownloadTransferProgress>,
    > {
        self.downloads.subscribe_transfers()
    }

    /// The bytes each release this process is making local has downloaded,
    /// by release, and each change to them.
    pub(crate) fn subscribe_make_local_transfers(
        &self,
    ) -> tokio::sync::watch::Receiver<
        std::collections::BTreeMap<String, crate::library::MakeLocalTransferProgress>,
    > {
        self.make_local_transfers.subscribe()
    }

    /// The export queue as it stands now, each entry beside its release, the
    /// active one with the percent it has reached.
    #[cfg(desktop)]
    pub async fn output_snapshot(&self) -> Result<crate::library::OutputSnapshot, LibraryError> {
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
    ) -> coven::LiveQuery<crate::library::OutputPhaseSnapshot> {
        self.outputs.subscribe()
    }

    /// Follow the files of the release `initial` names in its order.
    #[cfg(desktop)]
    pub(crate) fn subscribe_release_files(
        &self,
        initial: Option<crate::library::ReleaseFilesRequest>,
    ) -> coven::ReconfigurableLiveQuery<
        Option<crate::library::ReleaseFilesRequest>,
        Option<Vec<crate::db::DbFile>>,
    > {
        self.database.subscribe_release_files(initial)
    }

    /// Follow the release `initial` names as the storage inspector shows it,
    /// each value read in one go.
    #[cfg(desktop)]
    pub(crate) fn subscribe_storage_inspector(
        &self,
        initial: Option<String>,
    ) -> coven::ReconfigurableLiveQuery<Option<String>, Option<crate::db::DbStorageInspector>> {
        self.database.subscribe_storage_inspector(initial)
    }

    /// The percent the active export has reached, by release, and each
    /// change to it.
    #[cfg(desktop)]
    pub(crate) fn subscribe_output_transfers(
        &self,
    ) -> tokio::sync::watch::Receiver<std::collections::BTreeMap<String, u8>> {
        self.outputs.subscribe_transfers()
    }

    /// A transfer queue's own table failed to read or write in its worker,
    /// which has nobody to tell: the worker waits for the next command.
    pub(super) fn report_queue_store_failure(&self, queue: &str, error: &LibraryError) {
        self.diagnostics.report_bug(
            &format!("read or write the {queue} queue's table"),
            &bae_util::error_text::CauseChain(error),
        );
    }
}

/// The library's store, for a test that writes rows no operation writes.
#[cfg(all(test, desktop))]
mod test_access {
    impl super::LibraryManager {
        /// The store under the library, for a test that writes rows no
        /// operation writes as it wants them.
        pub(crate) fn database_for_test(&self) -> &crate::db::Database {
            &self.database
        }
    }
}
