use super::*;

async fn report_background_task_exit(task_name: &'static str, task: tokio::task::JoinHandle<()>) {
    if let Err(error) = task.await {
        tracing::error!(task = task_name, %error, "background task failed");
    }
}

impl LibraryManager {
    /// Open coven through the top-level builder and create the library manager
    /// over the resulting handle.
    #[allow(clippy::too_many_arguments)]
    pub fn open(
        config_handle: Arc<ConfigHandle>,
        clock: ClockRef,
        ids: IdRef,
        diagnostics: Diagnostics,
        runtime_handle: tokio::runtime::Handle,
        cloudkit_ops: Option<Arc<dyn coven::CloudKitOps>>,
        remote_images: crate::import::cover_art::RemoteImageCache,
    ) -> Result<Self, coven::DbError> {
        let (event_tx, _) = broadcast::channel(LIBRARY_EVENT_CHANNEL_CAPACITY);
        let uploads = crate::library::live_uploads::LiveUploads::new();

        let (observer, observer_events) =
            crate::sync::upload_observer::ReleaseUploadObserver::new(uploads.clone());
        let observer = Arc::new(observer);
        // coven holds only a `Weak` to the observer (via `WeakUploadObserver`);
        // the `LibraryManager` below owns the strong `Arc`. Registering the
        // observer strongly here would close a cycle through the `CovenHandle` it
        // holds back, pinning coven's store-open lock past the manager's life.
        let weak_observer = Arc::new(crate::sync::upload_observer::WeakUploadObserver::new(
            Arc::downgrade(&observer),
        ));
        let (max_uploads, max_downloads) = {
            let config = config_handle.config();
            (
                crate::config::usize_bound(config.max_concurrent_uploads),
                crate::config::usize_bound(config.max_concurrent_downloads),
            )
        };
        let handle = config_handle
            .coven_builder()
            .synced_tables(crate::sync::synced_tables())
            .clock(clock.clone())
            .oauth_clients(crate::oauth::clients())
            .apply_cloudkit_ops(cloudkit_ops.clone())
            .observer(weak_observer as Arc<dyn coven::BlobTransitionObserver>)
            .migrations(crate::migrations::all())
            .max_concurrent_uploads(max_uploads)
            .max_concurrent_downloads(max_downloads)
            .open()
            .map_err(|e| match e {
                coven::CovenError::Database(error) => *error,
                other => coven::DbError::Message(other.to_string()),
            })?;
        let database = Database::from_handle(handle.clone(), clock.clone(), ids.clone());
        let sync_status = SyncStatus::new(database.clone());

        let sync = SyncController::new(
            config_handle.clone(),
            database.clone(),
            uploads,
            cloudkit_ops,
            diagnostics.clone(),
        );

        let manager = LibraryManager {
            #[cfg(not(any(target_os = "ios", target_os = "android")))]
            preparations: crate::import::CandidatePreparations::new(database.clone()),
            database,
            config_handle,
            remote_images,
            clock,
            ids,
            diagnostics,
            runtime_handle,
            event_tx,
            sync,
            sync_status,
            transitions: crate::library::storage_transitions::StorageTransitions::new(),
            downloads: crate::library::Downloads::new(
                crate::library::download_snapshot::build_download_snapshot,
            ),
            #[cfg(not(any(target_os = "ios", target_os = "android")))]
            outputs: crate::library::Outputs::new(
                crate::library::output_snapshot::build_output_snapshot,
            ),
            _upload_observer: observer,
        };
        manager.start_upload_observer_events(observer_events);
        manager.start_queue_workers();
        Ok(manager)
    }

    /// Create a library manager over an already-open database handle. Production
    /// uses [`Self::open`] so the upload observer is installed into coven before
    /// sync starts; this constructor remains for tests that exercise database-only
    /// manager behavior.
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        database: Database,
        config_handle: Arc<ConfigHandle>,
        clock: ClockRef,
        ids: IdRef,
        diagnostics: Diagnostics,
        runtime_handle: tokio::runtime::Handle,
        remote_images: crate::import::cover_art::RemoteImageCache,
    ) -> Self {
        let (event_tx, _) = broadcast::channel(LIBRARY_EVENT_CHANNEL_CAPACITY);
        let uploads = crate::library::live_uploads::LiveUploads::new();
        let (observer, observer_events) =
            crate::sync::upload_observer::ReleaseUploadObserver::new(uploads.clone());

        let sync_status = SyncStatus::new(database.clone());

        let sync = SyncController::new(
            config_handle.clone(),
            database.clone(),
            uploads,
            None,
            diagnostics.clone(),
        );
        let manager = LibraryManager {
            #[cfg(not(any(target_os = "ios", target_os = "android")))]
            preparations: crate::import::CandidatePreparations::new(database.clone()),
            database,
            config_handle,
            remote_images,
            clock,
            ids,
            diagnostics,
            runtime_handle,
            event_tx,
            sync,
            sync_status,
            transitions: crate::library::storage_transitions::StorageTransitions::new(),
            downloads: crate::library::Downloads::new(
                crate::library::download_snapshot::build_download_snapshot,
            ),
            #[cfg(not(any(target_os = "ios", target_os = "android")))]
            outputs: crate::library::Outputs::new(
                crate::library::output_snapshot::build_output_snapshot,
            ),
            _upload_observer: Arc::new(observer),
        };
        manager.start_upload_observer_events(observer_events);
        manager.start_queue_workers();
        manager
    }

    pub(super) fn start_upload_observer_events(
        &self,
        events: crate::sync::upload_observer::UploadObserverEvents,
    ) {
        let sync = self.sync.clone();
        self.spawn_supervised_task("upload observer event processor", async move {
            events
                .run(move || {
                    let sync = sync.clone();
                    async move {
                        sync.process_upload_observer_event().await;
                    }
                })
                .await;
        });
    }

    pub(super) fn start_queue_workers(&self) {
        self.spawn_queue_worker("download queue worker", |manager| async move {
            manager.run_download_worker().await;
        });
        #[cfg(not(any(target_os = "ios", target_os = "android")))]
        self.spawn_queue_worker("output queue worker", |manager| async move {
            manager.run_output_worker().await;
        });
    }

    fn spawn_queue_worker<F, Fut>(&self, task_name: &'static str, worker: F)
    where
        F: FnOnce(Self) -> Fut + Send + 'static,
        Fut: Future<Output = ()> + Send + 'static,
    {
        let manager = self.clone();
        self.spawn_supervised_task(task_name, worker(manager));
    }

    pub(super) fn spawn_supervised_task<F>(&self, task_name: &'static str, task: F)
    where
        F: Future<Output = ()> + Send + 'static,
    {
        let task = self.runtime_handle.spawn(task);
        self.runtime_handle
            .spawn(report_background_task_exit(task_name, task));
    }

    pub(crate) fn current_transfer_action(&self, release_id: &str) -> Option<ReleaseStorageAction> {
        self.transitions.current(release_id)
    }

    pub fn subscribe_transfer_values(
        &self,
    ) -> tokio::sync::watch::Receiver<HashMap<String, ReleaseStorageAction>> {
        self.transitions.subscribe()
    }

    /// Connect a real `SyncManager` over an injected cloud home for tests, so the
    /// handle's make-Remote / make-Local / upload-drain / read paths all run
    /// against a mock cloud with no live provider — the test counterpart of
    /// `attach_and_start_sync`. `cipher` is the home's at-rest protection:
    /// `Plaintext` for a browsable mock, `Encrypted(service)` for an opaque one.
    /// After this, `has_cloud_home` and `is_sync_ready` resolve off the
    /// connected manager, no override needed.
    #[cfg(any(test, feature = "test-utils"))]
    pub async fn connect_test_cloud_home(
        &self,
        cloud_home: Arc<dyn ExactCloudHome>,
        cipher: crate::sync::CloudCipher,
    ) -> Result<(), LibraryError> {
        self.sync
            .connect_test_cloud_home(cloud_home, cipher)
            .await?;
        self.sync_status.publish();
        Ok(())
    }

    /// The same connection with no sync loop behind it, so an explicitly
    /// draining test is the only drainer of the upload queue. See
    /// `SyncController::connect_test_cloud_home_caller_driven` for what a
    /// test gives up by taking it.
    #[cfg(any(test, feature = "test-utils"))]
    pub async fn connect_test_cloud_home_caller_driven(
        &self,
        cloud_home: Arc<dyn ExactCloudHome>,
        cipher: crate::sync::CloudCipher,
    ) -> Result<(), LibraryError> {
        self.sync
            .connect_test_cloud_home_caller_driven(cloud_home, cipher)
            .await?;
        self.sync_status.publish();
        Ok(())
    }

    /// Set the cloud home's storage mode in config, so a test can exercise the
    /// browsable read/write paths against an injected cloud home (production sets
    /// this through the cloud-setup wizard). The connected home's cipher must match
    /// — `Plaintext` for browsable, `Encrypted` for opaque.
    #[cfg(any(test, feature = "test-utils"))]
    pub fn set_home_storage(&self, storage: crate::config::HomeStorage) {
        self.config_handle
            .update(|c| c.cloud_home.storage = storage)
            .expect("set test home storage mode");
    }

    /// The cloud object key the read path resolves for a remote file: the row's
    /// stored `cloud_path` on a browsable home, the hashed-by-id key on an opaque
    /// one. Exposed so a test can assert the read key matches the stored upload key
    /// without setting up full playback.
    #[cfg(any(test, feature = "test-utils"))]
    pub async fn resolve_track_cloud_key_for_test(&self, file_id: &str) -> String {
        let blob = self
            .release_file_row_blob_ref(file_id)
            .await
            .expect("blob ref");
        self.database
            .blob_cloud_key(blob.blob())
            .expect("cloud key")
    }

    /// Read the injected wall clock without handing its service to callers.
    #[cfg(not(any(target_os = "ios", target_os = "android")))]
    pub(crate) fn now(&self) -> chrono::DateTime<chrono::Utc> {
        self.clock.now()
    }

    #[cfg(not(any(target_os = "ios", target_os = "android")))]
    pub(crate) fn new_id(&self) -> String {
        self.ids.new_id()
    }

    pub(crate) fn record_telemetry(&self, event: TelemetryEvent) {
        self.diagnostics.event(event);
    }

    #[cfg(not(any(target_os = "ios", target_os = "android")))]
    pub(crate) async fn fetch_remote_image(
        &self,
        url: &str,
    ) -> Result<Option<crate::import::cover_art::RemoteImage>, crate::import::ImportError> {
        self.remote_images.fetch(url).await
    }

    #[cfg(not(any(target_os = "ios", target_os = "android")))]
    pub(crate) async fn fetch_required_remote_image(
        &self,
        url: &str,
    ) -> Result<crate::import::cover_art::RemoteImage, crate::import::ImportError> {
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

    #[cfg(test)]
    pub(crate) fn local_blob_exists_for_test(
        &self,
        namespace: &str,
        blob_id: &str,
    ) -> Result<bool, String> {
        self.config_handle
            .local_blob_exists_for_test(namespace, blob_id)
    }

    #[cfg(test)]
    pub(crate) async fn observe_blob_preparation_started_for_test(&self, file_id: &str) {
        let blob = self
            .database
            .row_blob_ref(crate::sync::RELEASE_FILES_NAMESPACE, file_id)
            .await
            .unwrap();
        coven::BlobTransitionObserver::on_blob_preparation_started(
            self._upload_observer.as_ref(),
            &blob,
        )
        .await;
    }

    #[cfg(test)]
    pub(crate) async fn observe_blob_preparation_progress_for_test(
        &self,
        file_id: &str,
        bytes_done: u64,
        bytes_total: u64,
    ) {
        let blob = self
            .database
            .row_blob_ref(crate::sync::RELEASE_FILES_NAMESPACE, file_id)
            .await
            .unwrap();
        coven::BlobTransitionObserver::on_blob_preparation_progress(
            self._upload_observer.as_ref(),
            &blob,
            bytes_done,
            bytes_total,
        )
        .await;
    }

    /// Subscribe to the sync loop's status and publish the current banner value.
    /// Call once after construction, with a tokio runtime available.
    pub fn start(&self) {
        let outbox = self.database.subscribe_cloud_outbox();
        let sync = self.sync.clone();
        self.spawn_supervised_task("cloud outbox subscription", async move {
            sync.run_cloud_outbox_subscription(outbox).await;
        });

        let mut rx = self.database.subscribe_sync_status();
        let lm = self.clone();
        self.spawn_supervised_task("sync status subscription", async move {
            loop {
                let status = rx.borrow_and_update().clone();
                // A cycle in progress (CheckingStorage / Publishing) shows the
                // spinner; a terminal status ends it. `SyncStatusUpdate` holds
                // what the status says about the rest of the banner.
                let syncing = matches!(
                    status,
                    SyncLoopStatus::CheckingStorage | SyncLoopStatus::Publishing
                );
                let SyncStatusUpdate {
                    error: error_update,
                    last_sync_time: last_sync_update,
                    blocked: blocked_update,
                } = SyncStatusUpdate::from_loop_status(&status);
                let (new_failure, newly_blocked) = lm.sync_status.apply(|state| {
                    let mut changed = false;
                    let mut new_failure = false;
                    let mut newly_blocked: Option<usize> = None;
                    if let Some(error) = error_update {
                        if error != state.error {
                            state.error = error.clone();
                            new_failure = error.is_some();
                            changed = true;
                        }
                    }
                    if let Some(blocked) = blocked_update {
                        if blocked != state.blocked {
                            newly_blocked = (!blocked.is_empty()).then_some(blocked.len());
                            state.blocked = blocked;
                            changed = true;
                        }
                    }
                    if syncing != state.syncing {
                        state.syncing = syncing;
                        changed = true;
                    }
                    if let Some(raw) = last_sync_update {
                        if state.last_sync_time_raw.as_deref() != Some(raw.as_str()) {
                            match crate::util::time::rfc3339_to_epoch_millis(&raw) {
                                Ok(ms) => {
                                    state.last_sync_time_raw = Some(raw);
                                    state.last_sync_time = Some(ms);
                                    changed = true;
                                }
                                Err(e) => {
                                    let message =
                                        format!("unparseable last_sync_time {raw:?}: {e}");
                                    warn!("{message}");
                                    state.error = Some(crate::ui::UiError::internal(message));
                                    new_failure = true;
                                    changed = true;
                                }
                            }
                        }
                    }
                    (changed, (new_failure, newly_blocked))
                });
                // Blocked operations wait on a person indefinitely, and the UI
                // reports them as one localized line — without this record the
                // reason coven gave lives nowhere a log reader can find it.
                if let Some(count) = newly_blocked {
                    warn!("sync cycle left {count} operation(s) waiting on a decision");
                }
                if new_failure {
                    let provider = lm.config_handle.config().cloud_home.provider.clone();
                    if let Some(provider) = provider {
                        lm.diagnostics.event(TelemetryEvent::SyncFailed {
                            provider,
                            operation: SyncOperation::Cycle,
                        });
                    }
                }
                if rx.changed().await.is_err() {
                    break;
                }
            }
        });
    }

    pub fn subscribe_events(&self) -> broadcast::Receiver<LibraryEvent> {
        self.event_tx.subscribe()
    }

    pub fn subscribe_sync_status_values(
        &self,
    ) -> tokio::sync::watch::Receiver<crate::library::SyncStatusSnapshot> {
        self.sync_status.subscribe()
    }

    /// Emit a library event to all subscribers. Logs at warn-level when no
    /// subscribers remain — the bus is alive for the lifetime of the library
    /// so empty subscribers is unusual and worth a trace.
    pub(super) fn emit(&self, event: LibraryEvent) {
        if let Err(err) = self.event_tx.send(event) {
            warn!("library event broadcast had no subscribers: {err}");
        }
    }

    /// Build and publish the current outbox snapshot. Called by durable outbox
    /// and display-row subscriptions and by each upload lifecycle callback.
    pub(crate) async fn emit_outbox_changed(&self) -> u64 {
        self.sync.emit_outbox_changed().await
    }

    pub fn subscribe_outbox_values(
        &self,
    ) -> tokio::sync::watch::Receiver<Option<Result<crate::library::OutboxSnapshot, String>>> {
        self.sync.subscribe_outbox_values()
    }

    /// The current download-queue snapshot — per-release state and a
    /// pre-formatted summary. Seeds the Downloads pane before the first value
    /// arrives on the stream.
    pub fn download_snapshot(&self) -> crate::library::DownloadSnapshot {
        self.downloads.snapshot()
    }

    pub fn subscribe_download_values(
        &self,
    ) -> tokio::sync::watch::Receiver<crate::library::DownloadSnapshot> {
        self.downloads.subscribe()
    }

    /// The current export-queue snapshot — per-release state. Seeds the
    /// Exporting pane before the first value arrives on the stream.
    #[cfg(not(any(target_os = "ios", target_os = "android")))]
    pub fn output_snapshot(&self) -> crate::library::OutputSnapshot {
        self.outputs.snapshot()
    }

    #[cfg(not(any(target_os = "ios", target_os = "android")))]
    pub fn subscribe_output_values(
        &self,
    ) -> tokio::sync::watch::Receiver<crate::library::OutputSnapshot> {
        self.outputs.subscribe()
    }

    /// The current outbox processing snapshot — queue depth, per-item state, and
    /// a pre-formatted summary. Seeds the Storage Manager panel before the first
    /// `OutboxChanged` event arrives.
    pub async fn outbox_snapshot(&self) -> Result<crate::library::OutboxSnapshot, LibraryError> {
        self.sync.outbox_snapshot().await
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
