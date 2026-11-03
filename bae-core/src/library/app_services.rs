use super::manager::LibraryManager;
#[cfg(not(any(target_os = "ios", target_os = "android")))]
use crate::import::IdentificationHandle;
#[cfg(not(any(target_os = "ios", target_os = "android")))]
use crate::import::ImportServiceHandle;
use bae_playback::PlaybackHandle;
use std::sync::Arc;

/// One track's stored audio, open for a single decode into a
/// [`DecodedSink`](bae_audio::DecodedSink).
pub struct TrackDecode {
    decode: bae_track_audio::StreamDecodeParams,
    sample_rate: u32,
    channels: u32,
    /// The library's, which a read of the decode that panicked is reported
    /// to.
    diagnostics: bae_diagnostics::Diagnostics,
}

impl TrackDecode {
    pub(crate) fn new(
        decode: bae_track_audio::StreamDecodeParams,
        sample_rate: u32,
        channels: u32,
        diagnostics: bae_diagnostics::Diagnostics,
    ) -> Self {
        Self { decode, sample_rate, channels, diagnostics }
    }

    /// Decode the track into `sink`, announcing its stored format first: why
    /// it stopped short, if it did. A read that panicked is reported as a bug
    /// and fails the decode with the panic's message. Blocking; run it off the
    /// async runtime.
    pub fn run_to_sink(
        &self,
        sink: &mut dyn bae_audio::DecodedSink,
        cancel: Arc<std::sync::atomic::AtomicBool>,
    ) -> Result<(), String> {
        self.decode.run_to_sink(self.sample_rate, self.channels, sink, cancel).map_err(|error| {
            match error {
                bae_audio::DecodeError::ReadPanicked(failed) => {
                    self.diagnostics.bug(failed).to_string()
                }
                error => error.to_string(),
            }
        })
    }

    /// Block until every file the decode reads has delivered its first bytes,
    /// or return why one can't be read, so a caller learns of an unreadable
    /// file before it commits to output. Blocking; run it off the async
    /// runtime.
    pub fn wait_readable(&self) -> Result<(), bae_audio::sparse_buffer::BufferStop> {
        self.decode.wait_readable()
    }
}

/// Why switching a metadata source on or off failed: the preference was
/// refused or could not be written, or the searches asking the source could
/// not be closed after it was.
#[derive(Debug, thiserror::Error)]
pub enum MetadataSourceSwitchError {
    #[error(transparent)]
    Preference(#[from] bae_preferences::PreferencesError),
    #[error(transparent)]
    Searches(#[from] crate::library::LibraryError),
}

macro_rules! delegate_sync {
    ($field:ident, $name:ident => $target:ident($($arg:ident: $ty:ty),* $(,)?) -> $ret:ty) => {
        pub fn $name(&self, $($arg: $ty),*) -> $ret {
            self.inner.$field.$target($($arg),*)
        }
    };
}

/// A live query of the manager's, as a surface pulls its values: the first
/// at once, then one after each write that changes it, until the surface
/// cancels or the library closes.
macro_rules! delegate_values {
    ($name:ident => $target:ident($($arg:ident: $ty:ty),* $(,)?) -> $value:ty) => {
        pub fn $name(&self, $($arg: $ty),*) -> crate::live_query::LiveValues<$value> {
            let manager = &self.inner.manager;
            manager.values_until_closed(manager.$target($($arg),*))
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

/// A read the library's views answer as it is, its failure told as a
/// library error.
macro_rules! delegate_view {
    ($name:ident => $target:ident($($arg:ident: $ty:ty),* $(,)?) -> $ok:ty) => {
        pub async fn $name(&self, $($arg: $ty),*) -> Result<$ok, crate::library::LibraryError> {
            Ok(self.inner.views.$target($($arg),*).await?)
        }
    };
}

struct AppServicesInner {
    manager: LibraryManager,
    /// The library's views, for the reads a screen shows as they are.
    views: crate::library::views::LibraryViews,
    playback: PlaybackHandle,
    /// The cast picker's list, which casting writes and the picker reads.
    cast_picker: bae_playback::renderer::CastPicker,
    /// The import service, which also owns the identify driver and the
    /// extraction feeding it: one of each per library, reached through it.
    #[cfg(not(any(target_os = "ios", target_os = "android")))]
    import: ImportServiceHandle,
    /// Queue-wide identification, started as the library's services start:
    /// the queue runs whether or not anyone has the Import section open, and
    /// opening a view is not what starts it.
    #[cfg(not(any(target_os = "ios", target_os = "android")))]
    identification: IdentificationHandle,
    /// What each library release being identified again asks about, by its
    /// session's candidate key. A release has no candidate row to store them
    /// on, so they are held here from the session's first run to its end.
    #[cfg(not(any(target_os = "ios", target_os = "android")))]
    release_lookup_choices: std::sync::Mutex<
        std::collections::HashMap<crate::import::CandidateKey, bae_identify::LookupChoices>,
    >,
}

impl Drop for AppServicesInner {
    /// Stop and join the playback and import worker threads when the last
    /// `AppServices` clone drops — i.e. when the app is torn down. Each runs on
    /// its own OS thread and, because a live command sender sits in this very
    /// struct, only stops on an explicit `Shutdown`; nothing else joins them.
    /// Until a thread exits it holds a `LibraryManager` clone, and through the
    /// shared coven handle that pins the store's exclusive open lock — so
    /// without joining *every* such thread the same library can't be reopened
    /// in-process. No-ops if [`AppServices::close`] or a playback `shutdown`
    /// already ran: each join handle is taken once.
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

impl std::fmt::Debug for AppServices {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AppServices").field("manager", &self.inner.manager).finish_non_exhaustive()
    }
}

impl AppServices {
    pub fn new(
        manager: LibraryManager,
        views: crate::library::views::LibraryViews,
        playback: PlaybackHandle,
        cast_picker: bae_playback::renderer::CastPicker,
        #[cfg(not(any(target_os = "ios", target_os = "android")))] import: ImportServiceHandle,
        #[cfg(not(any(target_os = "ios", target_os = "android")))]
        identification: IdentificationHandle,
    ) -> Self {
        AppServices {
            inner: Arc::new(AppServicesInner {
                manager,
                views,
                playback,
                cast_picker,
                #[cfg(not(any(target_os = "ios", target_os = "android")))]
                import,
                #[cfg(not(any(target_os = "ios", target_os = "android")))]
                identification,
                #[cfg(not(any(target_os = "ios", target_os = "android")))]
                release_lookup_choices: Default::default(),
            }),
        }
    }

    /// Close the library: stop playback, saving where it was, then the
    /// identification queue and the import service, joining each worker, and
    /// close the store last. When this returns no service holds a file of the
    /// library, the track it was playing included, or the store's lock. The
    /// order is the one dropping the last handle to these services keeps;
    /// whichever runs first stops each worker, and the other finds it stopped.
    /// A step that panics is reported as a bug and fails the close, and the
    /// steps after it still run, the store's close among them.
    pub async fn close(&self) -> Result<(), crate::library::LibraryError> {
        self.close_reporting(&|_| ()).await
    }

    /// [`Self::close`], telling `steps` as each step starts.
    pub async fn close_reporting(
        &self,
        steps: &(dyn Fn(crate::library::ClosingStep) + Send + Sync),
    ) -> Result<(), crate::library::LibraryError> {
        let stopped = self.stop_workers(steps).await;
        let closed = self.inner.manager.close_reporting(steps).await;
        match (stopped, closed) {
            (Ok(()), closed) => closed,
            (Err(stopped), Ok(())) => Err(stopped),
            (Err(stopped), Err(closed)) => Err(crate::library::LibraryError::Cleanup {
                operation: Box::new(stopped),
                cleanup: Box::new(closed),
            }),
        }
    }

    /// Report a bug found in work on these services that has no caller to
    /// fail, such as closing a library whose handle was released, to the log
    /// and the platform's crash reporter.
    pub fn report_bug(&self, operation: &str, error: &dyn std::fmt::Display) {
        self.inner.manager.report_bug(operation, error);
    }

    /// Run `task` as the library's own work until it ends, `stop` is
    /// cancelled, or the library closes. Closing or removing the library ends
    /// it with the manager's other tasks, before the store closes, so it never
    /// reads a closed store. A panic in it is reported as a bug. A library
    /// that has closed starts nothing and says so.
    pub fn spawn_library_task(
        &self,
        task_name: &'static str,
        stop: crate::library::CancellationToken,
        task: impl std::future::Future<Output = ()> + Send + 'static,
    ) -> Result<(), crate::library::LibraryError> {
        self.inner.manager.spawn_stoppable_task(task_name, stop, task, |_| {})
    }

    /// Stop playback, then identification and import, telling `steps` as
    /// each starts.
    async fn stop_workers(
        &self,
        steps: &(dyn Fn(crate::library::ClosingStep) + Send + Sync),
    ) -> Result<(), crate::library::LibraryError> {
        steps(crate::library::ClosingStep::StoppingPlayback);
        self.inner.playback.shutdown().await;
        #[cfg(not(any(target_os = "ios", target_os = "android")))]
        {
            steps(crate::library::ClosingStep::StoppingImports);
            let identification = self.inner.identification.clone();
            let import = self.inner.import.clone();
            // Both joins block until their threads exit, so they run off the
            // runtime rather than stall one of its threads.
            bae_util::task_failure::blocking("stop identification and import", move || {
                // Before the import worker's join, as in the drop: a cancelled
                // candidate writes no row.
                identification.stop();
                import.stop_and_join();
            })
            .await
            .map_err(|failed| self.inner.manager.bug(failed))?;
        }
        Ok(())
    }

    /// The library's preferences as the settings screens read them: the
    /// first value at once, then one after each write that changes them.
    pub fn subscribe_config(&self) -> crate::live_query::LiveValues<bae_preferences::Config> {
        self.inner.manager.subscribe_config()
    }

    /// The part of the preferences `part` takes: the first value at once,
    /// then one after each write that changes it.
    pub fn follow_preferences<T: Clone + PartialEq + Send + 'static>(
        &self,
        part: impl Fn(bae_preferences::Preferences) -> T + Send + Sync + 'static,
    ) -> crate::live_query::LiveValues<T> {
        self.inner.manager.follow_preferences(part)
    }

    pub fn subscribe_album_browse(
        &self,
        sort: &[crate::library::views::AlbumSortCriterion],
        group_by_artist: bool,
    ) -> crate::library::LiveAlbumBrowse {
        let manager = self.inner.manager.clone();
        let query = manager.until_closed(self.inner.views.subscribe_album_browse(
            sort,
            group_by_artist,
            std::collections::BTreeSet::new(),
        ));
        crate::library::LiveLibraryBrowse::new(query, move |projection, request_revision, cause| {
            crate::library::views::album::resolve_album_browse(projection, request_revision, cause)
        })
    }

    /// Which of the albums the grid has selected are gone from the library,
    /// as one live query whose ids move in place as the selection changes; it
    /// starts with none.
    pub fn subscribe_album_selection(&self) -> crate::library::LiveAlbumSelection {
        let manager = &self.inner.manager;
        crate::library::LiveAlbumSelection::new(manager.until_closed(
            self.inner.views.subscribe_album_selection(std::collections::BTreeSet::new()),
        ))
    }

    /// One live library search, pointed at a new query in place as the
    /// person types; it starts with no query.
    pub fn subscribe_library_search(&self) -> crate::library::LiveLibrarySearch {
        let manager = self.inner.manager.clone();
        let query = manager.until_closed(
            self.inner.views.subscribe_library_search(None, crate::library::SEARCH_RESULT_LIMIT),
        );
        crate::library::LiveLibrarySearch::new(query, move |projection| {
            crate::library::views::album::resolve_library_search_projection(projection)
        })
    }

    /// One live search of the library's artists for an artist picker,
    /// pointed at a new text and new windows in place as the person types and
    /// scrolls; it starts with no text and no windows.
    pub fn subscribe_artist_search(&self) -> crate::library::LiveArtistSearch {
        let manager = &self.inner.manager;
        crate::library::LiveArtistSearch::new(
            manager.until_closed(self.inner.views.subscribe_artist_search(
                crate::library::ArtistSearchView {
                    query: None,
                    windows: bae_db::LibraryPageWindows::new(),
                },
            )),
            crate::library::views::artist_search::resolve_artist_search,
        )
    }

    /// The library membership of the releases an import pane offers, as one
    /// live query whose checks move in place as the pane's offers change; it
    /// starts with none.
    pub fn subscribe_library_statuses(&self) -> crate::library::LiveLibraryStatus {
        let manager = &self.inner.manager;
        crate::library::LiveLibraryStatus::new(
            manager.until_closed(
                manager.subscribe_library_statuses(std::collections::BTreeSet::new()),
            ),
        )
    }

    pub fn subscribe_artist_browse(
        &self,
        sort: &[crate::library::views::ArtistSortCriterion],
    ) -> crate::library::LiveArtistBrowse {
        let manager = &self.inner.manager;
        let query = manager.until_closed(
            self.inner.views.subscribe_artist_browse(sort, std::collections::BTreeSet::new()),
        );
        crate::library::LiveLibraryBrowse::new(
            query,
            crate::library::views::artist::resolve_artist_browse,
        )
    }

    pub fn subscribe_composer_browse(
        &self,
        sort: &[crate::library::views::ComposerSortCriterion],
    ) -> crate::library::LiveComposerBrowse {
        let manager = &self.inner.manager;
        let query = manager.until_closed(
            self.inner.views.subscribe_composer_browse(sort, std::collections::BTreeSet::new()),
        );
        crate::library::LiveLibraryBrowse::new(
            query,
            crate::library::views::composer::resolve_composer_browse,
        )
    }

    pub fn subscribe_playback_progress(
        &self,
    ) -> tokio::sync::mpsc::UnboundedReceiver<bae_playback::PlaybackProgress> {
        self.inner.playback.subscribe_progress()
    }

    delegate_async!(manager, preferences => preferences() -> Result<bae_preferences::Preferences, bae_db::DbError>);
    delegate_sync!(manager, library_path => library_path() -> std::path::PathBuf);
    delegate_sync!(manager, store_id => store_id() -> coven::StoreId);
    delegate_async!(manager, ensure_mcp_token => ensure_mcp_token() -> Result<String, crate::library::LibraryError>);
    delegate_async!(manager, set_mcp_token => set_mcp_token(token: String) -> Result<(), crate::library::LibraryError>);
    delegate_async!(manager, set_mcp_config => set_mcp_config(config: bae_preferences::McpConfig) -> Result<(), bae_preferences::PreferencesError>);
    delegate_async!(manager, get_subsonic_password => get_subsonic_password() -> Result<Option<String>, crate::library::LibraryError>);
    delegate_async!(manager, set_subsonic_password => set_subsonic_password(password: String) -> Result<(), crate::library::LibraryError>);
    delegate_async!(manager, set_subsonic_config => set_subsonic_config(config: bae_preferences::SubsonicConfig) -> Result<(), bae_preferences::PreferencesError>);
    delegate_async!(manager, set_cast_enabled => set_cast_enabled(enabled: bool) -> Result<(), bae_preferences::PreferencesError>);
    delegate_async!(manager, set_cast_transcode_format => set_cast_transcode_format(format: bae_preferences::CastTranscodeFormat) -> Result<(), bae_preferences::PreferencesError>);
    delegate_sync!(manager, store_key_state => store_key_state() -> Result<coven::StoreKeyState, crate::library::LibraryError>);
    delegate_async!(manager, set_max_concurrent_uploads => set_max_concurrent_uploads(n: u32) -> Result<(), bae_preferences::PreferencesError>);
    delegate_async!(manager, set_max_concurrent_downloads => set_max_concurrent_downloads(n: u32) -> Result<(), bae_preferences::PreferencesError>);
    delegate_async!(manager, set_side_pause_countdown => set_side_pause_countdown(countdown: bae_preferences::SidePauseCountdown) -> Result<(), bae_preferences::PreferencesError>);
    delegate_async!(manager, set_show_remaining_time => set_show_remaining_time(enabled: bool) -> Result<(), bae_preferences::PreferencesError>);
    delegate_async!(manager, set_library_full_width => set_library_full_width(enabled: bool) -> Result<(), bae_preferences::PreferencesError>);
    delegate_async!(manager, set_identify_automatically => set_identify_automatically(enabled: bool) -> Result<(), bae_preferences::PreferencesError>);
    delegate_async!(manager, set_import_when_identified => set_import_when_identified(enabled: bool) -> Result<(), bae_preferences::PreferencesError>);
    delegate_async!(manager, set_import_to_cloud => set_import_to_cloud(enabled: bool) -> Result<(), bae_preferences::PreferencesError>);
    delegate_async!(manager, set_import_pinned => set_import_pinned(enabled: bool) -> Result<(), bae_preferences::PreferencesError>);

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
    pub async fn set_metadata_source_enabled(
        &self,
        source: bae_catalogs::catalog::LookupCatalog,
        enabled: bool,
    ) -> Result<(), MetadataSourceSwitchError> {
        self.inner.manager.set_metadata_source_enabled(source, enabled).await?;
        #[cfg(not(any(target_os = "ios", target_os = "android")))]
        {
            if !enabled {
                // Searches first: a landing that races the switch then finds a
                // part that has stopped looking, and is dropped.
                self.inner
                    .import
                    .stop_asking_source(source)
                    .await
                    .map_err(crate::library::LibraryError::from)?;
                // A run reads the provider list once, at its start, so every
                // run in flight is answering the list as it was. Each is
                // superseded by a run that reads the list as it now is — every
                // one of them, not only the candidate whose pane the switch was
                // flicked on, because a background run waiting on a source
                // nobody asks is exactly as wrong as the open one. A candidate
                // that already settled has no run to supersede; its next run
                // reads the list as it stands then.
                for key in self.inner.import.identifying_keys() {
                    self.inner.identification.rerun_identify(key);
                }
            }
        }
        Ok(())
    }

    delegate_async!(manager, set_save_presets => set_save_presets(presets: Vec<bae_preferences::SavePreset>) -> Result<(), bae_preferences::PreferencesError>);
    delegate_async!(manager, set_default_track_save_preset => set_default_track_save_preset(preset_id: String) -> Result<(), bae_preferences::PreferencesError>);
    delegate_async!(manager, set_default_release_save_preset => set_default_release_save_preset(preset_id: String) -> Result<(), bae_preferences::PreferencesError>);
    delegate_async!(manager, rename_library => rename_library(name: &crate::library_name::LibraryName) -> Result<(), crate::library::LibraryError>);
    delegate_async!(manager, forget_encryption_key => forget_encryption_key() -> Result<(), crate::library::LibraryError>);
    #[cfg(not(any(target_os = "ios", target_os = "android")))]
    delegate_async!(manager, get_discogs_token => get_discogs_token() -> Result<Option<String>, crate::library::LibraryError>);
    delegate_async!(manager, unlock_store_key => unlock_store_key() -> Result<(), crate::library::LibraryError>);
    delegate_sync!(manager, trigger_sync => trigger_sync() -> ());
    delegate_sync!(manager, network_changed => network_changed() -> ());
    delegate_async!(manager, release_memory => release_memory() -> Result<(), crate::library::LibraryError>);
    delegate_async!(manager, retry_now => retry_now() -> Result<impl std::future::Future<Output = crate::library::SyncRetryOutcome> + Send + 'static, crate::library::LibraryError>);
    delegate_async!(manager, reconnect_sync => reconnect_sync() -> Result<(), crate::library::LibraryError>);
    delegate_async!(manager, set_downloads_paused => set_downloads_paused(paused: bool) -> Result<(), crate::library::LibraryError>);
    delegate_async!(manager, cancel_download => cancel_download(release_id: &str) -> Result<(), crate::library::LibraryError>);
    delegate_async!(manager, retry_downloads => retry_downloads() -> Result<(), crate::library::LibraryError>);
    delegate_view!(get_artist_count => get_artist_count() -> u64);
    delegate_view!(get_artist_page => get_artist_page(sort: &[crate::library::views::ArtistSortCriterion], offset: u64, limit: u64) -> Vec<crate::album_detail::ArtistSummary>);
    delegate_view!(get_artist_detail => get_artist_detail(artist_id: &str) -> Option<crate::album_detail::ArtistDetail>);
    delegate_async!(manager, add_artist_credit_name => add_artist_credit_name(artist_id: &str, name: &str) -> Result<(), crate::library::LibraryError>);
    delegate_async!(manager, rename_artist_credit_name => rename_artist_credit_name(artist_id: &str, key: &str, name: &str) -> Result<(), crate::library::LibraryError>);
    delegate_async!(manager, remove_artist_credit_name => remove_artist_credit_name(artist_id: &str, key: &str) -> Result<(), crate::library::LibraryError>);
    delegate_view!(get_album_index => get_album_index(sort: &[crate::library::views::AlbumSortCriterion], album_id: &str, group_by_artist: bool) -> Option<u64>);
    delegate_async!(manager, check_releases_in_library => check_releases_in_library(checks: &[bae_db::LibraryCheck]) -> Result<Vec<bae_db::LibraryStatus>, crate::library::LibraryError>);
    delegate_view!(albums_in_browse_order => albums_in_browse_order(sort: &[crate::library::views::AlbumSortCriterion], album_ids: Vec<String>, group_by_artist: bool) -> Vec<crate::album_detail::AlbumSummary>);
    delegate_view!(find_album_detail => find_album_detail(album_id: &str) -> Option<crate::album_detail::AlbumDetail>);
    delegate_view!(find_release_detail => find_release_detail(release_id: &str) -> Option<crate::album_detail::ReleaseDetail>);
    delegate_view!(get_albums => get_albums(sort: &[crate::library::views::AlbumSortCriterion]) -> Vec<bae_db::DbAlbum>);
    delegate_async!(manager, get_releases_for_album => get_releases_for_album(album_id: &str) -> Result<Vec<bae_db::DbRelease>, crate::library::LibraryError>);
    delegate_async!(manager, get_release_by_id => get_release_by_id(release_id: &str) -> Result<Option<bae_db::DbRelease>, crate::library::LibraryError>);

    delegate_async!(manager, get_release_records => get_release_records(release_id: &str) -> Result<Vec<bae_catalogs::records::ReleaseRecord>, crate::library::LibraryError>);
    delegate_async!(manager, get_tracks_for_release => get_tracks_for_release(release_id: &str) -> Result<Vec<bae_db::DbTrack>, crate::library::LibraryError>);
    delegate_async!(manager, get_release_mediums => get_release_mediums(release_id: &str) -> Result<bae_db::DbReleaseMediums, crate::library::LibraryError>);
    delegate_async!(manager, get_files_for_release => get_files_for_release(release_id: &str) -> Result<Vec<bae_db::DbFile>, crate::library::LibraryError>);
    delegate_async!(manager, get_file_by_id => get_file_by_id(file_id: &str) -> Result<Option<bae_db::DbFile>, crate::library::LibraryError>);
    delegate_async!(manager, file_local_path => file_local_path(file_id: &str) -> Result<Option<std::path::PathBuf>, crate::library::LibraryError>);
    delegate_async!(manager, get_library_image => get_library_image(id: &str, image_type: &bae_db::LibraryImageType) -> Result<Option<bae_db::DbLibraryImage>, crate::library::LibraryError>);
    delegate_async!(manager, cover_ref => cover_ref(release_id: &str) -> Result<Option<bae_db::ImageRef>, crate::library::LibraryError>);
    delegate_async!(manager, read_image_bytes => read_image_bytes(image: &bae_db::ImageRef) -> Result<Option<Vec<u8>>, crate::library::LibraryError>);
    delegate_async!(manager, read_library_image_bytes => read_library_image_bytes(id: &str, image_type: &bae_db::LibraryImageType) -> Result<Option<Vec<u8>>, crate::library::LibraryError>);
    delegate_async!(manager, read_gallery_bytes => read_gallery_bytes(release_id: &str, source: &crate::album_detail::GallerySource) -> Result<Vec<u8>, crate::library::LibraryError>);
    delegate_async!(manager, read_cover_image_bytes => read_cover_image_bytes(release_id: &str) -> Result<Option<Vec<u8>>, crate::library::LibraryError>);
    delegate_async!(manager, get_artists_for_track => get_artists_for_track(track_id: &str) -> Result<Vec<bae_db::credits::LibraryCredit>, crate::library::LibraryError>);
    delegate_async!(manager, get_artists_for_album => get_artists_for_album(album_id: &str) -> Result<Vec<bae_db::credits::LibraryCredit>, crate::library::LibraryError>);
    delegate_async!(manager, get_all_track_ids => get_all_track_ids() -> Result<Vec<String>, crate::library::LibraryError>);
    delegate_async!(manager, filter_existing_track_ids => filter_existing_track_ids(ids: &[String]) -> Result<Vec<String>, crate::library::LibraryError>);
    delegate_async!(manager, resolve_track_audio => resolve_track_audio(track_id: &str) -> Result<bae_db::ResolvedTrackAudio, crate::library::LibraryError>);
    delegate_async!(manager, resolve_to_track_ids => resolve_to_track_ids(ids: &[String]) -> Result<Vec<String>, crate::library::LibraryError>);
    delegate_async!(manager, get_playback_track_info => get_playback_track_info(track_id: &str) -> Result<bae_playback::PlaybackTrackInfo, crate::library::LibraryError>);
    delegate_async!(manager, get_track_display => get_track_display(track_id: &str) -> Result<bae_playback::TrackDisplay, crate::library::LibraryError>);
    delegate_async!(manager, change_cover => change_cover(release_id: &str, selection: crate::library::CoverSelection) -> Result<(), crate::library::LibraryError>);
    delegate_async!(manager, set_album_primary_release => set_album_primary_release(album_id: &str, primary_release_id: &str) -> Result<(), crate::library::LibraryError>);
    delegate_async!(manager, unpin_release => unpin_release(release_id: &str) -> Result<(), crate::library::LibraryError>);
    delegate_async!(manager, upload_releases => upload_releases(release_ids: &[String]) -> Result<crate::library::UploadReleasesOutcome, crate::library::LibraryError>);
    delegate_async!(manager, detach_release_audio => detach_release_audio(release_id: &str, new_path: &std::path::Path) -> Result<(), crate::library::LibraryError>);
    delegate_async!(manager, delete_release => delete_release(release_id: &str) -> Result<(), crate::library::LibraryError>);
    delegate_async!(manager, delete_album => delete_album(album_id: &str) -> Result<(), crate::library::LibraryError>);
    delegate_async!(manager, save_s3_config => save_s3_config(data: crate::sync::S3ConfigData, device_name: &str, steps: crate::library::StorageSetupSteps<'_>) -> Result<coven::StorageSetup, crate::library::LibraryError>);
    delegate_async!(manager, generate_restore_code => generate_restore_code() -> Result<crate::sync::restore_code::RestoreCodeShare, crate::library::LibraryError>);
    delegate_async!(manager, get_members => get_members() -> Result<bae_db::membership::Membership, crate::library::LibraryError>);
    delegate_async!(manager, remove_member => remove_member(member: &coven::MemberId) -> Result<(), crate::library::LibraryError>);
    delegate_values!(subscribe_members => subscribe_members() -> bae_db::membership::Membership);
    delegate_async!(manager, add_device => add_device() -> Result<crate::sync::membership::AddingDevice, crate::library::LibraryError>);
    delegate_values!(subscribe_added_device => subscribe_added_device(watch: crate::sync::membership::AddedDeviceWatch) -> Option<bae_db::membership::ActiveDevice>);
    delegate_async!(manager, remove_device => remove_device(device: coven::DeviceId) -> Result<Option<coven::ProviderSignOut>, crate::library::LibraryError>);
    delegate_async!(manager, set_member_role => set_member_role(member: &coven::MemberId, role: coven::MemberRole) -> Result<(), crate::library::LibraryError>);
    delegate_values!(subscribe_admin_changes => subscribe_admin_changes() -> Vec<bae_db::admin_change::AdminChange>);
    delegate_async!(manager, acknowledge_admin_change => acknowledge_admin_change(operation: coven::OperationId) -> Result<(), crate::library::LibraryError>);
    delegate_async!(manager, invite_person => invite_person(role: coven::MemberRole, name: String, access: coven::InviteAccess, delivery: coven::InviteDelivery) -> Result<coven::InviteId, crate::library::LibraryError>);
    /// This device's open invites, each with the pending record of its
    /// operation when coven holds one.
    pub fn subscribe_invites(
        &self,
    ) -> crate::live_query::LiveValues<Vec<crate::people::OpenInvite>> {
        self.inner.manager.values_until_closed(self.inner.views.subscribe_invites())
    }
    delegate_async!(manager, approve_join_request => approve_join_request(request: &coven::JoinRequest) -> Result<(), crate::library::LibraryError>);
    delegate_async!(manager, decline_join_request => decline_join_request(request: &coven::JoinRequest) -> Result<(), crate::library::LibraryError>);
    delegate_async!(manager, cancel_invite => cancel_invite(invite: &coven::InviteId) -> Result<(), crate::library::LibraryError>);

    delegate_async!(manager, retry_outbox_now => retry_outbox_now() -> Result<(), crate::library::LibraryError>);
    delegate_async!(manager, cancel_release_transition => cancel_release_transition(release_id: &str) -> Result<(), crate::library::LibraryError>);
    delegate_async!(manager, cancel_all_release_uploads => cancel_all_release_uploads() -> Result<(), crate::library::LibraryError>);
    delegate_async!(manager, set_sync_paused => set_sync_paused(paused: bool) -> Result<(), crate::library::LibraryError>);
    delegate_async!(manager, enqueue_pins => enqueue_pins(release_ids: Vec<String>) -> Result<(), crate::library::LibraryError>);
    delegate_async!(manager, use_cloudkit => use_cloudkit(device_name: &str, steps: crate::library::StorageSetupSteps<'_>) -> Result<coven::StorageSetup, crate::library::LibraryError>);
    #[cfg(feature = "oauth-providers")]
    delegate_async!(manager, sign_in => sign_in(provider: coven::CloudProvider) -> Result<(), crate::library::LibraryError>);
    #[cfg(feature = "oauth-providers")]
    delegate_async!(manager, sign_in_again => sign_in_again(device_name: &str, steps: crate::library::StorageSetupSteps<'_>) -> Result<coven::StorageSetup, crate::library::LibraryError>);
    #[cfg(feature = "oauth-providers")]
    delegate_async!(manager, setup_oauth_storage => setup_oauth_storage(storage: coven::StorageConfig, device_name: &str, steps: crate::library::StorageSetupSteps<'_>) -> Result<coven::StorageSetup, crate::library::LibraryError>);
    #[cfg(feature = "oauth-providers")]
    delegate_async!(manager, onedrive_root => onedrive_root() -> Result<coven::StorageFolder, crate::library::LibraryError>);
    #[cfg(feature = "oauth-providers")]
    delegate_async!(manager, onedrive_folders => onedrive_folders(parent: &coven::StorageConfig) -> Result<Vec<coven::StorageFolder>, crate::library::LibraryError>);
    #[cfg(feature = "oauth-providers")]
    delegate_async!(manager, create_onedrive_folder => create_onedrive_folder(parent: &coven::StorageConfig, name: &str) -> Result<coven::StorageFolder, crate::library::LibraryError>);
    #[cfg(feature = "oauth-providers")]
    delegate_async!(manager, dropbox_shared_folders => dropbox_shared_folders() -> Result<Vec<coven::StorageFolder>, crate::library::LibraryError>);
    delegate_async!(manager, get_album_id_for_release => get_album_id_for_release(release_id: &str) -> Result<String, crate::library::LibraryError>);
    #[cfg(not(any(target_os = "ios", target_os = "android")))]
    delegate_async!(manager, open_release_edit => open_release_edit(release_id: &str) -> Result<crate::library::ReleaseEdit, crate::library::LibraryError>);
    #[cfg(not(any(target_os = "ios", target_os = "android")))]
    delegate_async!(manager, apply_release_edit_catalog => apply_release_edit_catalog(session: &crate::library::ReleaseEdit, link: &bae_identify::PressingLink, steps: &crate::import::CatalogApplySteps) -> Result<crate::import::ReleaseEditSeed, crate::library::LibraryError>);
    #[cfg(not(any(target_os = "ios", target_os = "android")))]
    delegate_async!(manager, save_release_edit => save_release_edit(session: &crate::library::ReleaseEdit, edit: &crate::import::RawReleaseEdit) -> Result<(), crate::library::LibraryError>);
    #[cfg(not(any(target_os = "ios", target_os = "android")))]
    delegate_async!(manager, set_release_edit_album => set_release_edit_album(session: &crate::library::ReleaseEdit, album_id: Option<String>) -> Result<crate::import::DraftAlbum, crate::library::LibraryError>);
    #[cfg(not(any(target_os = "ios", target_os = "android")))]
    delegate_async!(manager, enqueue_export => enqueue_export(release_id: &str, target_dir: std::path::PathBuf) -> Result<(), crate::library::LibraryError>);
    #[cfg(not(any(target_os = "ios", target_os = "android")))]
    delegate_async!(manager, enqueue_release_save => enqueue_release_save(release_id: &str, target_dir: std::path::PathBuf, preset_id: &str) -> Result<(), crate::library::LibraryError>);
    #[cfg(not(any(target_os = "ios", target_os = "android")))]
    delegate_async!(manager, save_track => save_track(track_id: &str, output_path: &std::path::Path, preset_id: &str) -> Result<(), crate::library::LibraryError>);
    #[cfg(not(any(target_os = "ios", target_os = "android")))]
    delegate_async!(manager, save_track_suggested_name => save_track_suggested_name(track_id: &str, preset_id: &str) -> Result<String, crate::library::LibraryError>);
    delegate_async!(manager, resolve_release_edit_credits => resolve_release_edit_credits(edit: &crate::import::RawReleaseEdit) -> Result<Vec<bae_db::ResolvedCredit>, crate::library::LibraryError>);
    delegate_async!(manager, apply_release_metadata_user_edit => apply_release_metadata_user_edit(release_id: &str, edit: &crate::import::ReleaseUserEdit) -> Result<(), crate::library::LibraryError>);
    delegate_view!(search_library => search_library(query: &crate::library::LibrarySearchQuery) -> crate::album_detail::SearchResults);
    #[cfg(not(any(target_os = "ios", target_os = "android")))]
    delegate_async!(manager, set_outputs_paused => set_outputs_paused(paused: bool) -> Result<(), crate::library::LibraryError>);
    #[cfg(not(any(target_os = "ios", target_os = "android")))]
    delegate_async!(manager, cancel_output => cancel_output(release_id: &str) -> Result<(), crate::library::LibraryError>);

    #[cfg(any(test, feature = "test-utils"))]
    delegate_async!(manager, has_cloud_home => has_cloud_home() -> Result<bool, crate::library::LibraryError>);
    #[cfg(any(test, feature = "test-utils"))]
    delegate_async!(manager, is_sync_configured => is_sync_configured() -> Result<bool, crate::library::LibraryError>);

    delegate_sync!(playback, playback_play_release => play_release(release_id: String, start_track_id: Option<String>, shuffle: bool) -> ());
    delegate_sync!(playback, playback_play_releases => play_releases(release_ids: Vec<String>) -> ());
    delegate_sync!(playback, playback_play_library_shuffled => play_library_shuffled() -> ());
    delegate_sync!(playback, playback_pause => pause() -> ());
    delegate_sync!(playback, playback_resume => resume() -> ());
    delegate_sync!(playback, playback_cancel_side_pause_countdown => cancel_side_pause_countdown() -> ());
    delegate_sync!(playback, playback_stop => stop() -> ());
    delegate_sync!(playback, playback_next => next() -> ());
    delegate_sync!(playback, playback_previous => previous() -> ());
    delegate_sync!(playback, playback_seek_by_ratio => seek_by_ratio(ratio: f64) -> ());
    delegate_sync!(playback, playback_set_volume => set_volume(volume: f32) -> ());
    delegate_async!(playback, playback_get_volume => get_volume() -> f32);
    delegate_sync!(playback, playback_set_muted => set_muted(muted: bool) -> ());
    delegate_sync!(playback, playback_play_on => play_on(channel: Box<dyn bae_playback::renderer::RendererChannel>, device: bae_playback::RemoteDevice, media_source: bae_playback::renderer::RendererMediaSource) -> ());
    delegate_sync!(playback, playback_play_on_airplay => play_on_airplay(sink: Box<dyn bae_playback::airplay_output::AirPlaySink>, device: bae_playback::RemoteDevice) -> ());
    delegate_sync!(playback, playback_stop_remote => stop_remote() -> ());
    delegate_sync!(playback, playback_preview_play => preview_play(target: bae_track_audio::PreviewTarget) -> ());
    delegate_sync!(playback, playback_preview_stop => preview_stop() -> ());
    delegate_sync!(playback, playback_preview_toggle_pause => preview_toggle_pause() -> ());
    delegate_sync!(playback, playback_preview_seek_by_ratio => preview_seek_by_ratio(ratio: f64) -> ());
    delegate_sync!(playback, playback_set_repeat_mode => set_repeat_mode(mode: bae_playback::RepeatMode) -> ());
    delegate_sync!(playback, playback_set_shuffle => set_shuffle(on: bool) -> ());
    delegate_sync!(playback, playback_add_to_queue => add_to_queue(track_ids: Vec<String>) -> ());
    delegate_sync!(playback, playback_add_next => add_next(track_ids: Vec<String>) -> ());
    delegate_sync!(playback, playback_add_release_to_queue => add_release_to_queue(release_id: String) -> ());
    delegate_sync!(playback, playback_add_release_next => add_release_next(release_id: String) -> ());
    delegate_sync!(playback, playback_insert_in_queue => insert_in_queue(track_ids: Vec<String>, index: usize) -> ());
    delegate_sync!(playback, playback_remove_entry => remove_entry(entry_id: bae_playback::QueueEntryId) -> ());
    delegate_sync!(playback, playback_reorder_entry => reorder_entry(entry_id: bae_playback::QueueEntryId, before: Option<bae_playback::QueueEntryId>) -> ());
    delegate_sync!(playback, playback_clear_up_next => clear_up_next() -> ());
    delegate_sync!(playback, playback_clear_playing_from => clear_playing_from() -> ());
    delegate_sync!(playback, playback_skip_to_entry => skip_to_entry(entry_id: bae_playback::QueueEntryId) -> ());
    delegate_async!(playback, playback_save_state => save_state() -> ());

    pub fn open_release_file_stream(
        &self,
        file_id: &str,
        size: u64,
    ) -> bae_audio::sparse_buffer::SharedSparseBuffer {
        let buffer = bae_audio::sparse_buffer::create_sparse_buffer(size);
        let reader = self.inner.manager.create_audio_reader(
            file_id,
            bae_track_audio::data_source::FetchArbiter::new(),
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
    /// Open `audio`'s stream for one decode start to finish (a transcode): the
    /// track from its first sample, as playback decodes it from its start — the
    /// silence a CUE `PREGAP` generates, then each segment, pregap included,
    /// seeked the way playback seeks it. Positions in the served stream are
    /// then positions in the track's stream, which a remote renderer reports
    /// and seeks by. One stream per file the segments read; they close when
    /// the returned decode is dropped.
    pub fn open_track_decode(&self, audio: &bae_db::ResolvedTrackAudio) -> TrackDecode {
        use bae_track_audio::{SegmentDecodeParams, StreamDecodeParams};

        let mut streams: std::collections::HashMap<
            &str,
            bae_audio::sparse_buffer::SharedSparseBuffer,
        > = std::collections::HashMap::new();
        let segments = audio
            .segments
            .iter()
            .map(|segment| {
                let buffer = streams
                    .entry(segment.file_id.as_str())
                    .or_insert_with(|| {
                        self.open_release_file_stream(&segment.file_id, segment.file_size)
                    })
                    .clone();
                SegmentDecodeParams::new(buffer, segment.span, 0)
            })
            .collect();
        self.inner.manager.track_decode(
            StreamDecodeParams::new(
                segments,
                bae_audio::byte_jump_keeps_place(&audio.content_type),
                audio.generated_pregap_frames(),
                0,
            ),
            audio.sample_rate.get(),
            audio.channels.get(),
        )
    }

    // The sync status, each value read from its tables in one go.
    delegate_values!(subscribe_sync_status => subscribe_sync_status() -> crate::library::SyncStatusSnapshot);

    pub async fn retry_pending_operation(
        &self,
        id: coven::OperationId,
    ) -> Result<(), crate::library::LibraryError> {
        self.inner.manager.retry_pending_operation(id).await
    }

    /// End a release's failed move to the cloud, `id`, as its pending
    /// record offers: the release stays on this device only, as it was.
    pub async fn cancel_failed_release_move(
        &self,
        id: coven::MoveId,
    ) -> Result<(), crate::library::LibraryError> {
        self.inner.manager.cancel_failed_release_move(id).await
    }

    // Everything sync is doing, for the activity indicator: each value read
    // in one go.
    pub fn subscribe_sync_activity(
        &self,
    ) -> crate::live_query::LiveValues<crate::library::SyncActivity> {
        self.inner.manager.values_until_closed(self.inner.views.subscribe_sync_activity())
    }

    delegate_async!(manager, pending => pending() -> Result<Vec<coven::PendingRecord>, crate::library::LibraryError>);
    delegate_async!(manager, library_name => library_name() -> Result<String, crate::library::LibraryError>);
    delegate_values!(subscribe_library_name => subscribe_library_name() -> String);

    /// Whether the person paused uploads: the first value at once, then one
    /// after each change.
    pub fn subscribe_uploads_paused(&self) -> crate::live_query::LiveValues<bool> {
        self.inner.manager.values_until_closed(self.inner.views.subscribe_uploads_paused())
    }

    delegate_async!(manager, is_sync_paused => is_sync_paused() -> Result<bool, crate::library::LibraryError>);

    delegate_sync!(manager, watch_outbox_transfers => watch_outbox_transfers() -> crate::library::OutboxTransfersWatch);
    delegate_sync!(manager, watch_outbox_total_transfer => watch_outbox_total_transfer() -> crate::library::OutboxTotalTransferWatch);
    delegate_sync!(manager, watch_upload_file_transfers => watch_upload_file_transfers() -> crate::library::FileTransfersWatch);
    delegate_sync!(manager, watch_pin_file_transfers => watch_pin_file_transfers() -> crate::library::FileTransfersWatch);

    /// Set whether playback pauses between vinyl/cassette sides and CD discs. Turning it on
    /// must take effect at the boundary already staged for gapless playback,
    /// not just the next one: `preload_next_track` decides staging once, at
    /// preload time, so writing the config alone leaves an already-staged
    /// track to cross gaplessly. Turning it off needs no follow-up — the
    /// drain-time gate (`side_pause_for_queue_front`) already re-reads the
    /// config before every boundary.
    pub async fn set_pause_between_sides(
        &self,
        enabled: bool,
    ) -> Result<(), bae_preferences::PreferencesError> {
        self.inner.manager.set_pause_between_sides(enabled).await?;
        if enabled {
            self.inner.playback.reevaluate_side_pause_staging();
        }
        Ok(())
    }
}

mod detail_reads;
#[cfg(not(any(target_os = "ios", target_os = "android")))]
mod import;
mod live_query_events;
#[cfg(not(any(target_os = "ios", target_os = "android")))]
mod pane;
mod playback_reads;
mod queue_reads;
#[cfg(desktop)]
mod release_files_reads;
mod release_queue_reads;
#[cfg(not(any(target_os = "ios", target_os = "android")))]
mod selection;
#[cfg(not(any(target_os = "ios", target_os = "android")))]
mod storage_inspector_reads;
mod storage_reads;
use live_query_events::reconfigurable_live_query_events;
#[cfg(test)]
#[path = "app_services_tests.rs"]
mod tests;

mod personal;

mod removal;

impl AppServices {
    delegate_async!(manager, detect_s3_settings => detect_s3_settings(storage:coven::StorageConfig, credentials:coven::S3Credentials) -> Result<coven::S3Detection, crate::library::LibraryError>);
    delegate_async!(manager, storage_location => storage_location() -> Result<Option<coven::StorageConfig>, crate::library::LibraryError>);
}
