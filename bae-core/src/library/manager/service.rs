use super::*;

/// Either open an existing store or retain a handle returned by creation or restore.
pub enum LibraryOpening {
    Registered(coven::CovenBuilder),
    Open(coven::CovenHandle),
}

impl From<coven::CovenBuilder> for LibraryOpening {
    fn from(builder: coven::CovenBuilder) -> Self {
        Self::Registered(builder)
    }
}

async fn report_background_task_exit(task_name: &'static str, task: tokio::task::JoinHandle<()>) {
    if let Err(error) = task.await {
        tracing::error!(task = task_name, %error, "background task failed");
    }
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
    /// over the resulting handle.
    pub async fn open(
        app_dir: crate::config::AppDir,
        config_handle: Arc<ConfigHandle>,
        clock: ClockRef,
        ids: IdSourceRef,
        diagnostics: Diagnostics,
        runtime_handle: tokio::runtime::Handle,
        opening: impl Into<LibraryOpening>,
        remote_images: bae_catalogs::remote_image::RemoteImageCache,
        #[cfg(desktop)] providers: crate::providers::Providers,
    ) -> Result<Self, crate::db::DbError> {
        let limits = config_handle.config().prefs.transfer_limits();
        let store_id = config_handle.config().store_id;
        let handle = match opening.into() {
            LibraryOpening::Registered(builder) => {
                crate::library::with_transfer_limits(
                    crate::library::configure_store(builder),
                    limits,
                )
                .clock(clock.clone())
                .open(store_id)
                .await?
            }
            LibraryOpening::Open(handle) => {
                handle.set_transfer_limits(limits);
                handle
            }
        };
        let database = Database::from_handle(handle, clock.clone());
        // No transfer outlives the process that ran it, so the library opens
        // with its transfer queues empty, before their workers start.
        database.clear_release_queues().await?;
        Ok(Self::assemble(
            database,
            app_dir,
            config_handle,
            clock,
            ids,
            diagnostics,
            runtime_handle,
            remote_images,
            #[cfg(desktop)]
            providers,
        ))
    }

    /// Create a library manager over an already-open database handle. Production
    /// uses [`Self::open`]; tests can supply a handle built with `TestCoven`.
    pub fn new(
        database: Database,
        app_dir: crate::config::AppDir,
        config_handle: Arc<ConfigHandle>,
        clock: ClockRef,
        ids: IdSourceRef,
        diagnostics: Diagnostics,
        runtime_handle: tokio::runtime::Handle,
        remote_images: bae_catalogs::remote_image::RemoteImageCache,
        #[cfg(desktop)] providers: crate::providers::Providers,
    ) -> Self {
        Self::assemble(
            database,
            app_dir,
            config_handle,
            clock,
            ids,
            diagnostics,
            runtime_handle,
            remote_images,
            #[cfg(desktop)]
            providers,
        )
    }

    /// Assemble the services that share this open library.
    fn assemble(
        database: Database,
        app_dir: crate::config::AppDir,
        config_handle: Arc<ConfigHandle>,
        clock: ClockRef,
        ids: IdSourceRef,
        diagnostics: Diagnostics,
        runtime_handle: tokio::runtime::Handle,
        remote_images: bae_catalogs::remote_image::RemoteImageCache,
        #[cfg(desktop)] providers: crate::providers::Providers,
    ) -> Self {
        let sync_status = SyncStatusState::new(&database);
        let sync = SyncController::new(database.clone(), diagnostics.clone());
        let downloads = crate::library::Downloads::new(database.clone());
        #[cfg(desktop)]
        let outputs = crate::library::Outputs::new(database.clone());
        let manager = LibraryManager {
            #[cfg(desktop)]
            preparations: crate::import::CandidatePreparations::new(database.clone()),
            database,
            app_dir,
            config_handle,
            remote_images,
            #[cfg(desktop)]
            providers,
            #[cfg(desktop)]
            discogs_client: Arc::new(std::sync::Mutex::new(None)),
            clock,
            ids,
            diagnostics,
            runtime_handle,
            tasks: BackgroundTasks::default(),
            track_deletions: Arc::default(),
            sync,
            sync_status,
            transitions: crate::library::storage_transitions::StorageTransitions::new(),
            downloads,
            #[cfg(desktop)]
            outputs,
            release_writes: Arc::default(),
            #[cfg(test)]
            disk_hold: Arc::default(),
        };
        manager
            .start_queue_workers()
            .expect("a manager being assembled has not closed");
        manager
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
        self.spawn_supervised_task(task_name, worker(manager))
    }

    /// Run `task` on the manager's runtime until it ends or the manager closes.
    /// A task that panics is reported, since nothing else awaits it. A library
    /// that has closed starts nothing and says so to the caller.
    pub(crate) fn spawn_supervised_task<F>(
        &self,
        task_name: &'static str,
        task: F,
    ) -> Result<(), LibraryError>
    where
        F: Future<Output = ()> + Send + 'static,
    {
        let closed = self.tasks.closed.lock().unwrap();
        if *closed {
            return Err(LibraryError::Closed(task_name));
        }
        let closing = self.tasks.closing.clone();
        let task = self.runtime_handle.spawn(async move {
            tokio::select! {
                () = closing.cancelled() => {}
                () = task => {}
            }
        });
        self.tasks.tracker.spawn_on(
            report_background_task_exit(task_name, task),
            &self.runtime_handle,
        );
        Ok(())
    }

    /// [`Self::spawn_supervised_task`] for a task its owner stops too: it
    /// runs until it ends, `stop` is cancelled, or the manager closes.
    pub(crate) fn spawn_stoppable_task<F>(
        &self,
        task_name: &'static str,
        stop: crate::library::CancellationToken,
        task: F,
    ) -> Result<(), LibraryError>
    where
        F: Future<Output = ()> + Send + 'static,
    {
        self.spawn_supervised_task(task_name, async move {
            stop.run_until_cancelled(task).await;
        })
    }

    /// Start the task behind a subscription to the library as one of the
    /// manager's tasks, which closing the library ends before the store
    /// closes; otherwise it runs until it ends. One asked for while the library
    /// closes starts nothing: the task and the sender it holds are dropped, so
    /// the subscription ends at once.
    pub(crate) fn spawn_subscription_task<F>(&self, task_name: &'static str, task: F)
    where
        F: Future<Output = ()> + Send + 'static,
    {
        if let Err(error) = self.spawn_supervised_task(task_name, task) {
            subscription_unstarted(&error);
        }
    }

    /// [`Self::spawn_subscription_task`] for a task its subscription stops
    /// too, by cancelling `stop`.
    pub(crate) fn spawn_stoppable_subscription_task<F>(
        &self,
        task_name: &'static str,
        stop: crate::library::CancellationToken,
        task: F,
    ) where
        F: Future<Output = ()> + Send + 'static,
    {
        if let Err(error) = self.spawn_stoppable_task(task_name, stop, task) {
            subscription_unstarted(&error);
        }
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

    /// A file operation owns its rollback even when its caller disappears.
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
            self.tasks.tracker.spawn_on(
                async move {
                    let result = work.await;
                    if let Err(error) = &result {
                        tracing::error!(operation = name, %error, "file operation failed");
                    }
                    result
                },
                &self.runtime_handle,
            )
        };
        task.await
            .map_err(|error| LibraryError::Storage(format!("{name} task failed: {error}")))?
    }

    /// Close the library: stop the sync loop, end every background task the
    /// manager runs and every subscription `until_closed` made, then close
    /// the store, so when this returns nothing of the manager holds a file in
    /// the library directory and nothing reads the closed store. The services
    /// built on the manager (playback, import) end their own work when they
    /// are dropped.
    pub async fn close(&self) -> Result<(), LibraryError> {
        self.database.stop_sync();
        self.tasks.close().await;
        self.database.close().await?;
        Ok(())
    }

    pub(crate) fn current_transfer_action(&self, release_id: &str) -> Option<ReleaseStorageAction> {
        self.transitions.current(release_id)
    }

    pub fn subscribe_transfer_values(
        &self,
    ) -> tokio::sync::watch::Receiver<HashMap<String, ReleaseStorageAction>> {
        self.transitions.subscribe()
    }

    /// Read the injected wall clock without handing its service to callers.
    #[cfg(desktop)]
    pub(crate) fn now(&self) -> chrono::DateTime<chrono::Utc> {
        self.clock.now().into()
    }

    #[cfg(desktop)]
    pub(crate) fn new_id(&self) -> String {
        self.ids.new_id().to_string()
    }

    pub(crate) fn record_telemetry(&self, event: TelemetryEvent) {
        self.diagnostics.event(event);
    }

    /// Report a bug found outside the library's own operations — playback's —
    /// to the log and the platform's crash reporter.
    pub(crate) fn report_bug(&self, operation: &str, error: &dyn std::fmt::Display) {
        self.diagnostics.report_bug(operation, error);
    }

    #[cfg(desktop)]
    pub(crate) async fn search_musicbrainz(
        &self,
        params: bae_catalogs::musicbrainz::ReleaseSearchParams,
        priority: bae_catalogs::rate_limiter::CallPriority,
    ) -> Result<Vec<crate::import::search::MetadataResult>, crate::import::ImportError> {
        self.providers.search_musicbrainz(params, priority).await
    }

    #[cfg(desktop)]
    pub(crate) async fn lookup_musicbrainz_discid(
        &self,
        discid: &str,
        priority: bae_catalogs::rate_limiter::CallPriority,
    ) -> Result<
        Vec<(
            crate::import::search::MetadataResult,
            crate::import::payloads::DiscIdRelease,
        )>,
        crate::signals::Failure,
    > {
        self.providers
            .lookup_musicbrainz_discid(discid, priority)
            .await
    }

    #[cfg(desktop)]
    pub(crate) async fn lookup_musicbrainz_isrcs(
        &self,
        isrcs: &[String],
        priority: bae_catalogs::rate_limiter::CallPriority,
    ) -> Result<Vec<crate::import::search::MetadataResult>, crate::signals::Failure> {
        self.providers
            .lookup_musicbrainz_isrcs(isrcs, priority)
            .await
    }

    /// The Cover Art Archive's images of a MusicBrainz release, then of its
    /// group.
    #[cfg(desktop)]
    pub(crate) async fn musicbrainz_gallery(
        &self,
        release_id: &str,
        group_id: Option<&str>,
    ) -> Result<
        Vec<bae_catalogs::remote_image::RemoteCover>,
        bae_catalogs::remote_image::RemoteImageError,
    > {
        self.providers
            .musicbrainz_gallery(release_id, group_id)
            .await
    }

    #[cfg(desktop)]
    pub(crate) async fn musicbrainz_group_gallery(
        &self,
        group_id: &str,
    ) -> Result<
        Vec<bae_catalogs::remote_image::RemoteCover>,
        bae_catalogs::remote_image::RemoteImageError,
    > {
        self.providers.musicbrainz_group_gallery(group_id).await
    }

    /// Every image one pick's releases offer, the primary's first.
    #[cfg(desktop)]
    pub(crate) async fn pick_gallery_covers(
        &self,
        primary: &crate::import::source_release::SourceRelease,
        partners: &[crate::import::source_release::SourceRelease],
    ) -> Result<Vec<bae_catalogs::remote_image::RemoteCover>, crate::import::ImportError> {
        self.providers.pick_gallery_covers(primary, partners).await
    }

    #[cfg(desktop)]
    pub(crate) async fn fetch_remote_image(
        &self,
        url: &str,
    ) -> Result<
        Option<bae_catalogs::remote_image::RemoteImage>,
        bae_catalogs::remote_image::RemoteImageError,
    > {
        self.remote_images.fetch(url).await
    }

    #[cfg(desktop)]
    pub(crate) async fn fetch_required_remote_image(
        &self,
        url: &str,
    ) -> Result<bae_catalogs::remote_image::RemoteImage, bae_catalogs::remote_image::RemoteImageError>
    {
        self.remote_images.fetch_required(url).await
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

    /// Every row of every table the library holds, rendered, by table name.
    #[cfg(test)]
    pub(crate) async fn every_row_for_test(
        &self,
    ) -> std::collections::BTreeMap<String, Vec<String>> {
        self.database.every_row_for_test().await.unwrap()
    }

    /// Subscribe to the sync loop's status and what a retry would do, and
    /// publish the current status line value. Call once after construction,
    /// with a tokio runtime available.
    pub fn start(&self) -> Result<(), LibraryError> {
        let mut statuses = self.database.subscribe_sync_status();
        let mut effects = self.database.subscribe_retry_effect();
        let lm = self.clone();
        self.spawn_supervised_task("sync status subscription", async move {
            let mut first = true;
            loop {
                let status_changed = first || statuses.has_changed().unwrap_or(false);
                first = false;
                let status = statuses.borrow_and_update().clone();
                // coven publishes what a retry would do before the status a
                // pass leaves, so the effect read after it is at least as new.
                let effect = *effects.borrow_and_update();
                // `set_sync` logs the failure, once, as it appears or changes.
                if status_changed && matches!(status, coven::SyncStatus::Failed { .. }) {
                    match lm.database.storage_config().await {
                        Ok(Some(storage)) => lm.diagnostics.event(TelemetryEvent::SyncFailed {
                            provider: storage.provider(),
                            operation: SyncOperation::Cycle,
                        }),
                        Ok(None) => {}
                        Err(error) => {
                            warn!(%error, "could not identify sync provider for diagnostics")
                        }
                    }
                }
                lm.sync_status.set_sync(status, effect);
                let open = tokio::select! {
                    changed = statuses.changed() => changed.is_ok(),
                    changed = effects.changed() => changed.is_ok(),
                };
                if !open {
                    break;
                }
            }
        })?;
        let pending = super::pending_item::PendingFollower::new(self.clone());
        self.spawn_supervised_task("pending sync records", pending.run())
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

    pub fn subscribe_sync_status_values(
        &self,
    ) -> tokio::sync::watch::Receiver<crate::library::SyncStatusSnapshot> {
        self.sync_status.subscribe()
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

    /// Follow how many downloads are in each phase, and whether the queue is
    /// paused: what the sync activity and the Downloads pane's controls show
    /// of them.
    /// The upload queue's counts, live.
    pub(crate) fn subscribe_upload_counts(
        &self,
    ) -> coven::LiveQuery<crate::library::UploadQueueCounts> {
        self.database.subscribe_upload_counts()
    }

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

    /// The export queue as it stands now, each entry beside its release, the
    /// active one with the percent it has reached.
    #[cfg(desktop)]
    pub async fn output_snapshot(&self) -> Result<crate::library::OutputSnapshot, LibraryError> {
        // The table first: every export it reads as active has its percent
        // held from before it started until after it stops.
        let snapshot = self.outputs.snapshot().await?;
        snapshot
            .with_progress(&self.outputs.active_progress())
            .ok_or_else(|| {
                LibraryError::Internal(
                    "an export the queue reads as active has no percent".to_string(),
                )
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

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn panicked_background_task_is_logged() {
        let logs = crate::test_logs::capture_warn_logs_async(|| async {
            report_background_task_exit(
                "test worker",
                tokio::spawn(async { panic!("worker panic") }),
            )
            .await;
        })
        .await;

        assert!(
            logs.contains("background task failed")
                && logs.contains("test worker")
                && logs.contains("panicked"),
            "the task failure must be named in the log, got: {logs}"
        );
    }
}

/// The providers a library asks, for a test to seed their answers.
#[cfg(all(any(test, feature = "test-utils"), desktop))]
mod test_access {
    impl super::LibraryManager {
        pub fn providers(&self) -> &crate::providers::Providers {
            &self.providers
        }
    }
}
