use super::manager::LibraryManager;
#[cfg(not(any(target_os = "ios", target_os = "android")))]
use crate::import::IdentificationHandle;
#[cfg(not(any(target_os = "ios", target_os = "android")))]
use crate::import::ImportServiceHandle;
use crate::playback::PlaybackHandle;
use std::sync::Arc;

macro_rules! delegate_sync {
    ($field:ident, $name:ident => $target:ident($($arg:ident: $ty:ty),* $(,)?) -> $ret:ty) => {
        pub fn $name(&self, $($arg: $ty),*) -> $ret {
            self.inner.$field.$target($($arg),*)
        }
    };
}

macro_rules! delegate_async {
    ($field:ident, $name:ident => $target:ident($($arg:ident: $ty:ty),* $(,)?) -> $ret:ty) => {
        pub async fn $name(&self, $($arg: $ty),*) -> $ret {
            self.inner.$field.$target($($arg),*).await
        }
    };
}

struct AppServicesInner {
    manager: LibraryManager,
    playback: PlaybackHandle,
    /// The import service, which also owns the identify driver and the
    /// extraction feeding it: one of each per library, reached through it.
    #[cfg(not(any(target_os = "ios", target_os = "android")))]
    import: ImportServiceHandle,
    /// Queue-wide identification. Built here rather than handed in, so that a
    /// library cannot exist without one: the queue runs whether or not anyone
    /// has the Import section open, and opening a view is not what starts it.
    #[cfg(not(any(target_os = "ios", target_os = "android")))]
    identification: IdentificationHandle,
}

impl Drop for AppServicesInner {
    /// Stop and join the playback and import worker threads when the last
    /// `AppServices` clone drops — i.e. when the app is torn down. Each runs on
    /// its own OS thread and, because a live command sender sits in this very
    /// struct, only stops on an explicit `Shutdown`; nothing else joins them.
    /// Until a thread exits it holds a `LibraryManager` clone, and through the
    /// shared coven handle that pins the store's exclusive open lock — so
    /// without joining *every* such thread the same library can't be reopened
    /// in-process. No-ops if `shutdown` already ran: each join handle is taken
    /// once.
    fn drop(&mut self) {
        self.playback.stop_and_join();
        #[cfg(not(any(target_os = "ios", target_os = "android")))]
        {
            // Before the import worker's join: the queue's in-flight
            // candidates are cancelled here, and a cancelled candidate writes
            // no row.
            self.identification.stop();
            self.import.stop_and_join();
        }
    }
}

/// The running application: library data layer + all service handles.
#[derive(Clone)]
pub struct AppServices {
    inner: Arc<AppServicesInner>,
}

pub struct StorageProjectionValue {
    pub page: crate::album_detail::StoragePage,
    pub total_size: u64,
}

/// Replace the Sync-queue filter's absolute release set only when durable
/// membership changed. Byte-progress snapshots keep the same IDs and must not
/// rebuild the database page subscription at buffer cadence.
fn replace_transitioning_release_ids(current: &mut Vec<String>, mut next: Vec<String>) -> bool {
    next.sort();
    next.dedup();
    if *current == next {
        return false;
    }
    *current = next;
    true
}

/// Each requested window of `projection`'s upcoming tail with the entries in
/// it, clamped to the tail's end.
fn upcoming_slices<'a>(
    projection: &'a crate::playback::PlaybackQueueProjection,
    requested: &crate::library::LibraryPageWindows,
) -> Vec<(
    crate::library::LibraryPageWindow,
    &'a [crate::playback::QueueEntry],
)> {
    let tail = projection
        .context
        .as_ref()
        .map(|context| context.upcoming.as_slice())
        .unwrap_or(&[]);
    requested
        .iter()
        .map(|window| {
            (
                window.clone(),
                crate::queue::clamp_upcoming_page(tail, window.offset, window.limit),
            )
        })
        .collect()
}

impl std::fmt::Debug for AppServices {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AppServices")
            .field("manager", &self.inner.manager)
            .finish_non_exhaustive()
    }
}

impl AppServices {
    pub fn new(
        manager: LibraryManager,
        playback: PlaybackHandle,
        #[cfg(not(any(target_os = "ios", target_os = "android")))] import: ImportServiceHandle,
    ) -> Self {
        #[cfg(not(any(target_os = "ios", target_os = "android")))]
        let identification = crate::import::identification::start(import.clone(), manager.clone());
        AppServices {
            inner: Arc::new(AppServicesInner {
                manager,
                playback,
                #[cfg(not(any(target_os = "ios", target_os = "android")))]
                import,
                #[cfg(not(any(target_os = "ios", target_os = "android")))]
                identification,
            }),
        }
    }

    #[cfg(all(
        feature = "test-utils",
        not(any(target_os = "ios", target_os = "android"))
    ))]
    pub async fn for_test(manager: LibraryManager) -> Result<Self, crate::import::ImportError> {
        let playback = manager.start_playback_service_with_audio_device(
            tokio::runtime::Handle::current(),
            50,
            false,
            Box::new(crate::playback::audio_output::FailingAudioDevice),
        );
        let import = manager
            .start_import_service(tokio::runtime::Handle::current())
            .await?;
        Ok(Self::new(manager, playback, import))
    }

    pub fn subscribe_config_changes(&self) -> tokio::sync::watch::Receiver<crate::config::Config> {
        self.inner.manager.subscribe_config_changes()
    }

    pub fn subscribe_album_browse(
        &self,
        sort: &[crate::db::AlbumSortCriterion],
    ) -> crate::library::AlbumBrowseSubscription {
        let manager = self.inner.manager.clone();
        let query = manager.subscribe_album_browse(sort, std::collections::BTreeSet::new());
        crate::library::LibraryBrowseSubscription::new(
            query,
            move |projection, request_revision, cause| {
                manager.resolve_album_browse(projection, request_revision, cause)
            },
        )
    }

    /// The summaries of the albums the grid has selected, as one live query
    /// whose ids move in place as the selection changes; it starts with none.
    pub fn subscribe_album_selection(&self) -> crate::library::AlbumSelectionSubscription {
        let manager = self.inner.manager.clone();
        let query = manager.subscribe_album_selection(std::collections::BTreeSet::new());
        crate::library::AlbumSelectionSubscription::new(query, move |projection| {
            manager.resolve_album_selection(projection)
        })
    }

    /// The album's detail as it changes: its rows, the releases' pin markers
    /// coven watches, and the config, cloud-home, and transfer state it is
    /// resolved against.
    pub fn subscribe_album_detail_values(
        &self,
        runtime_handle: &tokio::runtime::Handle,
        album_id: String,
    ) -> tokio::sync::mpsc::UnboundedReceiver<
        Result<Option<crate::album_detail::AlbumDetail>, crate::library::LibraryError>,
    > {
        let (tx, rx) = tokio::sync::mpsc::unbounded_channel();
        let services = self.clone();
        let manager = services.inner.manager.clone();
        let mut query =
            live_query_events(runtime_handle, manager.subscribe_album_detail(&album_id));
        let mut pins = manager.watch_release_pins();
        let mut config = services.subscribe_config_changes();
        let mut cloud_home = manager.subscribe_cloud_home();
        let mut transfers = services.subscribe_transfer_values();
        runtime_handle.spawn(async move {
            let mut last: Option<(crate::db::AlbumDetailProjection, Vec<bool>)> = None;
            loop {
                let value = tokio::select! {
                    event = query.recv() => match event {
                        None => return,
                        Some(Ok(projection)) => {
                            match pins.watch(LibraryManager::album_detail_pin_files(&projection)).await {
                                Ok(pinned) => {
                                    last = Some((projection.clone(), pinned.clone()));
                                    manager.resolve_album_detail_projection(projection, pinned)
                                }
                                Err(error) => Err(error),
                            }
                        }
                        Some(Err(error)) => Err(error),
                    },
                    answer = pins.changed() => match (answer, last.as_mut()) {
                        (Ok(pinned), Some((projection, last_pinned))) => {
                            *last_pinned = pinned.clone();
                            manager.resolve_album_detail_projection(projection.clone(), pinned)
                        }
                        (Ok(_), None) => continue,
                        (Err(error), _) => Err(error),
                    },
                    changed = async { tokio::select! {
                        value = config.changed() => value,
                        value = cloud_home.changed() => value,
                        value = transfers.changed() => value,
                    }} => {
                        if changed.is_err() { return; }
                        config.borrow_and_update();
                        cloud_home.borrow_and_update();
                        transfers.borrow_and_update();
                        let Some((projection, pinned)) = last.clone() else { continue };
                        manager.resolve_album_detail_projection(projection, pinned)
                    }
                };
                if tx.send(value).is_err() { return; }
            }
        });
        rx
    }

    /// One release's detail as it changes: its rows, its pin marker coven
    /// watches, and the config, cloud-home, and transfer state it is resolved
    /// against.
    pub fn subscribe_release_detail_values(
        &self,
        runtime_handle: &tokio::runtime::Handle,
        release_id: String,
    ) -> tokio::sync::mpsc::UnboundedReceiver<
        Result<Option<crate::album_detail::ReleaseDetail>, crate::library::LibraryError>,
    > {
        let (tx, rx) = tokio::sync::mpsc::unbounded_channel();
        let services = self.clone();
        let manager = services.inner.manager.clone();
        let mut query = live_query_events(
            runtime_handle,
            manager.subscribe_release_detail(&release_id),
        );
        let mut pins = manager.watch_release_pins();
        let mut config = services.subscribe_config_changes();
        let mut cloud_home = manager.subscribe_cloud_home();
        let mut transfers = services.subscribe_transfer_values();
        runtime_handle.spawn(async move {
            let mut last: Option<(crate::db::ReleaseDetailProjection, bool)> = None;
            loop {
                let value = tokio::select! {
                    event = query.recv() => match event {
                        None => return,
                        Some(Ok(projection)) => {
                            match pins.watch(LibraryManager::release_detail_pin_files(&projection)).await {
                                Ok(pinned) => {
                                    let pinned = pinned.first().copied().unwrap_or(false);
                                    last = Some((projection.clone(), pinned));
                                    manager.resolve_release_detail_projection(&release_id, projection, pinned)
                                }
                                Err(error) => Err(error),
                            }
                        }
                        Some(Err(error)) => Err(error),
                    },
                    answer = pins.changed() => match (answer, last.as_mut()) {
                        (Ok(pinned), Some((projection, last_pinned))) => {
                            *last_pinned = pinned.first().copied().unwrap_or(false);
                            manager.resolve_release_detail_projection(&release_id, projection.clone(), *last_pinned)
                        }
                        (Ok(_), None) => continue,
                        (Err(error), _) => Err(error),
                    },
                    changed = async { tokio::select! {
                        value = config.changed() => value,
                        value = cloud_home.changed() => value,
                        value = transfers.changed() => value,
                    }} => {
                        if changed.is_err() { return; }
                        config.borrow_and_update();
                        cloud_home.borrow_and_update();
                        transfers.borrow_and_update();
                        let Some((projection, pinned)) = last.clone() else { continue };
                        manager.resolve_release_detail_projection(&release_id, projection, pinned)
                    }
                };
                if tx.send(value).is_err() { return; }
            }
        });
        rx
    }

    /// One live library search, pointed at a new query in place as the
    /// person types; it starts with no query.
    pub fn subscribe_library_search(&self) -> crate::library::LibrarySearchSubscription {
        let manager = self.inner.manager.clone();
        let query = manager.subscribe_library_search(None);
        crate::library::LibrarySearchSubscription::new(query, move |projection| {
            manager.resolve_library_search_projection(projection)
        })
    }

    /// The library membership of the releases an import pane offers, as one
    /// live query whose checks move in place as the pane's offers change; it
    /// starts with none.
    pub fn subscribe_library_statuses(&self) -> crate::library::LibraryStatusSubscription {
        crate::library::LibraryStatusSubscription::new(
            self.inner
                .manager
                .subscribe_library_statuses(std::collections::BTreeSet::new()),
        )
    }

    /// A storage page as it changes: its rows, the rows' pin markers coven
    /// watches, the outbox's transitioning releases (for the Uploading
    /// filter), and the config, cloud-home, download, and transfer state it is
    /// resolved against.
    pub fn subscribe_storage_values(
        &self,
        runtime_handle: &tokio::runtime::Handle,
        sort: crate::db::StorageSortCriterion,
        filter: crate::db::StorageFilter,
        offset: u64,
        limit: u64,
    ) -> tokio::sync::mpsc::UnboundedReceiver<
        Result<StorageProjectionValue, crate::library::LibraryError>,
    > {
        let (tx, rx) = tokio::sync::mpsc::unbounded_channel();
        let services = self.clone();
        let manager = services.inner.manager.clone();
        let query_runtime = runtime_handle.clone();
        let mut outbox = services.subscribe_outbox_values();
        let mut cloud_home = manager.subscribe_cloud_home();
        let mut config = services.subscribe_config_changes();
        let mut downloads = services.subscribe_download_values();
        let mut transfers = services.subscribe_transfer_values();
        let mut pins = manager.watch_release_pins();
        let resolve = move |projection, pinned| {
            let (page, total_size) = manager.resolve_storage_page_projection(projection, pinned);
            StorageProjectionValue { page, total_size }
        };
        runtime_handle.spawn(async move {
            let mut transitioning = if filter == crate::db::StorageFilter::Uploading {
                let current = { outbox.borrow_and_update().clone() };
                match current {
                    Some(Ok(snapshot)) => snapshot.transitioning_release_ids(),
                    Some(Err(error)) => {
                        let _ = tx.send(Err(crate::library::LibraryError::Internal(error)));
                        return;
                    }
                    None => match services.outbox_snapshot().await {
                        Ok(snapshot) => snapshot.transitioning_release_ids(),
                        Err(error) => { let _ = tx.send(Err(error)); return; }
                    },
                }
            } else { Vec::new() };
            let mut query = reconfigurable_live_query_events(
                &query_runtime,
                services.inner.manager.subscribe_storage_page(
                    &sort,
                    filter,
                    transitioning.clone(),
                    offset,
                    limit,
                ),
            );
            let mut last: Option<(crate::db::StoragePageProjection, Vec<bool>)> = None;
            loop {
                let value = tokio::select! {
                    event = query.recv() => match event {
                        None => return,
                        Some(Ok(projection)) => {
                            match pins.watch(LibraryManager::storage_page_pin_files(&projection)).await {
                                Ok(pinned) => {
                                    last = Some((projection.clone(), pinned.clone()));
                                    Ok(resolve(projection, pinned))
                                }
                                Err(error) => Err(error),
                            }
                        }
                        Some(Err(error)) => Err(error),
                    },
                    answer = pins.changed() => match (answer, last.as_mut()) {
                        (Ok(pinned), Some((projection, last_pinned))) => {
                            *last_pinned = pinned.clone();
                            Ok(resolve(projection.clone(), pinned))
                        }
                        (Ok(_), None) => continue,
                        (Err(error), _) => Err(error),
                    },
                    changed = outbox.changed(), if filter == crate::db::StorageFilter::Uploading => {
                        if changed.is_err() { return; }
                        match outbox.borrow_and_update().clone() {
                            Some(Ok(snapshot)) => {
                                let next = snapshot.transitioning_release_ids();
                                if replace_transitioning_release_ids(&mut transitioning, next) {
                                    query.set(transitioning.clone());
                                }
                                continue;
                            }
                            Some(Err(error)) => Err(crate::library::LibraryError::Internal(error)),
                            None => continue,
                        }
                    }
                    changed = async { tokio::select! { value = cloud_home.changed() => value, value = config.changed() => value, value = downloads.changed() => value, value = transfers.changed() => value } } => {
                        if changed.is_err() { return; }
                        cloud_home.borrow_and_update(); config.borrow_and_update(); downloads.borrow_and_update(); transfers.borrow_and_update();
                        let Some((projection, pinned)) = last.clone() else { continue };
                        Ok(resolve(projection, pinned))
                    }
                };
                if tx.send(value).is_err() { return; }
            }
        });
        rx
    }

    pub fn subscribe_artist_browse(
        &self,
        sort: &[crate::db::ArtistSortCriterion],
    ) -> crate::library::ArtistBrowseSubscription {
        let manager = self.inner.manager.clone();
        let query = manager.subscribe_artist_browse(sort, std::collections::BTreeSet::new());
        crate::library::LibraryBrowseSubscription::new(
            query,
            move |projection, request_revision, cause| {
                manager.resolve_artist_browse(projection, request_revision, cause)
            },
        )
    }

    pub fn subscribe_artist_detail(
        &self,
        artist_id: &str,
    ) -> coven::LiveQuery<crate::db::ArtistDetailProjection> {
        self.inner.manager.subscribe_artist_detail(artist_id)
    }

    pub fn resolve_artist_detail_projection(
        &self,
        projection: crate::db::ArtistDetailProjection,
    ) -> Option<crate::album_detail::ArtistDetail> {
        self.inner
            .manager
            .resolve_artist_detail_projection(projection)
    }

    pub fn subscribe_composer_browse(
        &self,
        sort: &[crate::db::ComposerSortCriterion],
    ) -> crate::library::ComposerBrowseSubscription {
        let manager = self.inner.manager.clone();
        let query = manager.subscribe_composer_browse(sort, std::collections::BTreeSet::new());
        crate::library::LibraryBrowseSubscription::new(
            query,
            move |projection, request_revision, cause| {
                manager.resolve_composer_browse(projection, request_revision, cause)
            },
        )
    }

    pub fn subscribe_composer_detail(
        &self,
        artist_id: &str,
    ) -> coven::LiveQuery<crate::db::ComposerDetailProjection> {
        self.inner.manager.subscribe_composer_detail(artist_id)
    }

    pub fn resolve_composer_detail_projection(
        &self,
        projection: crate::db::ComposerDetailProjection,
    ) -> Option<crate::album_detail::ComposerDetail> {
        self.inner
            .manager
            .resolve_composer_detail_projection(projection)
    }

    pub fn subscribe_work_detail(
        &self,
        work_id: &str,
    ) -> coven::LiveQuery<crate::db::WorkDetailProjection> {
        self.inner.manager.subscribe_work_detail(work_id)
    }

    pub fn resolve_work_detail_projection(
        &self,
        projection: crate::db::WorkDetailProjection,
    ) -> Option<crate::album_detail::WorkDetail> {
        self.inner
            .manager
            .resolve_work_detail_projection(projection)
    }

    pub fn subscribe_playback_progress(
        &self,
    ) -> tokio::sync::mpsc::UnboundedReceiver<crate::playback::PlaybackProgress> {
        self.inner.playback.subscribe_progress()
    }

    pub fn subscribe_playback_values(
        &self,
    ) -> tokio::sync::watch::Receiver<crate::playback::PlaybackValues> {
        self.inner.playback.subscribe_values()
    }

    pub fn subscribe_queue_values(
        &self,
        runtime_handle: &tokio::runtime::Handle,
    ) -> tokio::sync::mpsc::UnboundedReceiver<
        Result<crate::queue::ResolvedQueueSnapshot, crate::library::LibraryError>,
    > {
        let (tx, rx) = tokio::sync::mpsc::unbounded_channel();
        let services = self.clone();
        let query_runtime = runtime_handle.clone();
        runtime_handle.spawn(async move {
            let manager = &services.inner.manager;
            let mut queue_values = services.inner.playback.subscribe_queue_values();
            let mut projection = queue_values.borrow_and_update().clone();
            let mut request = crate::library::manager::queue_catalog_request(&projection);
            let mut catalog = reconfigurable_live_query_events(
                &query_runtime,
                manager.subscribe_queue_catalog(request.clone()),
            );
            // The catalog last read for `request`, so a queue change that
            // shows the same tracks — reordered, say — resolves again
            // without another read.
            let mut current = None;
            loop {
                tokio::select! {
                    event = catalog.recv() => {
                        let Some(result) = event else { return };
                        let value = result.map(|read: crate::db::QueueCatalogProjection| {
                            current = Some(read.clone());
                            manager.resolve_queue_catalog(projection.clone(), read)
                        });
                        if tx.send(value).is_err() { return; }
                    }
                    changed = queue_values.changed() => {
                        if changed.is_err() { return; }
                        projection = queue_values.borrow_and_update().clone();
                        let next = crate::library::manager::queue_catalog_request(&projection);
                        if next != request {
                            request = next;
                            current = None;
                            catalog.set(request.clone());
                        } else if let Some(read) = current.clone() {
                            let value = manager.resolve_queue_catalog(projection.clone(), read);
                            if tx.send(Ok(value)).is_err() { return; }
                        }
                    }
                }
            }
        });
        rx
    }

    /// The context's upcoming tail, read in the windows the subscription is
    /// asked for — none until the first [`set_windows`]. One catalog query
    /// serves every window: a new window set or a queue revision points it at
    /// the tracks now in those windows, and one that leaves those tracks
    /// alone resolves the read it already has.
    ///
    /// [`set_windows`]: crate::library::QueueUpcomingSubscription::set_windows
    pub fn subscribe_queue_upcoming(
        &self,
        runtime_handle: &tokio::runtime::Handle,
    ) -> crate::library::QueueUpcomingSubscription {
        let (windows_tx, mut windows) =
            tokio::sync::watch::channel(crate::library::LibraryPageWindows::new());
        let (tx, rx) = tokio::sync::mpsc::unbounded_channel();
        let services = self.clone();
        let query_runtime = runtime_handle.clone();
        let task = runtime_handle.spawn(async move {
            let manager = &services.inner.manager;
            let mut queue_values = services.inner.playback.subscribe_queue_values();
            let mut projection = queue_values.borrow_and_update().clone();
            let mut requested = windows.borrow_and_update().clone();
            let catalog_request =
                |projection: &crate::playback::PlaybackQueueProjection,
                 requested: &crate::library::LibraryPageWindows| {
                    crate::db::QueueCatalogRequest::for_entries(
                        upcoming_slices(projection, requested)
                            .into_iter()
                            .flat_map(|(_, entries)| entries),
                        None,
                    )
                };
            let snapshot = |projection: &crate::playback::PlaybackQueueProjection,
                            requested: &crate::library::LibraryPageWindows,
                            read: &crate::db::QueueCatalogProjection| {
                crate::library::QueueUpcomingSnapshot {
                    revision: projection.revision,
                    windows: upcoming_slices(projection, requested)
                        .into_iter()
                        .map(|(window, entries)| crate::library::QueueUpcomingWindow {
                            window,
                            items: manager.resolve_queue_entries(read, entries),
                        })
                        .collect(),
                }
            };
            let mut request = catalog_request(&projection, &requested);
            let mut catalog = reconfigurable_live_query_events(
                &query_runtime,
                manager.subscribe_queue_catalog(request.clone()),
            );
            // The catalog last read for `request`: a queue revision or a
            // window change that shows the same tracks resolves it again
            // without another read.
            let mut current: Option<crate::db::QueueCatalogProjection> = None;
            loop {
                tokio::select! {
                    event = catalog.recv() => {
                        let Some(result) = event else { return };
                        let value = result.map(|read| {
                            let value = snapshot(&projection, &requested, &read);
                            current = Some(read);
                            value
                        });
                        if tx.send(value).is_err() { return; }
                        continue;
                    }
                    changed = queue_values.changed() => {
                        if changed.is_err() { return; }
                        projection = queue_values.borrow_and_update().clone();
                    }
                    changed = windows.changed() => {
                        if changed.is_err() { return; }
                        requested = windows.borrow_and_update().clone();
                    }
                }
                let next = catalog_request(&projection, &requested);
                if next != request {
                    request = next;
                    current = None;
                    catalog.set(request.clone());
                } else if let Some(read) = current.as_ref() {
                    if tx
                        .send(Ok(snapshot(&projection, &requested, read)))
                        .is_err()
                    {
                        return;
                    }
                }
            }
        });
        crate::library::QueueUpcomingSubscription::new(windows_tx, rx, task)
    }

    #[cfg(not(any(target_os = "ios", target_os = "android")))]
    pub(crate) fn record_telemetry(&self, event: crate::diagnostics::TelemetryEvent) {
        self.inner.manager.record_telemetry(event);
    }

    delegate_sync!(manager, get_config => get_config() -> crate::config::Config);
    delegate_sync!(manager, ensure_mcp_token => ensure_mcp_token() -> Result<String, crate::library::LibraryError>);
    delegate_sync!(manager, set_mcp_token => set_mcp_token(token: String) -> Result<(), crate::library::LibraryError>);
    delegate_sync!(manager, set_mcp_config => set_mcp_config(config: crate::config::McpConfig) -> Result<(), crate::config::ConfigError>);
    delegate_sync!(manager, get_subsonic_password => get_subsonic_password() -> Result<Option<String>, crate::library::LibraryError>);
    delegate_sync!(manager, set_subsonic_password => set_subsonic_password(password: String) -> Result<(), crate::library::LibraryError>);
    delegate_sync!(manager, set_subsonic_config => set_subsonic_config(config: crate::config::SubsonicConfig) -> Result<(), crate::config::ConfigError>);
    delegate_sync!(manager, set_cast_enabled => set_cast_enabled(enabled: bool) -> Result<(), crate::config::ConfigError>);
    delegate_sync!(manager, cloud_home_key_state => cloud_home_key_state() -> Result<coven::CloudHomeKeyState, crate::library::LibraryError>);
    delegate_sync!(manager, set_max_concurrent_uploads => set_max_concurrent_uploads(n: u32) -> Result<(), crate::config::ConfigError>);
    delegate_sync!(manager, set_max_concurrent_downloads => set_max_concurrent_downloads(n: u32) -> Result<(), crate::config::ConfigError>);
    delegate_sync!(manager, set_show_remaining_time => set_show_remaining_time(enabled: bool) -> Result<(), crate::config::ConfigError>);
    delegate_sync!(manager, set_library_full_width => set_library_full_width(enabled: bool) -> Result<(), crate::config::ConfigError>);
    delegate_sync!(manager, set_identify_automatically => set_identify_automatically(enabled: bool) -> Result<(), crate::config::ConfigError>);
    delegate_sync!(manager, set_prefill_with_file_metadata => set_prefill_with_file_metadata(enabled: bool) -> Result<(), crate::config::ConfigError>);

    /// Ask, or stop asking, one metadata source — the switch on the Find online
    /// header and in Settings, which are two views of this one preference.
    ///
    /// Written here rather than delegated, because switching a source off is
    /// not only a preference: everything already asking it has to stop. The
    /// live searches close that source's part, dropping the results it had
    /// found and the answer it still has out; every live identify run is
    /// replaced by one that reads the list as it now stands, so no run is left
    /// waiting on the source nobody is asking. This is the one object that
    /// holds the config, the searches, and the runs, which is why the order
    /// lives here.
    ///
    /// Switching a source *on* re-runs nothing: the next run or search asks it.
    /// Re-dispatching a settled run because a source became available would
    /// throw away an answer the person is reading.
    pub fn set_metadata_source_enabled(
        &self,
        source: crate::import::Catalog,
        enabled: bool,
    ) -> Result<(), crate::config::ConfigError> {
        self.inner
            .manager
            .set_metadata_source_enabled(source, enabled)?;
        #[cfg(not(any(target_os = "ios", target_os = "android")))]
        {
            if !enabled {
                // Searches first: a landing that races the switch then finds a
                // part that has stopped looking, and is dropped.
                self.inner.import.stop_asking_source(source);
                // A run reads the provider list once, at its start, so every
                // run in flight is answering the list as it was. Each is
                // superseded by a run that reads the list as it now is — every
                // one of them, not only the candidate whose pane the switch was
                // flicked on, because a background run waiting on a source
                // nobody asks is exactly as wrong as the open one. A candidate
                // that already settled has no run to supersede; the automatic
                // admission reads those again when its own config watcher
                // fires.
                for key in self.inner.import.identifying_keys() {
                    self.inner.identification.rerun_identify(key);
                }
            }
        }
        Ok(())
    }

    delegate_sync!(manager, set_save_presets => set_save_presets(presets: Vec<crate::config::SavePreset>) -> Result<(), crate::config::ConfigError>);
    delegate_sync!(manager, set_default_track_save_preset => set_default_track_save_preset(preset_id: String) -> Result<(), crate::config::ConfigError>);
    delegate_sync!(manager, set_default_release_save_preset => set_default_release_save_preset(preset_id: String) -> Result<(), crate::config::ConfigError>);
    delegate_sync!(manager, rename_library => rename_library(library_id: &str, name: &crate::library_name::LibraryName) -> Result<(), crate::library::LibraryError>);
    delegate_async!(manager, forget_encryption_key => forget_encryption_key() -> Result<(), crate::library::LibraryError>);
    #[cfg(not(any(target_os = "ios", target_os = "android")))]
    delegate_sync!(manager, get_discogs_token => get_discogs_token() -> Result<Option<String>, crate::library::LibraryError>);
    delegate_async!(manager, disconnect_cloud_provider => disconnect_cloud_provider() -> Result<(), crate::library::LibraryError>);
    delegate_async!(manager, close => close() -> ());
    delegate_async!(manager, unlock_cloud_home => unlock_cloud_home(serialized_master_key: &str) -> Result<(), crate::library::LibraryError>);
    delegate_sync!(manager, trigger_sync => trigger_sync() -> ());
    delegate_async!(manager, reconnect_sync => reconnect_sync() -> Result<(), crate::library::LibraryError>);
    delegate_sync!(manager, is_sync_ready => is_sync_ready() -> bool);
    delegate_sync!(manager, download_snapshot => download_snapshot() -> crate::library::DownloadSnapshot);
    delegate_sync!(manager, set_downloads_paused => set_downloads_paused(paused: bool) -> ());
    delegate_sync!(manager, cancel_download => cancel_download(release_id: &str) -> ());
    delegate_sync!(manager, retry_downloads => retry_downloads() -> ());
    delegate_async!(manager, get_artist_count => get_artist_count() -> Result<u64, crate::library::LibraryError>);
    delegate_async!(manager, get_artist_page => get_artist_page(sort: &[crate::db::ArtistSortCriterion], offset: u64, limit: u64) -> Result<Vec<crate::album_detail::ArtistSummary>, crate::library::LibraryError>);
    delegate_async!(manager, get_artist_detail => get_artist_detail(artist_id: &str) -> Result<Option<crate::album_detail::ArtistDetail>, crate::library::LibraryError>);
    delegate_async!(manager, search_artists => search_artists(query: &crate::library::LibrarySearchQuery) -> Result<Vec<crate::album_detail::ArtistSearchResult>, crate::library::LibraryError>);
    delegate_async!(manager, get_album_index => get_album_index(sort: &[crate::db::AlbumSortCriterion], album_id: &str) -> Result<Option<u64>, crate::library::LibraryError>);
    delegate_async!(manager, find_album_detail => find_album_detail(album_id: &str) -> Result<Option<crate::album_detail::AlbumDetail>, crate::library::LibraryError>);
    delegate_async!(manager, find_release_detail => find_release_detail(release_id: &str) -> Result<Option<crate::album_detail::ReleaseDetail>, crate::library::LibraryError>);
    delegate_async!(manager, get_albums => get_albums(sort: &[crate::db::AlbumSortCriterion]) -> Result<Vec<crate::db::DbAlbum>, crate::library::LibraryError>);
    delegate_async!(manager, get_releases_for_album => get_releases_for_album(album_id: &str) -> Result<Vec<crate::db::DbRelease>, crate::library::LibraryError>);
    delegate_async!(manager, get_release_by_id => get_release_by_id(release_id: &str) -> Result<Option<crate::db::DbRelease>, crate::library::LibraryError>);

    delegate_async!(manager, get_release_records => get_release_records(release_id: &str) -> Result<Vec<crate::import::ReleaseRecord>, crate::library::LibraryError>);
    delegate_async!(manager, get_tracks_for_release => get_tracks_for_release(release_id: &str) -> Result<Vec<crate::db::DbTrack>, crate::library::LibraryError>);
    delegate_async!(manager, get_files_for_release => get_files_for_release(release_id: &str) -> Result<Vec<crate::db::DbFile>, crate::library::LibraryError>);
    delegate_async!(manager, get_file_by_id => get_file_by_id(file_id: &str) -> Result<Option<crate::db::DbFile>, crate::library::LibraryError>);
    delegate_async!(manager, file_local_path => file_local_path(file_id: &str) -> Result<Option<std::path::PathBuf>, crate::library::LibraryError>);
    delegate_async!(manager, get_library_image => get_library_image(id: &str, image_type: &crate::db::LibraryImageType) -> Result<Option<crate::db::DbLibraryImage>, crate::library::LibraryError>);
    delegate_async!(manager, read_image_blob => read_image_blob(image: &crate::album_detail::ImageRef) -> Result<Option<Vec<u8>>, crate::library::LibraryError>);
    delegate_async!(manager, read_gallery_bytes => read_gallery_bytes(release_id: &str, source: &crate::album_detail::GallerySource) -> Result<Vec<u8>, crate::library::LibraryError>);
    delegate_async!(manager, read_cover_image_blob => read_cover_image_blob(release_id: &str) -> Result<Option<Vec<u8>>, crate::library::LibraryError>);
    delegate_async!(manager, get_artists_for_track => get_artists_for_track(track_id: &str) -> Result<Vec<crate::db::DbArtist>, crate::library::LibraryError>);
    delegate_async!(manager, get_all_track_ids => get_all_track_ids() -> Result<Vec<String>, crate::library::LibraryError>);
    delegate_async!(manager, filter_existing_track_ids => filter_existing_track_ids(ids: &[String]) -> Result<Vec<String>, crate::library::LibraryError>);
    delegate_async!(manager, resolve_track_audio => resolve_track_audio(track_id: &str) -> Result<crate::library::ResolvedTrackAudio, crate::library::LibraryError>);
    delegate_async!(manager, resolve_to_track_ids => resolve_to_track_ids(ids: &[String]) -> Result<Vec<String>, crate::library::LibraryError>);
    delegate_async!(manager, get_playback_track_info => get_playback_track_info(track_id: &str) -> Result<crate::playback::PlaybackTrackInfo, crate::library::LibraryError>);
    delegate_async!(manager, change_cover => change_cover(release_id: &str, selection: crate::library::CoverSelection) -> Result<(), crate::library::LibraryError>);
    delegate_async!(manager, set_album_primary_release => set_album_primary_release(album_id: &str, primary_release_id: &str) -> Result<(), crate::library::LibraryError>);
    delegate_async!(manager, unpin_release => unpin_release(release_id: &str) -> Result<(), crate::library::LibraryError>);
    delegate_async!(manager, make_releases_remote => make_releases_remote(release_ids: &[String], pin: bool) -> Result<crate::library::MakeReleasesRemoteOutcome, crate::library::LibraryError>);
    delegate_async!(manager, make_release_local => make_release_local(release_id: &str, new_path: &str) -> Result<(), crate::library::LibraryError>);
    delegate_async!(manager, delete_release => delete_release(release_id: &str) -> Result<(), crate::library::LibraryError>);
    delegate_async!(manager, save_s3_config => save_s3_config(data: crate::sync::S3ConfigData) -> Result<(), crate::library::LibraryError>);
    delegate_async!(manager, cloud_only_release_count => cloud_only_release_count() -> Result<u64, crate::library::LibraryError>);
    delegate_async!(manager, generate_restore_code => generate_restore_code() -> Result<String, crate::library::LibraryError>);
    delegate_async!(manager, get_members => get_members() -> Result<crate::sync::membership::Membership, crate::library::LibraryError>);
    delegate_async!(manager, start_device_pairing => start_device_pairing() -> Result<crate::library::DevicePairingSession, crate::library::LibraryError>);
    delegate_async!(manager, remove_member => remove_member(public_key_hex: &str) -> Result<(), crate::library::LibraryError>);
    delegate_async!(manager, outbox_snapshot => outbox_snapshot() -> Result<crate::library::OutboxSnapshot, crate::library::LibraryError>);
    delegate_async!(manager, retry_outbox_now => retry_outbox_now() -> Result<(), crate::library::LibraryError>);
    delegate_async!(manager, cancel_release_transition => cancel_release_transition(release_id: &str) -> Result<(), crate::library::LibraryError>);
    delegate_async!(manager, set_sync_paused => set_sync_paused(paused: bool) -> ());
    delegate_async!(manager, enqueue_pins => enqueue_pins(release_ids: Vec<String>) -> ());
    delegate_async!(manager, use_cloudkit => use_cloudkit(storage: crate::config::HomeStorage) -> Result<(), crate::library::LibraryError>);
    #[cfg(feature = "oauth-providers")]
    delegate_async!(manager, sign_in_cloud_provider => sign_in_cloud_provider(provider: crate::config::CloudProvider, storage: crate::config::HomeStorage) -> Result<(), crate::library::LibraryError>);
    delegate_async!(manager, get_album_id_for_release => get_album_id_for_release(release_id: &str) -> Result<String, crate::library::LibraryError>);
    #[cfg(not(any(target_os = "ios", target_os = "android")))]
    delegate_async!(manager, release_edit_seed => release_edit_seed(release_id: &str) -> Result<crate::import::ReleaseEditSeed, crate::library::LibraryError>);
    #[cfg(not(any(target_os = "ios", target_os = "android")))]
    delegate_async!(manager, enqueue_export => enqueue_export(release_id: &str, target_dir: std::path::PathBuf) -> Result<(), crate::library::LibraryError>);
    #[cfg(not(any(target_os = "ios", target_os = "android")))]
    delegate_async!(manager, enqueue_release_save => enqueue_release_save(release_id: &str, target_dir: std::path::PathBuf, preset_id: &str) -> Result<(), crate::library::LibraryError>);
    #[cfg(not(any(target_os = "ios", target_os = "android")))]
    delegate_async!(manager, save_track => save_track(track_id: &str, output_path: &std::path::Path, preset_id: &str) -> Result<(), crate::library::LibraryError>);
    #[cfg(not(any(target_os = "ios", target_os = "android")))]
    delegate_async!(manager, save_track_suggested_name => save_track_suggested_name(track_id: &str, preset_id: &str) -> Result<String, crate::library::LibraryError>);
    #[cfg(not(any(target_os = "ios", target_os = "android")))]
    delegate_async!(manager, re_identify_release => re_identify_release(release_id: &str, reseed: crate::import::ReleaseReseed) -> Result<(), crate::library::LibraryError>);
    #[cfg(not(any(target_os = "ios", target_os = "android")))]
    delegate_async!(manager, reset_metadata_to_source => reset_metadata_to_source(release_id: &str) -> Result<crate::import::ReleaseUserEdit, crate::library::LibraryError>);
    #[cfg(not(any(target_os = "ios", target_os = "android")))]
    delegate_async!(manager, reset_release_edit_to_source => reset_release_edit_to_source(release_id: &str) -> Result<crate::import::RawReleaseEdit, crate::library::LibraryError>);
    delegate_async!(manager, apply_release_metadata_user_edit => apply_release_metadata_user_edit(release_id: &str, edit: &crate::import::ReleaseUserEdit) -> Result<(), crate::library::LibraryError>);
    delegate_async!(manager, search_library => search_library(query: &crate::library::LibrarySearchQuery) -> Result<crate::album_detail::SearchResults, crate::library::LibraryError>);
    #[cfg(not(any(target_os = "ios", target_os = "android")))]
    delegate_sync!(manager, output_snapshot => output_snapshot() -> crate::library::OutputSnapshot);
    #[cfg(not(any(target_os = "ios", target_os = "android")))]
    delegate_sync!(manager, set_outputs_paused => set_outputs_paused(paused: bool) -> ());
    #[cfg(not(any(target_os = "ios", target_os = "android")))]
    delegate_sync!(manager, cancel_output => cancel_output(release_id: &str) -> ());
    #[cfg(not(any(target_os = "ios", target_os = "android")))]
    delegate_sync!(manager, retry_outputs => retry_outputs() -> ());

    #[cfg(any(test, feature = "test-utils"))]
    delegate_sync!(manager, has_cloud_home => has_cloud_home() -> bool);
    #[cfg(any(test, feature = "test-utils"))]
    delegate_sync!(manager, is_sync_configured => is_sync_configured() -> bool);

    delegate_sync!(playback, playback_play_release => play_release(release_id: String, start_track_index: Option<usize>, shuffle: bool) -> ());
    delegate_sync!(playback, playback_play_releases => play_releases(release_ids: Vec<String>) -> ());
    delegate_sync!(playback, playback_play_library_shuffled => play_library_shuffled() -> ());
    delegate_sync!(playback, playback_pause => pause() -> ());
    delegate_sync!(playback, playback_resume => resume() -> ());
    delegate_sync!(playback, playback_stop => stop() -> ());
    delegate_sync!(playback, playback_next => next() -> ());
    delegate_sync!(playback, playback_previous => previous() -> ());
    delegate_sync!(playback, playback_seek_by_ratio => seek_by_ratio(ratio: f64) -> ());
    delegate_sync!(playback, playback_set_volume => set_volume(volume: f32) -> ());
    delegate_async!(playback, playback_get_volume => get_volume() -> f32);
    delegate_sync!(playback, playback_set_muted => set_muted(muted: bool) -> ());
    delegate_sync!(playback, playback_play_on => play_on(channel: Box<dyn crate::renderer::RendererChannel>, device_name: String, media_source: crate::renderer::RendererMediaSource) -> ());
    delegate_sync!(playback, playback_play_on_airplay => play_on_airplay(sink: Box<dyn crate::playback::airplay_output::AirPlaySink>, device_name: String, latency_frames: u32) -> ());
    delegate_sync!(playback, playback_stop_remote => stop_remote() -> ());
    delegate_sync!(playback, playback_preview_play => preview_play(target: crate::playback::PreviewTarget) -> ());
    delegate_sync!(playback, playback_preview_stop => preview_stop() -> ());
    delegate_sync!(playback, playback_preview_toggle_pause => preview_toggle_pause() -> ());
    delegate_sync!(playback, playback_preview_seek_by_ratio => preview_seek_by_ratio(ratio: f64) -> ());
    delegate_sync!(playback, playback_set_repeat_mode => set_repeat_mode(mode: crate::playback::RepeatMode) -> ());
    delegate_sync!(playback, playback_set_shuffle => set_shuffle(on: bool) -> ());
    delegate_sync!(playback, playback_add_to_queue => add_to_queue(track_ids: Vec<String>) -> ());
    delegate_sync!(playback, playback_add_next => add_next(track_ids: Vec<String>) -> ());
    delegate_sync!(playback, playback_add_release_to_queue => add_release_to_queue(release_id: String) -> ());
    delegate_sync!(playback, playback_add_release_next => add_release_next(release_id: String) -> ());
    delegate_sync!(playback, playback_insert_in_queue => insert_in_queue(track_ids: Vec<String>, index: usize) -> ());
    delegate_sync!(playback, playback_remove_entry => remove_entry(entry_id: crate::playback::QueueEntryId) -> ());
    delegate_sync!(playback, playback_reorder_entry => reorder_entry(entry_id: crate::playback::QueueEntryId, before: Option<crate::playback::QueueEntryId>) -> ());
    delegate_sync!(playback, playback_clear_up_next => clear_up_next() -> ());
    delegate_sync!(playback, playback_clear_playing_from => clear_playing_from() -> ());
    delegate_sync!(playback, playback_skip_to_entry => skip_to_entry(entry_id: crate::playback::QueueEntryId) -> ());
    delegate_async!(playback, playback_shutdown => shutdown() -> ());
    delegate_async!(playback, playback_save_state => save_state() -> ());

    pub fn open_release_file_stream(
        &self,
        file_id: &str,
        size: u64,
    ) -> crate::playback::SharedSparseBuffer {
        let buffer = crate::playback::sparse_buffer::create_sparse_buffer(size);
        let reader = crate::playback::data_source::create_audio_reader(
            &self.inner.manager,
            file_id,
            crate::playback::data_source::FetchArbiter::new(),
            None,
            false,
        );
        let file_id = file_id.to_string();
        reader.start_reading(
            buffer.clone(),
            Box::new(move |error| {
                tracing::warn!(file_id, %error, "reading release file failed");
            }),
        );
        buffer
    }
    pub fn get_sync_status(&self) -> crate::library::SyncStatusSnapshot {
        self.inner.manager.get_sync_status()
    }

    pub fn subscribe_sync_status_values(
        &self,
    ) -> tokio::sync::watch::Receiver<crate::library::SyncStatusSnapshot> {
        self.inner.manager.subscribe_sync_status_values()
    }

    pub async fn retry_blocked_sync_operation(
        &self,
        id: &str,
    ) -> Result<(), crate::library::LibraryError> {
        self.inner.manager.retry_blocked_sync_operation(id).await
    }

    pub fn subscribe_eager_cache_fill_status(
        &self,
    ) -> tokio::sync::watch::Receiver<crate::library::EagerCacheFillStatus> {
        self.inner.manager.subscribe_eager_cache_fill_status()
    }

    pub fn cancel_eager_cache_fill(&self) {
        self.inner.manager.cancel_eager_cache_fill();
    }

    pub fn subscribe_outbox_values(
        &self,
    ) -> tokio::sync::watch::Receiver<Option<Result<crate::library::OutboxSnapshot, String>>> {
        self.inner.manager.subscribe_outbox_values()
    }

    pub fn subscribe_download_values(
        &self,
    ) -> tokio::sync::watch::Receiver<crate::library::DownloadSnapshot> {
        self.inner.manager.subscribe_download_values()
    }

    pub fn subscribe_transfer_values(
        &self,
    ) -> tokio::sync::watch::Receiver<
        std::collections::HashMap<String, crate::album_detail::ReleaseStorageAction>,
    > {
        self.inner.manager.subscribe_transfer_values()
    }

    #[cfg(not(any(target_os = "ios", target_os = "android")))]
    pub fn subscribe_output_values(
        &self,
    ) -> tokio::sync::watch::Receiver<crate::library::OutputSnapshot> {
        self.inner.manager.subscribe_output_values()
    }

    /// Set whether playback pauses between vinyl/cassette sides and CD discs. Turning it on
    /// must take effect at the boundary already staged for gapless playback,
    /// not just the next one: `preload_next_track` decides staging once, at
    /// preload time, so writing the config alone leaves an already-staged
    /// track to cross gaplessly. Turning it off needs no follow-up — the
    /// drain-time gate (`side_pause_for_queue_front`) already re-reads the
    /// config before every boundary.
    pub fn set_pause_between_sides(&self, enabled: bool) -> Result<(), crate::config::ConfigError> {
        self.inner.manager.set_pause_between_sides(enabled)?;
        if enabled {
            self.inner.playback.reevaluate_side_pause_staging();
        }
        Ok(())
    }
}

#[cfg(not(any(target_os = "ios", target_os = "android")))]
mod import;
mod live_query_events;
use live_query_events::{live_query_events, reconfigurable_live_query_events};
#[cfg(test)]
#[path = "app_services_tests.rs"]
mod tests;
