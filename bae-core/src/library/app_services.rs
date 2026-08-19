use super::manager::LibraryManager;
#[cfg(not(any(target_os = "ios", target_os = "android")))]
use crate::identify::IdentifyServiceHandle;
#[cfg(not(any(target_os = "ios", target_os = "android")))]
use crate::import::ImportServiceHandle;
#[cfg(not(any(target_os = "ios", target_os = "android")))]
use crate::import::QueueSweepHandle;
use crate::playback::PlaybackHandle;
#[cfg(not(any(target_os = "ios", target_os = "android")))]
use crate::signals::ExtractionServiceHandle;
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
    #[cfg(not(any(target_os = "ios", target_os = "android")))]
    import: ImportServiceHandle,
    #[cfg(not(any(target_os = "ios", target_os = "android")))]
    identify: IdentifyServiceHandle,
    #[cfg(not(any(target_os = "ios", target_os = "android")))]
    extraction: ExtractionServiceHandle,
    /// Queue-wide identification. Built here rather than handed in, so that a
    /// library cannot exist without one: the sweep runs whether or not anyone
    /// has the Import section open, and opening a view is not what starts it.
    #[cfg(not(any(target_os = "ios", target_os = "android")))]
    sweep: QueueSweepHandle,
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
            // Before the import worker's join: the sweep's in-flight candidates
            // are cancelled here, and a cancelled candidate writes no row.
            self.sweep.stop();
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
        let (identify, extraction) = import.start_candidate_services();
        #[cfg(not(any(target_os = "ios", target_os = "android")))]
        let sweep = crate::import::sweep::start(
            import.clone(),
            identify.clone(),
            extraction.clone(),
            manager.clone(),
        );
        AppServices {
            inner: Arc::new(AppServicesInner {
                manager,
                playback,
                #[cfg(not(any(target_os = "ios", target_os = "android")))]
                import,
                #[cfg(not(any(target_os = "ios", target_os = "android")))]
                identify,
                #[cfg(not(any(target_os = "ios", target_os = "android")))]
                extraction,
                #[cfg(not(any(target_os = "ios", target_os = "android")))]
                sweep,
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

    pub fn subscribe_album_page(
        &self,
        sort: &[crate::db::AlbumSortCriterion],
        offset: u64,
        limit: u64,
    ) -> coven::LiveQuery<crate::db::AlbumPageProjection> {
        self.inner.manager.subscribe_album_page(sort, offset, limit)
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

    pub fn resolve_album_page(
        &self,
        projection: crate::db::AlbumPageProjection,
    ) -> (Vec<crate::album_detail::AlbumSummary>, u64) {
        self.inner.manager.resolve_album_page(projection)
    }

    pub fn subscribe_album_detail_values(
        &self,
        runtime_handle: &tokio::runtime::Handle,
        album_id: String,
    ) -> tokio::sync::mpsc::UnboundedReceiver<
        Result<Option<crate::album_detail::AlbumDetail>, crate::library::LibraryError>,
    > {
        let (tx, rx) = tokio::sync::mpsc::unbounded_channel();
        let services = self.clone();
        let mut query = services.inner.manager.subscribe_album_detail(&album_id);
        let mut config = services.subscribe_config_changes();
        let mut sync = services.subscribe_sync_status_values();
        let mut transfers = services.subscribe_transfer_values();
        runtime_handle.spawn(async move {
            let mut last = None;
            loop {
                tokio::select! {
                    result = query.next() => match result {
                        Ok(projection) => {
                            last = Some(projection.clone());
                            let value = services.inner.manager.resolve_album_detail_projection(projection).await;
                            if tx.send(value).is_err() { return; }
                        }
                        Err(error) => {
                            let error = match error {
                                coven::CovenError::Database(error) => *error,
                                other => coven::DbError::Message(other.to_string()),
                            };
                            if tx.send(Err(crate::library::LibraryError::Database(error))).is_err() { return; }
                        }
                    },
                    changed = async { tokio::select! {
                        value = config.changed() => value,
                        value = sync.changed() => value,
                        value = transfers.changed() => value,
                    }} => {
                        if changed.is_err() { return; }
                        config.borrow_and_update();
                        sync.borrow_and_update();
                        transfers.borrow_and_update();
                        if let Some(projection) = last.clone() {
                            let value = services.inner.manager.resolve_album_detail_projection(projection).await;
                            if tx.send(value).is_err() { return; }
                        }
                    }
                }
            }
        });
        rx
    }

    pub fn subscribe_release_detail_values(
        &self,
        runtime_handle: &tokio::runtime::Handle,
        release_id: String,
    ) -> tokio::sync::mpsc::UnboundedReceiver<
        Result<Option<crate::album_detail::ReleaseDetail>, crate::library::LibraryError>,
    > {
        let (tx, rx) = tokio::sync::mpsc::unbounded_channel();
        let services = self.clone();
        let mut query = services.inner.manager.subscribe_release_detail(&release_id);
        let mut config = services.subscribe_config_changes();
        let mut sync = services.subscribe_sync_status_values();
        let mut transfers = services.subscribe_transfer_values();
        runtime_handle.spawn(async move {
            let mut last = None;
            loop {
                tokio::select! {
                    result = query.next() => match result {
                        Ok(projection) => {
                            last = Some(projection.clone());
                            let value = services.inner.manager.resolve_release_detail_projection(&release_id, projection).await;
                            if tx.send(value).is_err() { return; }
                        }
                        Err(error) => {
                            let error = match error {
                                coven::CovenError::Database(error) => *error,
                                other => coven::DbError::Message(other.to_string()),
                            };
                            if tx.send(Err(crate::library::LibraryError::Database(error))).is_err() { return; }
                        }
                    },
                    changed = async { tokio::select! {
                        value = config.changed() => value,
                        value = sync.changed() => value,
                        value = transfers.changed() => value,
                    }} => {
                        if changed.is_err() { return; }
                        config.borrow_and_update();
                        sync.borrow_and_update();
                        transfers.borrow_and_update();
                        if let Some(projection) = last.clone() {
                            let value = services.inner.manager.resolve_release_detail_projection(&release_id, projection).await;
                            if tx.send(value).is_err() { return; }
                        }
                    }
                }
            }
        });
        rx
    }

    pub fn subscribe_library_search(
        &self,
        query: &crate::library::LibrarySearchQuery,
    ) -> coven::LiveQuery<crate::db::LibrarySearchProjection> {
        self.inner.manager.subscribe_library_search(query)
    }

    pub fn resolve_library_search_projection(
        &self,
        projection: crate::db::LibrarySearchProjection,
    ) -> crate::album_detail::SearchResults {
        self.inner
            .manager
            .resolve_library_search_projection(projection)
    }

    pub fn subscribe_release_library_status(
        &self,
        check: crate::db::LibraryCheck,
    ) -> coven::LiveQuery<crate::db::LibraryStatus> {
        self.inner.manager.subscribe_release_library_status(check)
    }

    pub fn subscribe_storage_page(
        &self,
        sort: &crate::db::StorageSortCriterion,
        filter: crate::db::StorageFilter,
        transitioning_release_ids: Vec<String>,
        offset: u64,
        limit: u64,
    ) -> coven::LiveQuery<crate::db::StoragePageProjection> {
        self.inner.manager.subscribe_storage_page(
            sort,
            filter,
            transitioning_release_ids,
            offset,
            limit,
        )
    }

    pub async fn resolve_storage_page_projection(
        &self,
        projection: crate::db::StoragePageProjection,
    ) -> Result<(crate::album_detail::StoragePage, u64), crate::library::LibraryError> {
        self.inner
            .manager
            .resolve_storage_page_projection(projection)
            .await
    }

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
        let mut outbox = services.subscribe_outbox_values();
        let mut sync = services.subscribe_sync_status_values();
        let mut config = services.subscribe_config_changes();
        let mut downloads = services.subscribe_download_values();
        let mut transfers = services.subscribe_transfer_values();
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
            let mut query = services.subscribe_storage_page(
                &sort,
                filter,
                transitioning.clone(),
                offset,
                limit,
            );
            let mut last = None;
            loop {
                tokio::select! {
                    result = query.next() => match result {
                        Ok(projection) => {
                            last = Some(projection.clone());
                            let value = services.resolve_storage_page_projection(projection).await
                                .map(|(page, total_size)| StorageProjectionValue { page, total_size });
                            if tx.send(value).is_err() { return; }
                        }
                        Err(error) => {
                            let error = match error { coven::CovenError::Database(error) => *error, other => coven::DbError::Message(other.to_string()) };
                            if tx.send(Err(crate::library::LibraryError::Database(error))).is_err() { return; }
                        }
                    },
                    changed = outbox.changed(), if filter == crate::db::StorageFilter::Uploading => {
                        if changed.is_err() { return; }
                        match outbox.borrow_and_update().clone() {
                            Some(Ok(snapshot)) => {
                                let next = snapshot.transitioning_release_ids();
                                if replace_transitioning_release_ids(&mut transitioning, next) {
                                    query = services.subscribe_storage_page(&sort, filter, transitioning.clone(), offset, limit);
                                }
                            }
                            Some(Err(error)) => match tx.send(Err(crate::library::LibraryError::Internal(error))) {
                                Ok(()) => {}
                                Err(_) => return,
                            },
                            None => {}
                        }
                    }
                    _ = async { tokio::select! { value = sync.changed() => value, value = config.changed() => value, value = downloads.changed() => value, value = transfers.changed() => value } } => {
                        sync.borrow_and_update(); config.borrow_and_update(); downloads.borrow_and_update(); transfers.borrow_and_update();
                        if let Some(projection) = last.clone() {
                            let value = services.resolve_storage_page_projection(projection).await
                                .map(|(page, total_size)| StorageProjectionValue { page, total_size });
                            if tx.send(value).is_err() { return; }
                        }
                    }
                }
            }
        });
        rx
    }

    pub fn subscribe_artist_page(
        &self,
        sort: &[crate::db::ArtistSortCriterion],
        offset: u64,
        limit: u64,
    ) -> coven::LiveQuery<crate::db::ArtistPageProjection> {
        self.inner
            .manager
            .subscribe_artist_page(sort, offset, limit)
    }

    pub fn resolve_artist_page(
        &self,
        projection: crate::db::ArtistPageProjection,
    ) -> (Vec<crate::album_detail::ArtistSummary>, u64) {
        self.inner.manager.resolve_artist_page(projection)
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

    pub fn subscribe_composer_page(
        &self,
        sort: &[crate::db::ComposerSortCriterion],
        offset: u64,
        limit: u64,
    ) -> coven::LiveQuery<crate::db::ComposerPageProjection> {
        self.inner
            .manager
            .subscribe_composer_page(sort, offset, limit)
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

    pub fn resolve_composer_page(
        &self,
        projection: crate::db::ComposerPageProjection,
    ) -> (Vec<crate::album_detail::ComposerSummary>, u64) {
        self.inner.manager.resolve_composer_page(projection)
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
        runtime_handle.spawn(async move {
            let mut queue_values = services.inner.playback.subscribe_queue_values();
            let mut projection = queue_values.borrow_and_update().clone();
            let mut catalog = services.inner.manager.subscribe_queue_catalog(&projection);
            loop {
                tokio::select! {
                    result = catalog.next() => {
                        let value = result
                            .map(|catalog| services.inner.manager.resolve_queue_catalog(projection.clone(), catalog))
                            .map_err(|error| match error {
                                coven::CovenError::Database(error) => crate::library::LibraryError::Database(*error),
                                other => crate::library::LibraryError::Database(coven::DbError::Message(other.to_string())),
                            });
                        if tx.send(value).is_err() {
                            return;
                        }
                    }
                    changed = queue_values.changed() => {
                        if changed.is_err() { return; }
                        projection = queue_values.borrow_and_update().clone();
                        catalog = services.inner.manager.subscribe_queue_catalog(&projection);
                    }
                }
            }
        });
        rx
    }

    pub fn subscribe_queue_upcoming_values(
        &self,
        runtime_handle: &tokio::runtime::Handle,
        offset: u32,
        limit: u32,
    ) -> tokio::sync::mpsc::UnboundedReceiver<
        Result<crate::queue::ResolvedQueueUpcomingPage, crate::library::LibraryError>,
    > {
        let (tx, rx) = tokio::sync::mpsc::unbounded_channel();
        let services = self.clone();
        runtime_handle.spawn(async move {
            let mut queue_values = services.inner.playback.subscribe_queue_values();
            let mut projection = queue_values.borrow_and_update().clone();
            let page_entries = |projection: &crate::playback::PlaybackQueueProjection| {
                let tail = projection
                    .context
                    .as_ref()
                    .map(|context| context.upcoming.as_slice())
                    .unwrap_or(&[]);
                crate::queue::clamp_upcoming_page(tail, offset, limit).to_vec()
            };
            let mut entries = page_entries(&projection);
            let mut catalog = services
                .inner
                .manager
                .subscribe_queue_entries(entries.clone());
            loop {
                tokio::select! {
                    result = catalog.next() => {
                        let value = result
                            .map(|catalog| crate::queue::ResolvedQueueUpcomingPage {
                                revision: projection.revision,
                                items: services.inner.manager.resolve_queue_entries(entries.len(), catalog),
                            })
                            .map_err(|error| match error {
                                coven::CovenError::Database(error) => crate::library::LibraryError::Database(*error),
                                other => crate::library::LibraryError::Database(coven::DbError::Message(other.to_string())),
                            });
                        if tx.send(value).is_err() { return; }
                    }
                    changed = queue_values.changed() => {
                        if changed.is_err() { return; }
                        projection = queue_values.borrow_and_update().clone();
                        entries = page_entries(&projection);
                        catalog = services.inner.manager.subscribe_queue_entries(entries.clone());
                    }
                }
            }
        });
        rx
    }

    #[cfg(not(any(target_os = "ios", target_os = "android")))]
    pub(crate) fn record_telemetry(&self, event: crate::diagnostics::TelemetryEvent) {
        self.inner.manager.record_telemetry(event);
    }

    #[cfg(not(any(target_os = "ios", target_os = "android")))]
    pub(crate) fn subscribe_import_events(
        &self,
    ) -> tokio::sync::broadcast::Receiver<crate::import::ImportEvent> {
        self.inner.import.subscribe_events()
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
    delegate_sync!(manager, set_save_presets => set_save_presets(presets: Vec<crate::config::SavePreset>) -> Result<(), crate::config::ConfigError>);
    delegate_sync!(manager, set_default_track_save_preset => set_default_track_save_preset(preset_id: String) -> Result<(), crate::config::ConfigError>);
    delegate_sync!(manager, set_default_release_save_preset => set_default_release_save_preset(preset_id: String) -> Result<(), crate::config::ConfigError>);
    delegate_sync!(manager, rename_library => rename_library(library_id: &str, name: &crate::library_name::LibraryName) -> Result<(), crate::library::LibraryError>);
    delegate_async!(manager, forget_encryption_key => forget_encryption_key() -> Result<(), crate::library::LibraryError>);
    #[cfg(not(any(target_os = "ios", target_os = "android")))]
    delegate_sync!(manager, get_discogs_token => get_discogs_token() -> Result<Option<String>, crate::library::LibraryError>);
    delegate_async!(manager, disconnect_cloud_provider => disconnect_cloud_provider() -> Result<(), crate::library::LibraryError>);
    delegate_async!(manager, forget_library => forget_library() -> Result<(), crate::library::LibraryError>);
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
    delegate_async!(manager, get_album_page => get_album_page(sort: &[crate::db::AlbumSortCriterion], offset: u64, limit: u64) -> Result<Vec<crate::album_detail::AlbumSummary>, crate::library::LibraryError>);
    delegate_async!(manager, get_album_index => get_album_index(sort: &[crate::db::AlbumSortCriterion], album_id: &str) -> Result<Option<u64>, crate::library::LibraryError>);
    delegate_async!(manager, get_album_count => get_album_count() -> Result<u64, crate::library::LibraryError>);
    delegate_async!(manager, get_composer_count => get_composer_count() -> Result<u64, crate::library::LibraryError>);
    delegate_async!(manager, get_composer_page => get_composer_page(sort: &[crate::db::ComposerSortCriterion], offset: u64, limit: u64) -> Result<Vec<crate::album_detail::ComposerSummary>, crate::library::LibraryError>);
    delegate_async!(manager, get_composer_detail => get_composer_detail(artist_id: &str) -> Result<Option<crate::album_detail::ComposerDetail>, crate::library::LibraryError>);
    delegate_async!(manager, get_work_detail => get_work_detail(work_id: &str) -> Result<Option<crate::album_detail::WorkDetail>, crate::library::LibraryError>);
    delegate_async!(manager, find_album_detail => find_album_detail(album_id: &str) -> Result<Option<crate::album_detail::AlbumDetail>, crate::library::LibraryError>);
    delegate_async!(manager, find_release_detail => find_release_detail(release_id: &str) -> Result<Option<crate::album_detail::ReleaseDetail>, crate::library::LibraryError>);
    delegate_async!(manager, get_albums => get_albums(sort: &[crate::db::AlbumSortCriterion]) -> Result<Vec<crate::db::DbAlbum>, crate::library::LibraryError>);
    delegate_async!(manager, get_releases_for_album => get_releases_for_album(album_id: &str) -> Result<Vec<crate::db::DbRelease>, crate::library::LibraryError>);
    delegate_async!(manager, get_release_by_id => get_release_by_id(release_id: &str) -> Result<Option<crate::db::DbRelease>, crate::library::LibraryError>);
    delegate_async!(manager, get_tracks_for_release => get_tracks_for_release(release_id: &str) -> Result<Vec<crate::db::DbTrack>, crate::library::LibraryError>);
    delegate_async!(manager, get_files_for_release => get_files_for_release(release_id: &str) -> Result<Vec<crate::db::DbFile>, crate::library::LibraryError>);
    delegate_async!(manager, get_file_by_id => get_file_by_id(file_id: &str) -> Result<Option<crate::db::DbFile>, crate::library::LibraryError>);
    delegate_async!(manager, file_local_path => file_local_path(file_id: &str) -> Result<Option<std::path::PathBuf>, crate::library::LibraryError>);
    delegate_async!(manager, get_storage_page => get_storage_page(sort: &crate::db::StorageSortCriterion, filter: crate::db::StorageFilter, offset: u64, limit: u64) -> Result<crate::album_detail::StoragePage, crate::library::LibraryError>);
    delegate_async!(manager, get_storage_count => get_storage_count(filter: crate::db::StorageFilter) -> Result<u64, crate::library::LibraryError>);
    delegate_async!(manager, get_storage_total_size => get_storage_total_size(filter: crate::db::StorageFilter) -> Result<u64, crate::library::LibraryError>);
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
    delegate_async!(manager, make_release_remote => make_release_remote(release_id: &str, pin: bool) -> Result<u64, crate::library::LibraryError>);
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
    delegate_async!(manager, release_edit_seed => release_edit_seed(release_id: &str) -> Result<crate::import::RawReleaseEdit, crate::library::LibraryError>);
    #[cfg(not(any(target_os = "ios", target_os = "android")))]
    delegate_async!(manager, enqueue_export => enqueue_export(release_id: &str, target_dir: std::path::PathBuf) -> Result<(), crate::library::LibraryError>);
    #[cfg(not(any(target_os = "ios", target_os = "android")))]
    delegate_async!(manager, enqueue_release_save => enqueue_release_save(release_id: &str, target_dir: std::path::PathBuf, preset_id: &str) -> Result<(), crate::library::LibraryError>);
    #[cfg(not(any(target_os = "ios", target_os = "android")))]
    delegate_async!(manager, save_track => save_track(track_id: &str, output_path: &std::path::Path, preset_id: &str) -> Result<(), crate::library::LibraryError>);
    #[cfg(not(any(target_os = "ios", target_os = "android")))]
    delegate_async!(manager, save_track_suggested_name => save_track_suggested_name(track_id: &str, preset_id: &str) -> Result<String, crate::library::LibraryError>);
    #[cfg(not(any(target_os = "ios", target_os = "android")))]
    delegate_async!(manager, re_identify_release => re_identify_release(release_id: &str, identity_choice: crate::import::IdentityChoice) -> Result<(), crate::library::LibraryError>);
    #[cfg(not(any(target_os = "ios", target_os = "android")))]
    delegate_async!(manager, reset_metadata_to_source => reset_metadata_to_source(release_id: &str) -> Result<crate::import::ReleaseUserEdit, crate::library::LibraryError>);
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
    delegate_sync!(playback, playback_play_on => play_on(channel: Box<dyn crate::renderer::RendererChannel>, device_name: String, stream_url_provider: crate::renderer::MediaUrlProvider, cover_url_provider: crate::renderer::CoverUrlProvider, stream_format: crate::renderer::StreamFormatFn) -> ());
    delegate_sync!(playback, playback_play_on_airplay => play_on_airplay(sink: Box<dyn crate::playback::airplay_output::AirPlaySink>, device_name: String, latency_frames: u32) -> ());
    delegate_sync!(playback, playback_stop_remote => stop_remote() -> ());
    delegate_sync!(playback, playback_preview_play => preview_play(path: String) -> ());
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

    #[cfg(not(any(target_os = "ios", target_os = "android")))]
    delegate_async!(import, import_add_watched_folder => add_watched_folder(path: String) -> Result<(), crate::import::ImportError>);
    #[cfg(not(any(target_os = "ios", target_os = "android")))]
    delegate_async!(import, import_remove_watched_folder => remove_watched_folder(path: String) -> Result<(), crate::import::ImportError>);
    #[cfg(not(any(target_os = "ios", target_os = "android")))]
    delegate_sync!(import, import_scan_watched_folders => scan_watched_folders() -> Result<(), crate::import::ImportError>);
    #[cfg(not(any(target_os = "ios", target_os = "android")))]
    delegate_sync!(import, import_subscribe_folder_scan_events => subscribe_folder_scan_events() -> tokio::sync::mpsc::UnboundedReceiver<crate::import::ScanEvent>);
    #[cfg(not(any(target_os = "ios", target_os = "android")))]
    delegate_async!(import, import_set_candidate_skipped => set_candidate_skipped(path: String, skipped: bool) -> Result<(), crate::import::ImportError>);
    #[cfg(not(any(target_os = "ios", target_os = "android")))]
    delegate_async!(import, import_search_with_status => search_with_status(query: crate::import::SearchQuery) -> Result<crate::import::GroupedSearchResults, crate::import::ImportError>);
    #[cfg(not(any(target_os = "ios", target_os = "android")))]
    delegate_async!(import, import_prefetch_release => prefetch_release(candidate_key: &str, release_id: &str, source: crate::import::MetadataSource, level: crate::import::ClaimLevel) -> Result<crate::import::search::ImportReleasePrefetch, crate::import::ImportError>);
    #[cfg(not(any(target_os = "ios", target_os = "android")))]
    delegate_async!(import, import_preview_file_tags_for_folder => preview_file_tags_for_folder(candidate_key: String) -> Result<crate::import::ReleaseUserEdit, crate::import::ImportError>);
    #[cfg(not(any(target_os = "ios", target_os = "android")))]
    delegate_async!(import, import_start_import => start_import(candidate_key: &str, selected_cover: Option<crate::import::CoverSelection>, storage_mode: crate::import::StorageMode, pin: bool, identity_choice: crate::import::IdentityChoice, user_edit: Option<crate::import::ReleaseUserEdit>) -> Result<String, crate::import::ImportError>);
    #[cfg(not(any(target_os = "ios", target_os = "android")))]
    delegate_async!(import, import_save_discogs_token => save_discogs_token(token: &str) -> Result<crate::import::DiscogsSaveOutcome, crate::import::ImportError>);
    #[cfg(not(any(target_os = "ios", target_os = "android")))]
    delegate_async!(import, import_revalidate_discogs_token => revalidate_discogs_token() -> Result<(), crate::import::ImportError>);
    #[cfg(not(any(target_os = "ios", target_os = "android")))]
    delegate_sync!(import, import_remove_discogs_token => remove_discogs_token() -> Result<(), crate::import::ImportError>);
    #[cfg(not(any(target_os = "ios", target_os = "android")))]
    delegate_async!(import, import_pick_candidate_identity => pick_candidate_identity(candidate_key: String, pick: crate::import::IdentityPick) -> Result<crate::import::DecidedIdentity, crate::import::ImportError>);
    #[cfg(not(any(target_os = "ios", target_os = "android")))]
    delegate_async!(import, import_candidate_answer => candidate_answer(candidate_key: String) -> Result<Option<crate::import::DecidedIdentity>, crate::import::ImportError>);
    #[cfg(not(any(target_os = "ios", target_os = "android")))]
    delegate_async!(import, import_refresh_watched_folder => refresh_watched_folder(path: String) -> Result<(), crate::import::ImportError>);
    #[cfg(not(any(target_os = "ios", target_os = "android")))]
    delegate_async!(import, import_set_folder_release_decision => set_folder_release_decision(key: crate::import::FolderReleaseDecisionKey, decision: crate::import::FolderReleaseDecision) -> Result<(), crate::import::ImportError>);
    #[cfg(not(any(target_os = "ios", target_os = "android")))]
    delegate_async!(import, import_sheet_binding_options => sheet_binding_options(candidate_key: String, sheet_file_id: String) -> Result<Vec<crate::import::folder_scanner::SheetBindingOption>, crate::import::ImportError>);
    #[cfg(not(any(target_os = "ios", target_os = "android")))]
    delegate_async!(import, import_set_sheet_binding => set_sheet_binding(candidate_key: String, sheet_file_id: String, audio_file_id: Option<String>) -> Result<(), crate::import::ImportError>);
    #[cfg(not(any(target_os = "ios", target_os = "android")))]
    delegate_async!(import, import_set_sheet_disc => set_sheet_disc(candidate_key: String, sheet_file_id: String, disc: crate::import::folder_scanner::SheetDisc) -> Result<(), crate::import::ImportError>);
    #[cfg(not(any(target_os = "ios", target_os = "android")))]
    delegate_async!(import, import_set_file_role => set_file_role(candidate_key: String, file_id: String, choice: crate::import::folder_scanner::FileRoleChoice) -> Result<(), crate::import::ImportError>);
    #[cfg(not(any(target_os = "ios", target_os = "android")))]
    delegate_async!(import, import_fetch_remote_covers => fetch_remote_covers(release_id: &str) -> Result<Vec<crate::import::cover_art::RemoteCover>, crate::import::ImportError>);
    #[cfg(not(any(target_os = "ios", target_os = "android")))]
    delegate_async!(import, import_fetch_remote_image_bytes => fetch_remote_image_bytes(url: String) -> Result<Option<crate::import::cover_art::RemoteImage>, crate::import::ImportError>);
    #[cfg(not(any(target_os = "ios", target_os = "android")))]
    delegate_sync!(import, import_claim_for_pick => claim_for_pick(candidate_key: &str, release: &crate::import::ClaimRelease, level: crate::import::ClaimLevel) -> crate::import::ClaimLine);
    #[cfg(not(any(target_os = "ios", target_os = "android")))]
    delegate_sync!(import, import_candidate_mapping => candidate_mapping(candidate_key: &str) -> Result<crate::import::MappingTable, crate::import::ImportError>);
    #[cfg(not(any(target_os = "ios", target_os = "android")))]
    delegate_sync!(identify, identify_start => start(key: String, priority: crate::util::rate_limiter::CallPriority) -> ());
    #[cfg(not(any(target_os = "ios", target_os = "android")))]
    delegate_sync!(identify, identify_cancel => cancel(key: &str) -> ());
    #[cfg(not(any(target_os = "ios", target_os = "android")))]
    delegate_sync!(identify, identify_toggle_signal => toggle_signal(key: &str, signal: crate::identify::ExcludedSignal) -> ());
    #[cfg(not(any(target_os = "ios", target_os = "android")))]
    delegate_sync!(identify, identify_rerun => rerun(key: &str) -> ());
    #[cfg(not(any(target_os = "ios", target_os = "android")))]
    delegate_sync!(extraction, extraction_register_analyzer => register_analyzer(analyzer: Arc<dyn crate::signals::ArtworkAnalyzer>) -> ());
    #[cfg(not(any(target_os = "ios", target_os = "android")))]
    delegate_sync!(extraction, extraction_start => start(key: String, source: crate::signals::ExtractionSource, priority: crate::util::rate_limiter::CallPriority) -> ());
    #[cfg(not(any(target_os = "ios", target_os = "android")))]
    delegate_sync!(extraction, extraction_cancel => cancel(key: &str) -> ());

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

    /// Identify a folder candidate for a person who is looking at it: the run
    /// goes out at `Interactive`, and its verdict is persisted like the sweep's
    /// own.
    ///
    /// Re-identifying a library release is deliberately *not* routed through
    /// here: it has no candidate folder, so there is nothing to key a stored
    /// verdict by.
    #[cfg(not(any(target_os = "ios", target_os = "android")))]
    pub fn identify_folder_candidate(&self, candidate_key: String) {
        self.inner.sweep.identify_for_selection(candidate_key);
    }

    /// Re-run a candidate's identification from the toolbar. Dispatches on
    /// where the run lives: a live driver re-combines from its retained
    /// signals; a candidate showing a resumed verdict has no driver, so a
    /// fresh interactive run replaces the stored answer. Re-identify keys
    /// always have a live driver while their sheet is open, so they take the
    /// first arm.
    #[cfg(not(any(target_os = "ios", target_os = "android")))]
    pub fn rerun_identify(&self, candidate_key: String) {
        if self.inner.identify.is_running(&candidate_key) {
            self.inner.identify.rerun(&candidate_key);
        } else {
            self.inner.sweep.rerun_for_selection(candidate_key);
        }
    }

    pub fn get_sync_status(&self) -> crate::library::SyncStatusSnapshot {
        self.inner.manager.get_sync_status()
    }

    pub fn subscribe_sync_status_values(
        &self,
    ) -> tokio::sync::watch::Receiver<crate::library::SyncStatusSnapshot> {
        self.inner.manager.subscribe_sync_status_values()
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

    #[cfg(not(any(target_os = "ios", target_os = "android")))]
    pub fn subscribe_import_candidates(
        &self,
    ) -> tokio::sync::watch::Receiver<crate::import::ImportCandidatesSnapshot> {
        self.inner.import.subscribe_import_candidates()
    }

    #[cfg(not(any(target_os = "ios", target_os = "android")))]
    pub fn subscribe_import_triage_values(
        &self,
        runtime_handle: &tokio::runtime::Handle,
    ) -> tokio::sync::mpsc::UnboundedReceiver<
        Result<crate::import::TriageQueue, crate::library::LibraryError>,
    > {
        let (tx, rx) = tokio::sync::mpsc::unbounded_channel();
        let services = self.clone();
        let mut candidates = services.subscribe_import_candidates();
        runtime_handle.spawn(async move {
            let mut snapshot = candidates.borrow().clone();
            let mut query = services
                .inner
                .manager
                .subscribe_import_triage(snapshot.clone());
            loop {
                tokio::select! {
                    result = query.next() => {
                        let value = match result {
                            Ok(projection) => services.inner.manager.resolve_import_triage(snapshot.clone(), projection),
                            Err(error) => Err(crate::library::LibraryError::Database(match error {
                                coven::CovenError::Database(error) => *error,
                                other => coven::DbError::Message(other.to_string()),
                            })),
                        };
                        if tx.send(value).is_err() { return; }
                    }
                    changed = candidates.changed() => {
                        if changed.is_err() { return; }
                        snapshot = candidates.borrow_and_update().clone();
                        query = services.inner.manager.subscribe_import_triage(snapshot.clone());
                    }
                }
            }
        });
        rx
    }

    /// Set whether playback pauses between vinyl/cassette sides. Turning it on
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

#[cfg(test)]
#[path = "app_services_tests.rs"]
mod tests;
