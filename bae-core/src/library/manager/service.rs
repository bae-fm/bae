use super::*;

impl LibraryManager {
    /// Open coven through the top-level builder and create the library manager
    /// over the resulting handle.
    #[allow(clippy::too_many_arguments)]
    pub fn open(
        config_handle: Arc<ConfigHandle>,
        key_service: StoreKeys,
        clock: ClockRef,
        ids: IdRef,
        diagnostics: Diagnostics,
        runtime_handle: tokio::runtime::Handle,
        cloudkit_ops: Option<Arc<dyn coven::CloudKitOps>>,
        remote_images: crate::import::cover_art::RemoteImageCache,
    ) -> Result<Self, coven::DbError> {
        let (event_tx, _) = broadcast::channel(LIBRARY_EVENT_CHANNEL_CAPACITY);
        let outbox_in_flight = Arc::new(Mutex::new(HashMap::new()));
        let upload_sessions = Arc::new(crate::library::UploadSessions::new());
        let upload_throughput = Arc::new(crate::library::UploadThroughput::new());
        let sync_paused = Arc::new(std::sync::atomic::AtomicBool::new(false));

        let (observer, observer_events) = crate::sync::upload_observer::ReleaseUploadObserver::new(
            outbox_in_flight.clone(),
            upload_throughput.clone(),
            sync_paused.clone(),
        );
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
                coven::CovenError::Database(error) => error,
                other => coven::DbError::Message(other.to_string()),
            })?;
        let database = Database::from_handle(handle.clone(), clock.clone(), ids.clone());
        let sync_status = Arc::new(Mutex::new(SyncStatusState::initial(&database)));

        let sync = SyncController::new(
            config_handle.clone(),
            key_service.clone(),
            clock.clone(),
            event_tx.clone(),
            database.clone(),
            outbox_in_flight,
            upload_sessions,
            upload_throughput,
            sync_paused,
            cloudkit_ops,
            diagnostics.clone(),
        );

        let manager = LibraryManager {
            database,
            config_handle,
            key_service,
            remote_images,
            clock,
            ids,
            diagnostics,
            runtime_handle,
            event_tx,
            sync,
            sync_status,
            transfer_cancels: Arc::new(Mutex::new(HashMap::new())),
            transfer_actions: Arc::new(Mutex::new(HashMap::new())),
            download_queue: Arc::new(crate::library::DownloadQueue::new()),
            #[cfg(not(any(target_os = "ios", target_os = "android")))]
            output_queue: Arc::new(crate::library::OutputQueue::new()),
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
        key_service: StoreKeys,
        clock: ClockRef,
        ids: IdRef,
        diagnostics: Diagnostics,
        runtime_handle: tokio::runtime::Handle,
        remote_images: crate::import::cover_art::RemoteImageCache,
    ) -> Self {
        let (event_tx, _) = broadcast::channel(LIBRARY_EVENT_CHANNEL_CAPACITY);
        let outbox_in_flight = Arc::new(Mutex::new(HashMap::new()));
        let upload_sessions = Arc::new(crate::library::UploadSessions::new());
        let upload_throughput = Arc::new(crate::library::UploadThroughput::new());
        let sync_paused = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let (observer, observer_events) = crate::sync::upload_observer::ReleaseUploadObserver::new(
            outbox_in_flight.clone(),
            upload_throughput.clone(),
            sync_paused.clone(),
        );

        let sync = SyncController::new(
            config_handle.clone(),
            key_service.clone(),
            clock.clone(),
            event_tx.clone(),
            database.clone(),
            outbox_in_flight,
            upload_sessions,
            upload_throughput,
            sync_paused,
            None,
            diagnostics.clone(),
        );
        let sync_status = Arc::new(Mutex::new(SyncStatusState::initial(&database)));

        let manager = LibraryManager {
            database,
            config_handle,
            key_service,
            remote_images,
            clock,
            ids,
            diagnostics,
            runtime_handle,
            event_tx,
            sync,
            sync_status,
            transfer_cancels: Arc::new(Mutex::new(HashMap::new())),
            transfer_actions: Arc::new(Mutex::new(HashMap::new())),
            download_queue: Arc::new(crate::library::DownloadQueue::new()),
            #[cfg(not(any(target_os = "ios", target_os = "android")))]
            output_queue: Arc::new(crate::library::OutputQueue::new()),
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
        self.runtime_handle.spawn(async move {
            events
                .run(move |event| {
                    let sync = sync.clone();
                    async move {
                        sync.process_upload_observer_event(event).await;
                    }
                })
                .await;
        });
    }

    pub(super) fn start_queue_workers(&self) {
        self.spawn_queue_worker(|manager| async move {
            manager.run_download_worker().await;
        });
        #[cfg(not(any(target_os = "ios", target_os = "android")))]
        self.spawn_queue_worker(|manager| async move {
            manager.run_output_worker().await;
        });
    }

    pub(super) fn spawn_queue_worker<F, Fut>(&self, worker: F)
    where
        F: FnOnce(Self) -> Fut + Send + 'static,
        Fut: Future<Output = ()> + Send + 'static,
    {
        let manager = self.clone();
        self.runtime_handle.spawn(worker(manager));
    }

    pub(crate) fn current_transfer_action(&self, release_id: &str) -> Option<ReleaseStorageAction> {
        self.transfer_actions
            .lock()
            .unwrap()
            .get(release_id)
            .copied()
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
        self.sync.connect_test_cloud_home(cloud_home, cipher).await
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
            .await
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
    pub(crate) async fn observe_blob_upload_started_for_test(&self, file_id: &str) {
        coven::BlobTransitionObserver::on_blob_upload_started(
            self._upload_observer.as_ref(),
            file_id,
        )
        .await;
    }

    #[cfg(test)]
    pub(crate) async fn observe_blob_upload_progress_for_test(
        &self,
        file_id: &str,
        bytes_done: u64,
        bytes_total: u64,
    ) {
        coven::BlobTransitionObserver::on_blob_upload_progress(
            self._upload_observer.as_ref(),
            file_id,
            bytes_done,
            bytes_total,
        )
        .await;
    }

    #[cfg(test)]
    pub(crate) async fn observe_blob_uploaded_for_test(&self, file_id: &str) {
        coven::BlobTransitionObserver::on_blob_uploaded(self._upload_observer.as_ref(), file_id)
            .await;
    }

    #[cfg(test)]
    pub(crate) async fn observe_root_made_remote_for_test(&self, root_table: &str, root_id: &str) {
        coven::BlobTransitionObserver::on_root_made_remote(
            self._upload_observer.as_ref(),
            root_table,
            root_id,
        )
        .await;
    }

    #[cfg(test)]
    pub(crate) async fn observe_root_made_local_for_test(&self, root_table: &str, root_id: &str) {
        coven::BlobTransitionObserver::on_root_made_local(
            self._upload_observer.as_ref(),
            root_table,
            root_id,
        )
        .await;
    }

    /// Subscribe to the sync loop's status and turn it into library events: the
    /// banner state, and granular entity events for the rows an applied changeset
    /// touched. Call once after construction, with a tokio runtime available.
    pub fn start(&self) {
        let mut rx = self.database.subscribe_sync_status();
        let lm = self.clone();
        self.runtime_handle.spawn(async move {
            loop {
                let status = rx.borrow_and_update().clone();
                // Row changes ride on a cycle that reached storage and changed
                // data (Synchronized, or Blocked with writes still applied); they
                // are a refresh hint, so the entity events they drive are re-reads
                // by primary key, not a trusted stream.
                if let SyncLoopStatus::Synchronized(success)
                | SyncLoopStatus::Blocked { success, .. } = &status
                {
                    if let Some(row_changes) = &success.row_changes {
                        let (changes, missing_fk) =
                            crate::library::sync_events::changes_from_row_changes(row_changes);
                        for _ in 0..missing_fk {
                            lm.record_telemetry(TelemetryEvent::Anomaly {
                                kind: crate::diagnostics::AnomalyKind::ChangesetMissingFk,
                            });
                        }
                        lm.emit_sync_entity_changes(changes).await;
                    }
                }
                // Fold coven's sync-loop status onto bae's flat banner state: a
                // cycle in progress (CheckingStorage / Publishing) shows the
                // spinner; a terminal status ends it, clearing the banner and
                // recording the sync time on Synchronized/Blocked, or setting the
                // banner on Failed. `error_update` is `None` when the status has no
                // verdict on the banner (in progress, or Offline).
                let syncing = matches!(
                    status,
                    SyncLoopStatus::CheckingStorage | SyncLoopStatus::Publishing
                );
                let (error_update, last_sync_update): (Option<Option<String>>, Option<String>) =
                    match &status {
                        SyncLoopStatus::CheckingStorage
                        | SyncLoopStatus::Publishing
                        | SyncLoopStatus::Offline => (None, None),
                        SyncLoopStatus::Synchronized(success)
                        | SyncLoopStatus::Blocked { success, .. } => {
                            (Some(None), Some(success.last_sync_time.clone()))
                        }
                        SyncLoopStatus::Failed { error } => (Some(Some(error.clone())), None),
                    };
                let mut emit_error = None;
                let mut emit_syncing = None;
                let mut emit_time = None;
                {
                    let mut state = lm.sync_status.lock().unwrap();
                    if let Some(error) = error_update {
                        if error != state.error {
                            state.error = error.clone();
                            emit_error = Some(error.map(crate::ui::UiError::internal));
                        }
                    }
                    if syncing != state.syncing {
                        state.syncing = syncing;
                        emit_syncing = Some(syncing);
                    }
                    if let Some(raw) = last_sync_update {
                        if state.last_sync_time_raw.as_deref() != Some(raw.as_str()) {
                            match crate::util::time::rfc3339_to_epoch_millis(&raw) {
                                Ok(ms) => {
                                    state.last_sync_time_raw = Some(raw);
                                    state.last_sync_time = Some(ms);
                                    emit_time = Some(state.last_sync_time);
                                }
                                Err(e) => {
                                    let message =
                                        format!("unparseable last_sync_time {raw:?}: {e}");
                                    warn!("{message}");
                                    emit_error = Some(Some(crate::ui::UiError::internal(message)));
                                }
                            }
                        }
                    }
                }
                if let Some(error) = emit_error {
                    // A newly-set banner (inner `Some`) is a sync-cycle failure;
                    // clearing it (inner `None`) is not. Ship the typed event
                    // only for an actual failure, and only when a provider is
                    // configured — a sync failure with no provider can't happen,
                    // so its absence means don't fabricate one.
                    if error.is_some() {
                        let provider = lm.config_handle.config().cloud_home.provider.clone();
                        if let Some(provider) = provider {
                            lm.diagnostics.event(TelemetryEvent::SyncFailed {
                                provider,
                                operation: SyncOperation::Cycle,
                            });
                        }
                    }
                    // coven's error string is opaque (connectivity, auth, storage);
                    // the UI shows a generic line plus this as copyable, log-only
                    // detail. `None` clears the banner.
                    lm.emit(LibraryEvent::SyncError { error });
                }
                if let Some(syncing) = emit_syncing {
                    lm.emit(LibraryEvent::SyncingChanged { syncing });
                }
                if let Some(time) = emit_time {
                    lm.emit(LibraryEvent::SyncTimeChanged { time });
                }
                // coven gives no per-item drain signal in the status, so re-derive
                // the outbox snapshot each cycle to catch what it uploaded or failed.
                lm.emit_outbox_changed().await;
                if rx.changed().await.is_err() {
                    break;
                }
            }
        });
    }

    pub fn subscribe_events(&self) -> broadcast::Receiver<LibraryEvent> {
        self.event_tx.subscribe()
    }

    /// Emit a library event to all subscribers. Logs at warn-level when no
    /// subscribers remain — the bus is alive for the lifetime of the library
    /// so empty subscribers is unusual and worth a trace.
    pub(super) fn emit(&self, event: LibraryEvent) {
        if let Err(err) = self.event_tx.send(event) {
            warn!("library event broadcast had no subscribers: {err}");
        }
    }

    /// Build the current outbox snapshot and emit it as `OutboxChanged`. Called
    /// at every outbox mutation, once per sync cycle, and on each upload
    /// lifecycle callback so the Storage Manager's queue panel stays current.
    pub(crate) async fn emit_outbox_changed(&self) {
        self.sync.emit_outbox_changed().await
    }

    /// Build the current download-queue snapshot and emit it as
    /// `DownloadQueueChanged`. Called at every queue mutation (enqueue,
    /// worker pick-up, per-file progress, success, failure, cancel, retry,
    /// pause/resume) so the Storage Manager's Downloads pane stays current.
    pub(crate) fn emit_download_queue_changed(&self) {
        self.emit(LibraryEvent::DownloadQueueChanged {
            snapshot: self.download_snapshot(),
        });
    }

    /// The current download-queue snapshot — per-release state and a
    /// pre-formatted summary, built from the in-memory queue. Seeds the
    /// Downloads pane before the first `DownloadQueueChanged` event arrives.
    pub fn download_snapshot(&self) -> crate::library::DownloadSnapshot {
        crate::library::download_snapshot::build_download_snapshot(
            &self.download_queue.ops(),
            self.download_queue.is_paused(),
        )
    }

    /// Build the current export-queue snapshot and emit it as
    /// `OutputQueueChanged`. Called at every queue mutation (enqueue, worker
    /// pick-up, per-file progress, success, failure, cancel, retry,
    /// pause/resume) so the Storage Manager's Exporting pane stays current.
    #[cfg(not(any(target_os = "ios", target_os = "android")))]
    pub(crate) fn emit_output_queue_changed(&self) {
        self.emit(LibraryEvent::OutputQueueChanged {
            snapshot: self.output_snapshot(),
        });
    }

    /// The current export-queue snapshot — per-release state built from the
    /// in-memory queue. Seeds the Exporting pane before the first
    /// `OutputQueueChanged` event arrives.
    #[cfg(not(any(target_os = "ios", target_os = "android")))]
    pub fn output_snapshot(&self) -> crate::library::OutputSnapshot {
        crate::library::output_snapshot::build_output_snapshot(
            &self.output_queue.ops(),
            self.output_queue.is_paused(),
        )
    }

    /// The current outbox processing snapshot — queue depth, per-item state, and
    /// a pre-formatted summary. Seeds the Storage Manager panel before the first
    /// `OutboxChanged` event arrives.
    pub async fn outbox_snapshot(&self) -> Result<crate::library::OutboxSnapshot, LibraryError> {
        self.sync.outbox_snapshot().await
    }

    // ── Fat-event emit helpers ───────────────────────────────────────
    // Each reads the current state of the entity post-mutation from the DB
    // and packs it into the event payload.

    pub async fn emit_album_added(&self, album_id: &str) {
        match self.find_album_detail(album_id).await {
            Ok(Some(album)) => {
                self.emit(LibraryEvent::AlbumAdded { album });
            }
            Ok(None) => {
                warn!("emit_album_added: album {album_id} not found in DB, skipping event");
            }
            Err(e) => {
                warn!("emit_album_added: DB error for album {album_id}: {e}");
            }
        }
    }

    pub async fn emit_album_updated(&self, album_id: &str) {
        match self.find_album_detail(album_id).await {
            Ok(Some(album)) => self.emit(LibraryEvent::AlbumUpdated { album }),
            Ok(None) => {
                warn!("emit_album_updated: album {album_id} not found in DB, skipping event");
            }
            Err(e) => {
                warn!("emit_album_updated: DB error for album {album_id}: {e}");
            }
        }
    }

    /// Re-emit `AlbumUpdated` for every album, on each cloud-home transition. A
    /// release's available storage actions are computed at resolve time from
    /// whether a cloud home exists and then baked into the cached `ReleaseDetail`,
    /// so connecting or disconnecting one leaves every already-resolved release
    /// holding stale actions until a restart. Re-resolving reads `has_cloud_home()`
    /// fresh. A burst, but connect/disconnect is rare.
    pub async fn emit_all_albums_updated(&self) {
        let albums = match self.get_albums(&[]).await {
            Ok(albums) => albums,
            Err(e) => {
                warn!("emit_all_albums_updated: failed to list albums: {e}");
                return;
            }
        };
        for album in albums {
            self.emit_album_updated(&album.id).await;
        }
    }

    pub fn emit_album_removed(&self, album_id: &str, release_ids: Vec<String>) {
        self.emit(LibraryEvent::AlbumRemoved {
            album_id: album_id.to_string(),
            release_ids,
        });
    }

    pub async fn emit_release_added(&self, album_id: &str, release_id: &str) {
        let release = match self.find_release_detail(release_id).await {
            Ok(Some(release)) => release,
            Ok(None) => {
                warn!("emit_release_added: release {release_id} not found in DB, skipping event");
                return;
            }
            Err(e) => {
                warn!("emit_release_added: DB error for release {release_id}: {e}");
                return;
            }
        };
        let raw_album = match self.database.find_album_summary(album_id).await {
            Ok(Some(raw)) => raw,
            Ok(None) => {
                warn!("emit_release_added: album {album_id} not found in DB, skipping event");
                return;
            }
            Err(e) => {
                warn!("emit_release_added: DB error for album {album_id}: {e}");
                return;
            }
        };
        // The release/album rows are already committed, so the event must fire for
        // them. A cover lookup failure degrades to no covers (the UI lazily fetches
        // them by id) — never drops the event for committed state.
        let covers = self
            .cover_refs(&raw_album.release_ids)
            .await
            .unwrap_or_else(|e| {
                warn!(
                    "emit_release_added: cover lookup failed for album {album_id}: {e}; \
                 emitting without covers"
                );
                HashMap::new()
            });
        let album = AlbumSummary::from_raw(raw_album, |rid| covers.get(rid).cloned());
        self.emit(LibraryEvent::ReleaseAdded { album, release });
    }

    pub async fn emit_release_updated(&self, album_id: &str, release_id: &str) {
        match self.find_release_detail(release_id).await {
            Ok(Some(release)) => {
                self.emit(LibraryEvent::ReleaseUpdated {
                    album_id: album_id.to_string(),
                    release,
                });
            }
            Ok(None) => {
                warn!("emit_release_updated: release {release_id} not found in DB, skipping event");
            }
            Err(e) => {
                warn!("emit_release_updated: DB error for release {release_id}: {e}");
            }
        }
    }

    pub async fn emit_release_removed(&self, album_id: &str, release_id: &str) {
        // Ship the parent album's post-removal summary so the reducer interns it
        // rather than reading the old release list to patch it — a read-to-write
        // that goes stale on the sync path, where no AlbumUpdated co-fires. `None`
        // means strictly "the album itself was removed with its last release"; a
        // transient cover-lookup failure must NOT misreport the album as gone, so
        // it degrades to a summary with no covers (the UI lazily fetches them).
        let album = match self.database.find_album_summary(album_id).await {
            Ok(Some(raw)) => {
                let covers = self.cover_refs(&raw.release_ids).await.unwrap_or_else(|e| {
                    warn!(
                        "emit_release_removed: cover lookup failed for album {album_id}: {e}; \
                         emitting without covers"
                    );
                    HashMap::new()
                });
                Some(AlbumSummary::from_raw(raw, |rid| covers.get(rid).cloned()))
            }
            Ok(None) => None,
            Err(e) => {
                warn!("emit_release_removed: DB error for album {album_id}: {e}");
                None
            }
        };
        self.emit(LibraryEvent::ReleaseRemoved {
            album_id: album_id.to_string(),
            release_id: release_id.to_string(),
            album,
        });
    }

    /// Emit granular library events for the entity changes an applied changeset
    /// produced.
    pub async fn emit_sync_entity_changes(
        &self,
        mut changes: crate::library::sync_events::ChangesetEntityChanges,
    ) {
        use crate::library::sync_events::{AlbumChangeEvent, ReleaseChangeEvent};

        // The changeset couldn't resolve these to albums itself: a track whose
        // release it didn't carry, or a cover whose release it didn't carry (a peer's
        // `change_cover` writes the `covers` row alone). Both become album updates —
        // the album payload carries its releases' tracks and cover refs — deduped
        // against the albums the changeset already produced an event for, so one
        // changeset never updates an album twice.
        let mut seen: std::collections::HashSet<String> = changes
            .album_events
            .iter()
            .map(|event| match event {
                AlbumChangeEvent::Added(id) | AlbumChangeEvent::Updated(id) => id.clone(),
                AlbumChangeEvent::Removed { album_id, .. } => album_id.clone(),
            })
            .collect();

        if !changes.unresolved_track_ids.is_empty() {
            match self
                .database
                .get_album_ids_for_tracks(&changes.unresolved_track_ids)
                .await
            {
                Ok(resolved) => {
                    for album_id in resolved.values() {
                        if seen.insert(album_id.clone()) {
                            changes
                                .album_events
                                .push(AlbumChangeEvent::Updated(album_id.clone()));
                        }
                    }
                }
                Err(e) => {
                    tracing::warn!("Failed to resolve track IDs to album IDs: {e}");
                }
            }
        }

        if !changes.unresolved_release_ids.is_empty() {
            match self
                .database
                .get_album_ids_for_releases(&changes.unresolved_release_ids)
                .await
            {
                Ok(resolved) => {
                    for album_id in resolved.values() {
                        if seen.insert(album_id.clone()) {
                            changes
                                .album_events
                                .push(AlbumChangeEvent::Updated(album_id.clone()));
                        }
                    }
                }
                Err(e) => {
                    tracing::warn!("Failed to resolve release IDs to album IDs: {e}");
                }
            }
        }

        for event in changes.album_events {
            match event {
                AlbumChangeEvent::Added(id) => self.emit_album_added(&id).await,
                AlbumChangeEvent::Updated(id) => self.emit_album_updated(&id).await,
                AlbumChangeEvent::Removed {
                    album_id,
                    release_ids,
                } => self.emit_album_removed(&album_id, release_ids),
            }
        }
        for event in changes.release_events {
            match event {
                ReleaseChangeEvent::Added {
                    album_id,
                    release_id,
                } => self.emit_release_added(&album_id, &release_id).await,
                ReleaseChangeEvent::Updated {
                    album_id,
                    release_id,
                } => self.emit_release_updated(&album_id, &release_id).await,
                ReleaseChangeEvent::Removed {
                    album_id,
                    release_id,
                } => self.emit_release_removed(&album_id, &release_id).await,
            }
        }
    }
}
