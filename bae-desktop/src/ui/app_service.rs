//! AppService - encapsulates reactive state and backend service coordination
//!
//! AppService owns the Store<AppState> and is responsible for:
//! - Subscribing to backend service events
//! - Reducing events and updating the Store
//! - Converting DB types to UI types
//! - Delegating actions to backend services
//!
//! UI components access AppService via `use_app()` and:
//! - Read state reactively from `app.state`
//! - Call action methods like `app.play_album()`

use crate::ui::display_types::{
    album_from_db_ref, artist_from_db_ref, release_from_db_ref, track_from_db_ref,
};
use crate::ui::image_url;
use crate::ui::import_helpers::consume_scan_events;
use bae_core::cache;
use bae_core::config;
use bae_core::db::{DbStorageProfile, ImportStatus, StorageLocation};
use bae_core::import::{self, ImportProgress};
use bae_core::library::{LibraryEvent, SharedLibraryManager};
use bae_core::playback::{self, PlaybackProgress};
#[cfg(feature = "torrent")]
use bae_core::torrent;
use bae_ui::display_types::{QueueItem, TrackImportState};
use bae_ui::stores::{
    ActiveImport, ActiveImportsUiStateStoreExt, AlbumDetailStateStoreExt, AppState,
    AppStateStoreExt, ConfigStateStoreExt, ImportOperationStatus, LibraryStateStoreExt,
    PlaybackStatus, PlaybackUiStateStoreExt, PrepareStep, RepeatMode, StorageProfilesStateStoreExt,
};
use bae_ui::StorageProfile;
use dioxus::prelude::*;
use std::collections::HashMap;

use super::app_context::AppServices;

/// Main application service that encapsulates state and backend coordination.
///
/// Created inside the Dioxus component tree because Store<AppState> is not Send-safe.
/// Access via `use_app()` from any component.
#[derive(Clone)]
pub struct AppService {
    /// Reactive application state (Store for fine-grained reactivity)
    pub state: Store<AppState>,

    /// Library manager for database operations
    pub library_manager: SharedLibraryManager,
    /// Application configuration
    pub config: config::Config,
    /// Import service handle for submitting imports
    pub import_handle: import::ImportServiceHandle,
    /// Playback service handle for audio control
    pub playback_handle: playback::PlaybackHandle,
    /// Cache manager for images/files
    pub cache: cache::CacheManager,
    /// Torrent manager (feature-gated)
    #[cfg(feature = "torrent")]
    pub torrent_manager: torrent::LazyTorrentManager,
}

impl AppService {
    /// Create a new AppService from backend services
    pub fn new(services: &AppServices) -> Self {
        #[cfg(feature = "torrent")]
        {
            Self {
                state: Store::new(AppState::default()),
                library_manager: services.library_manager.clone(),
                config: services.config.clone(),
                import_handle: services.import_handle.clone(),
                playback_handle: services.playback_handle.clone(),
                cache: services.cache.clone(),
                torrent_manager: services.torrent_manager.clone(),
            }
        }
        #[cfg(not(feature = "torrent"))]
        {
            Self {
                state: Store::new(AppState::default()),
                library_manager: services.library_manager.clone(),
                config: services.config.clone(),
                import_handle: services.import_handle.clone(),
                playback_handle: services.playback_handle.clone(),
                cache: services.cache.clone(),
            }
        }
    }

    /// Start all event subscriptions. Call this once after creating AppService.
    pub fn start_subscriptions(&self) {
        self.subscribe_playback_events();

        #[cfg(target_os = "macos")]
        self.subscribe_playback_menu_actions();

        self.subscribe_import_progress();
        self.subscribe_library_events();
        self.subscribe_folder_scan_events();
        self.load_initial_data();
    }

    // =========================================================================
    // Event Subscriptions
    // =========================================================================

    /// Subscribe to playback state changes and update Store
    fn subscribe_playback_events(&self) {
        let state = self.state;
        let playback_handle = self.playback_handle.clone();
        let library_manager = self.library_manager.clone();

        spawn(async move {
            let mut progress_rx = playback_handle.subscribe_progress();
            while let Some(progress) = progress_rx.recv().await {
                match progress {
                    PlaybackProgress::StateChanged { state: new_state } => {
                        let (
                            status,
                            current_track_id,
                            release_id,
                            db_track,
                            position_ms,
                            duration_ms,
                            pregap_ms,
                        ) = match &new_state {
                            bae_core::playback::PlaybackState::Stopped => {
                                (PlaybackStatus::Stopped, None, None, None, 0, 0, None)
                            }
                            bae_core::playback::PlaybackState::Loading { track_id } => (
                                PlaybackStatus::Loading,
                                Some(track_id.clone()),
                                None,
                                None,
                                0,
                                0,
                                None,
                            ),
                            bae_core::playback::PlaybackState::Playing {
                                track,
                                position,
                                duration,
                                pregap_ms,
                                ..
                            } => (
                                PlaybackStatus::Playing,
                                Some(track.id.clone()),
                                Some(track.release_id.clone()),
                                Some(track.clone()),
                                position.as_millis() as u64,
                                duration.map(|d| d.as_millis() as u64).unwrap_or(0),
                                *pregap_ms,
                            ),
                            bae_core::playback::PlaybackState::Paused {
                                track,
                                position,
                                duration,
                                pregap_ms,
                                ..
                            } => (
                                PlaybackStatus::Paused,
                                Some(track.id.clone()),
                                Some(track.release_id.clone()),
                                Some(track.clone()),
                                position.as_millis() as u64,
                                duration.map(|d| d.as_millis() as u64).unwrap_or(0),
                                *pregap_ms,
                            ),
                        };

                        state.playback().status().set(status);
                        state
                            .playback()
                            .current_track_id()
                            .set(current_track_id.clone());
                        state.playback().current_release_id().set(release_id);
                        state.playback().position_ms().set(position_ms);
                        state.playback().duration_ms().set(duration_ms);
                        state.playback().pregap_ms().set(pregap_ms);

                        // Load album and artist info for current track
                        let (current_track, artist_name, cover_url) = if let Some(track) = db_track
                        {
                            let (album_title, cover, artist) = if let Some(ref track_id) =
                                current_track_id
                            {
                                if let Ok(album_id) =
                                    library_manager.get().get_album_id_for_track(track_id).await
                                {
                                    let album_info = if let Ok(Some(album)) =
                                        library_manager.get().get_album_by_id(&album_id).await
                                    {
                                        let cover = album
                                            .cover_image_id
                                            .as_ref()
                                            .map(|id| image_url(id))
                                            .or(album.cover_art_url.clone());
                                        (album.title, cover)
                                    } else {
                                        ("Unknown Album".to_string(), None)
                                    };

                                    // Get artist name
                                    let artist = if let Ok(artists) =
                                        library_manager.get().get_artists_for_album(&album_id).await
                                    {
                                        artists.first().map(|a| a.name.clone()).unwrap_or_default()
                                    } else {
                                        String::new()
                                    };

                                    (album_info.0, album_info.1, artist)
                                } else {
                                    ("Unknown Album".to_string(), None, String::new())
                                }
                            } else {
                                ("Unknown Album".to_string(), None, String::new())
                            };

                            (
                                Some(QueueItem {
                                    track: track_from_db_ref(&track),
                                    album_title,
                                    cover_url: cover.clone(),
                                }),
                                artist,
                                cover,
                            )
                        } else {
                            (None, String::new(), None)
                        };

                        state.playback().current_track().set(current_track);
                        state.playback().artist_name().set(artist_name);
                        state.playback().cover_url().set(cover_url);
                    }
                    PlaybackProgress::PositionUpdate { position, .. } => {
                        state
                            .playback()
                            .position_ms()
                            .set(position.as_millis() as u64);
                    }
                    PlaybackProgress::Seeked {
                        position,
                        was_paused,
                        ..
                    } => {
                        state
                            .playback()
                            .position_ms()
                            .set(position.as_millis() as u64);
                        // Update status based on was_paused
                        if was_paused {
                            state.playback().status().set(PlaybackStatus::Paused);
                        } else {
                            state.playback().status().set(PlaybackStatus::Playing);
                        }
                    }
                    PlaybackProgress::PlaybackError { message } => {
                        state.playback().playback_error().set(Some(message.clone()));
                        // Clear error after 5 seconds
                        let state = state;
                        spawn(async move {
                            tokio::time::sleep(std::time::Duration::from_secs(5)).await;
                            state.playback().playback_error().set(None);
                        });
                    }
                    PlaybackProgress::QueueUpdated { tracks } => {
                        state.playback().queue().set(tracks.clone());

                        // Load track/album details for queue items
                        let mut queue_items = Vec::new();
                        for track_id in &tracks {
                            if let Ok(Some(track)) = library_manager.get().get_track(track_id).await
                            {
                                let (album_title, cover_url) = if let Ok(album_id) =
                                    library_manager.get().get_album_id_for_track(track_id).await
                                {
                                    if let Ok(Some(album)) =
                                        library_manager.get().get_album_by_id(&album_id).await
                                    {
                                        let cover = album
                                            .cover_image_id
                                            .as_ref()
                                            .map(|id| image_url(id))
                                            .or(album.cover_art_url.clone());
                                        (album.title, cover)
                                    } else {
                                        ("Unknown Album".to_string(), None)
                                    }
                                } else {
                                    ("Unknown Album".to_string(), None)
                                };

                                queue_items.push(QueueItem {
                                    track: track_from_db_ref(&track),
                                    album_title,
                                    cover_url,
                                });
                            }
                        }
                        state.playback().queue_items().set(queue_items);
                    }
                    PlaybackProgress::RepeatModeChanged { mode } => {
                        let ui_mode = match mode {
                            bae_core::playback::RepeatMode::None => RepeatMode::None,
                            bae_core::playback::RepeatMode::Track => RepeatMode::Track,
                            bae_core::playback::RepeatMode::Album => RepeatMode::Album,
                        };
                        state.playback().repeat_mode().set(ui_mode);

                        #[cfg(target_os = "macos")]
                        crate::ui::window_activation::set_playback_repeat_mode(mode);
                    }
                    _ => {}
                }
            }
        });
    }

    /// Subscribe to playback menu actions (macOS native menu)
    #[cfg(target_os = "macos")]
    fn subscribe_playback_menu_actions(&self) {
        let state = self.state;
        let playback_handle = self.playback_handle.clone();

        spawn(async move {
            let mut rx = crate::ui::shortcuts::subscribe_playback_actions();
            while let Ok(action) = rx.recv().await {
                match action {
                    crate::ui::shortcuts::PlaybackAction::SetRepeatMode(mode) => {
                        playback_handle.set_repeat_mode(mode);
                    }
                    crate::ui::shortcuts::PlaybackAction::TogglePlayPause => {
                        let status = *state.playback().status().read();
                        match status {
                            PlaybackStatus::Playing => playback_handle.pause(),
                            PlaybackStatus::Paused => playback_handle.resume(),
                            PlaybackStatus::Stopped | PlaybackStatus::Loading => {}
                        }
                    }
                    crate::ui::shortcuts::PlaybackAction::Next => playback_handle.next(),
                    crate::ui::shortcuts::PlaybackAction::Previous => playback_handle.previous(),
                }
            }
        });
    }

    /// Subscribe to import progress and update Store
    fn subscribe_import_progress(&self) {
        let state = self.state;
        let import_handle = self.import_handle.clone();
        let library_manager = self.library_manager.clone();

        spawn(async move {
            let mut progress_rx = import_handle.subscribe_all_imports();
            while let Some(event) = progress_rx.recv().await {
                // Reload library when import completes
                let should_reload = matches!(event, ImportProgress::Complete { .. });

                handle_import_progress(&state, event);

                if should_reload {
                    load_library(&state, &library_manager).await;
                }
            }
        });
    }

    /// Subscribe to library events and reload when albums change
    fn subscribe_library_events(&self) {
        let state = self.state;
        let library_manager = self.library_manager.clone();

        spawn(async move {
            let mut rx = library_manager.get().subscribe_events();
            while let Ok(event) = rx.recv().await {
                match event {
                    LibraryEvent::AlbumsChanged => {
                        load_library(&state, &library_manager).await;
                    }
                }
            }
        });
    }

    /// Subscribe to folder scan events
    fn subscribe_folder_scan_events(&self) {
        let app_service = self.clone();
        let rx = self.import_handle.subscribe_folder_scan_events();

        spawn(async move {
            consume_scan_events(app_service, rx).await;
        });
    }

    /// Load initial data from database
    fn load_initial_data(&self) {
        self.load_config();
        self.load_active_imports();
        self.load_library();
        self.load_storage_profiles();
    }

    /// Load config into Store
    fn load_config(&self) {
        let config = &self.config;
        self.state
            .config()
            .discogs_api_key()
            .set(config.discogs_api_key.clone());
        self.state
            .config()
            .subsonic_enabled()
            .set(config.subsonic_enabled);
        self.state
            .config()
            .subsonic_port()
            .set(config.subsonic_port);
        self.state
            .config()
            .torrent_bind_interface()
            .set(config.torrent_bind_interface.clone());
        self.state
            .config()
            .torrent_listen_port()
            .set(config.torrent_listen_port);
        self.state
            .config()
            .torrent_enable_upnp()
            .set(config.torrent_enable_upnp);
        self.state
            .config()
            .torrent_max_connections()
            .set(config.torrent_max_connections);
        self.state
            .config()
            .torrent_max_connections_per_torrent()
            .set(config.torrent_max_connections_per_torrent);
        self.state
            .config()
            .torrent_max_uploads()
            .set(config.torrent_max_uploads);
        self.state
            .config()
            .torrent_max_uploads_per_torrent()
            .set(config.torrent_max_uploads_per_torrent);
    }

    /// Load active imports from database
    fn load_active_imports(&self) {
        let state = self.state;
        let library_manager = self.library_manager.clone();

        spawn(async move {
            state.active_imports().is_loading().set(true);
            match library_manager.get().get_active_imports().await {
                Ok(db_imports) => {
                    let imports: Vec<ActiveImport> = db_imports
                        .into_iter()
                        .map(|db| ActiveImport {
                            import_id: db.id,
                            album_title: db.album_title,
                            artist_name: db.artist_name,
                            status: convert_import_status(db.status),
                            current_step: None,
                            progress_percent: None,
                            release_id: db.release_id,
                            cover_art_url: None,
                            cover_image_id: None,
                        })
                        .collect();
                    state.active_imports().imports().set(imports);
                }
                Err(e) => {
                    tracing::warn!("Failed to load active imports: {}", e);
                }
            }
            state.active_imports().is_loading().set(false);
        });
    }

    /// Load library albums from database
    fn load_library(&self) {
        let state = self.state;
        let library_manager = self.library_manager.clone();

        spawn(async move {
            load_library(&state, &library_manager).await;
        });
    }

    // =========================================================================
    // Album Detail Methods
    // =========================================================================

    /// Load album detail data into Store (called when navigating to album page)
    pub fn load_album_detail(&self, album_id: &str, release_id: Option<&str>) {
        let state = self.state;
        let library_manager = self.library_manager.clone();
        let album_id = album_id.to_string();
        let release_id = release_id.map(|s| s.to_string());

        spawn(async move {
            load_album_detail(&state, &library_manager, &album_id, release_id.as_deref()).await;
        });
    }

    // =========================================================================
    // Config Methods
    // =========================================================================

    /// Update config and save to disk
    pub fn save_config(&self, updater: impl FnOnce(&mut config::Config)) {
        // Clone current config, apply update, save to disk, update Store
        let mut new_config = self.config.clone();
        updater(&mut new_config);

        // Save to disk
        if let Err(e) = new_config.save() {
            tracing::error!("Failed to save config: {}", e);
            return;
        }

        // Update Store
        self.state
            .config()
            .discogs_api_key()
            .set(new_config.discogs_api_key.clone());
        self.state
            .config()
            .subsonic_enabled()
            .set(new_config.subsonic_enabled);
        self.state
            .config()
            .subsonic_port()
            .set(new_config.subsonic_port);
        self.state
            .config()
            .torrent_bind_interface()
            .set(new_config.torrent_bind_interface.clone());
        self.state
            .config()
            .torrent_listen_port()
            .set(new_config.torrent_listen_port);
        self.state
            .config()
            .torrent_enable_upnp()
            .set(new_config.torrent_enable_upnp);
        self.state
            .config()
            .torrent_max_connections()
            .set(new_config.torrent_max_connections);
        self.state
            .config()
            .torrent_max_connections_per_torrent()
            .set(new_config.torrent_max_connections_per_torrent);
        self.state
            .config()
            .torrent_max_uploads()
            .set(new_config.torrent_max_uploads);
        self.state
            .config()
            .torrent_max_uploads_per_torrent()
            .set(new_config.torrent_max_uploads_per_torrent);
    }

    // =========================================================================
    // Storage Profile Methods
    // =========================================================================

    /// Load storage profiles from database into Store
    pub fn load_storage_profiles(&self) {
        let state = self.state;
        let library_manager = self.library_manager.clone();

        spawn(async move {
            state.storage_profiles().loading().set(true);
            state.storage_profiles().error().set(None);

            match library_manager.get_all_storage_profiles().await {
                Ok(db_profiles) => {
                    let profiles = db_profiles.iter().map(storage_profile_from_db).collect();
                    state.storage_profiles().profiles().set(profiles);
                }
                Err(e) => {
                    state
                        .storage_profiles()
                        .error()
                        .set(Some(format!("Failed to load storage profiles: {}", e)));
                }
            }

            state.storage_profiles().loading().set(false);
        });
    }

    /// Save (create or update) a storage profile
    pub fn save_storage_profile(&self, profile: StorageProfile) {
        let state = self.state;
        let library_manager = self.library_manager.clone();
        let is_new = profile.id.is_empty();

        spawn(async move {
            let result = if is_new {
                let location = storage_location_from_display(profile.location);
                let db_profile = if location == StorageLocation::Local {
                    DbStorageProfile::new_local(
                        &profile.name,
                        &profile.location_path,
                        profile.encrypted,
                    )
                } else {
                    DbStorageProfile::new_cloud(
                        &profile.name,
                        profile.cloud_bucket.as_deref().unwrap_or(""),
                        profile.cloud_region.as_deref().unwrap_or(""),
                        profile.cloud_endpoint.as_deref(),
                        profile.cloud_access_key.as_deref().unwrap_or(""),
                        profile.cloud_secret_key.as_deref().unwrap_or(""),
                        profile.encrypted,
                    )
                }
                .with_default(profile.is_default);
                library_manager.insert_storage_profile(&db_profile).await
            } else {
                let mut db_profile = DbStorageProfile {
                    id: profile.id.clone(),
                    name: profile.name.clone(),
                    location: storage_location_from_display(profile.location),
                    location_path: profile.location_path.clone(),
                    encrypted: profile.encrypted,
                    is_default: profile.is_default,
                    cloud_bucket: profile.cloud_bucket.clone(),
                    cloud_region: profile.cloud_region.clone(),
                    cloud_endpoint: profile.cloud_endpoint.clone(),
                    cloud_access_key: profile.cloud_access_key.clone(),
                    cloud_secret_key: profile.cloud_secret_key.clone(),
                    created_at: chrono::Utc::now(),
                    updated_at: chrono::Utc::now(),
                };

                if db_profile.location == StorageLocation::Local {
                    db_profile.cloud_bucket = None;
                    db_profile.cloud_region = None;
                    db_profile.cloud_endpoint = None;
                    db_profile.cloud_access_key = None;
                    db_profile.cloud_secret_key = None;
                }

                library_manager.update_storage_profile(&db_profile).await
            };

            match result {
                Ok(()) => {
                    tracing::info!("Saved storage profile: {}", profile.name);
                    // Reload profiles
                    if let Ok(db_profiles) = library_manager.get_all_storage_profiles().await {
                        let profiles = db_profiles.iter().map(storage_profile_from_db).collect();
                        state.storage_profiles().profiles().set(profiles);
                    }
                }
                Err(e) => {
                    tracing::error!("Failed to save storage profile: {}", e);
                    state
                        .storage_profiles()
                        .error()
                        .set(Some(format!("Failed to save: {}", e)));
                }
            }
        });
    }

    /// Delete a storage profile
    pub fn delete_storage_profile(&self, profile_id: &str) {
        let state = self.state;
        let library_manager = self.library_manager.clone();
        let profile_id = profile_id.to_string();

        spawn(async move {
            match library_manager.delete_storage_profile(&profile_id).await {
                Ok(()) => {
                    tracing::info!("Deleted storage profile: {}", profile_id);
                    // Reload profiles
                    if let Ok(db_profiles) = library_manager.get_all_storage_profiles().await {
                        let profiles = db_profiles.iter().map(storage_profile_from_db).collect();
                        state.storage_profiles().profiles().set(profiles);
                    }
                }
                Err(e) => {
                    tracing::error!("Failed to delete storage profile: {}", e);
                    state
                        .storage_profiles()
                        .error()
                        .set(Some(format!("Failed to delete: {}", e)));
                }
            }
        });
    }

    /// Set a storage profile as default
    pub fn set_default_storage_profile(&self, profile_id: &str) {
        let state = self.state;
        let library_manager = self.library_manager.clone();
        let profile_id = profile_id.to_string();

        spawn(async move {
            match library_manager
                .set_default_storage_profile(&profile_id)
                .await
            {
                Ok(()) => {
                    tracing::info!("Set default storage profile: {}", profile_id);
                    // Reload profiles
                    if let Ok(db_profiles) = library_manager.get_all_storage_profiles().await {
                        let profiles = db_profiles.iter().map(storage_profile_from_db).collect();
                        state.storage_profiles().profiles().set(profiles);
                    }
                }
                Err(e) => {
                    tracing::error!("Failed to set default storage profile: {}", e);
                    state
                        .storage_profiles()
                        .error()
                        .set(Some(format!("Failed to set default: {}", e)));
                }
            }
        });
    }
}

// =============================================================================
// Helper Functions
// =============================================================================

/// Convert DbStorageProfile to display StorageProfile
fn storage_profile_from_db(p: &DbStorageProfile) -> StorageProfile {
    StorageProfile {
        id: p.id.clone(),
        name: p.name.clone(),
        location: storage_location_to_display(p.location),
        location_path: p.location_path.clone(),
        encrypted: p.encrypted,
        is_default: p.is_default,
        cloud_bucket: p.cloud_bucket.clone(),
        cloud_region: p.cloud_region.clone(),
        cloud_endpoint: p.cloud_endpoint.clone(),
        cloud_access_key: p.cloud_access_key.clone(),
        cloud_secret_key: p.cloud_secret_key.clone(),
    }
}

/// Convert StorageLocation to display type
fn storage_location_to_display(loc: StorageLocation) -> bae_ui::StorageLocation {
    match loc {
        StorageLocation::Local => bae_ui::StorageLocation::Local,
        StorageLocation::Cloud => bae_ui::StorageLocation::Cloud,
    }
}

/// Convert display StorageLocation to DB type
fn storage_location_from_display(loc: bae_ui::StorageLocation) -> StorageLocation {
    match loc {
        bae_ui::StorageLocation::Local => StorageLocation::Local,
        bae_ui::StorageLocation::Cloud => StorageLocation::Cloud,
    }
}

/// Load library albums and artists into the Store
async fn load_library(state: &Store<AppState>, library_manager: &SharedLibraryManager) {
    state.library().loading().set(true);
    state.library().error().set(None);

    match library_manager.get().get_albums().await {
        Ok(album_list) => {
            let mut artists_map = HashMap::new();
            for album in &album_list {
                if let Ok(db_artists) = library_manager.get().get_artists_for_album(&album.id).await
                {
                    let artists = db_artists.iter().map(artist_from_db_ref).collect();
                    artists_map.insert(album.id.clone(), artists);
                }
            }
            let display_albums = album_list.iter().map(album_from_db_ref).collect();

            state.library().albums().set(display_albums);
            state.library().artists_by_album().set(artists_map);
        }
        Err(e) => {
            state
                .library()
                .error()
                .set(Some(format!("Failed to load library: {}", e)));
        }
    }

    state.library().loading().set(false);
}

/// Load album detail data into the Store
async fn load_album_detail(
    state: &Store<AppState>,
    library_manager: &SharedLibraryManager,
    album_id: &str,
    release_id_param: Option<&str>,
) {
    state.album_detail().loading().set(true);
    state.album_detail().error().set(None);

    // Load album
    let album = match library_manager.get().get_album_by_id(album_id).await {
        Ok(Some(db_album)) => Some(album_from_db_ref(&db_album)),
        Ok(None) => {
            state
                .album_detail()
                .error()
                .set(Some("Album not found".to_string()));
            state.album_detail().loading().set(false);
            return;
        }
        Err(e) => {
            state
                .album_detail()
                .error()
                .set(Some(format!("Failed to load album: {}", e)));
            state.album_detail().loading().set(false);
            return;
        }
    };
    state.album_detail().album().set(album);

    // Load releases
    let releases = match library_manager.get().get_releases_for_album(album_id).await {
        Ok(db_releases) => db_releases,
        Err(e) => {
            state
                .album_detail()
                .error()
                .set(Some(format!("Failed to load releases: {}", e)));
            state.album_detail().loading().set(false);
            return;
        }
    };

    if releases.is_empty() {
        state
            .album_detail()
            .error()
            .set(Some("Album has no releases".to_string()));
        state.album_detail().loading().set(false);
        return;
    }

    // Determine selected release
    let selected_release = if let Some(rid) = release_id_param {
        releases
            .iter()
            .find(|r| r.id == rid)
            .unwrap_or(&releases[0])
    } else {
        &releases[0]
    };
    let selected_release_id = selected_release.id.clone();

    // Check if release is importing
    let is_importing = selected_release.import_status == ImportStatus::Importing
        || selected_release.import_status == ImportStatus::Queued;
    if is_importing {
        // Progress will be updated by import subscription
        state.album_detail().import_progress().set(Some(0));
    }

    let display_releases = releases.iter().map(release_from_db_ref).collect();
    state.album_detail().releases().set(display_releases);
    state
        .album_detail()
        .selected_release_id()
        .set(Some(selected_release_id.clone()));

    // Load artists
    if let Ok(db_artists) = library_manager.get().get_artists_for_album(album_id).await {
        let artists = db_artists.iter().map(artist_from_db_ref).collect();
        state.album_detail().artists().set(artists);
    }

    // Load tracks for selected release (sorted by disc/track number)
    match library_manager.get().get_tracks(&selected_release_id).await {
        Ok(db_tracks) => {
            let mut tracks: Vec<_> = db_tracks.iter().map(track_from_db_ref).collect();
            tracks.sort_by(|a, b| {
                (a.disc_number, a.track_number).cmp(&(b.disc_number, b.track_number))
            });

            // Set derived fields first to avoid subscribing to tracks for count/ids/disc info
            let track_count = tracks.len();
            let track_ids: Vec<String> = tracks.iter().map(|t| t.id.clone()).collect();
            let track_disc_info: Vec<(Option<i32>, String)> = tracks
                .iter()
                .map(|t| (t.disc_number, t.id.clone()))
                .collect();
            state.album_detail().track_count().set(track_count);
            state.album_detail().track_ids().set(track_ids);
            state.album_detail().track_disc_info().set(track_disc_info);
            state.album_detail().tracks().set(tracks);
        }
        Err(e) => {
            state
                .album_detail()
                .error()
                .set(Some(format!("Failed to load tracks: {}", e)));
        }
    }

    state.album_detail().loading().set(false);
}

/// Convert bae_core ImportOperationStatus to bae_ui ImportOperationStatus
fn convert_import_status(status: bae_core::db::ImportOperationStatus) -> ImportOperationStatus {
    match status {
        bae_core::db::ImportOperationStatus::Preparing => ImportOperationStatus::Preparing,
        bae_core::db::ImportOperationStatus::Importing => ImportOperationStatus::Importing,
        bae_core::db::ImportOperationStatus::Complete => ImportOperationStatus::Complete,
        bae_core::db::ImportOperationStatus::Failed => ImportOperationStatus::Failed,
    }
}

/// Convert bae_core PrepareStep to bae_ui PrepareStep
fn convert_prepare_step(step: bae_core::import::PrepareStep) -> PrepareStep {
    match step {
        bae_core::import::PrepareStep::ParsingMetadata => PrepareStep::ParsingMetadata,
        bae_core::import::PrepareStep::DownloadingCoverArt => PrepareStep::DownloadingCoverArt,
        bae_core::import::PrepareStep::DiscoveringFiles => PrepareStep::DiscoveringFiles,
        bae_core::import::PrepareStep::ValidatingTracks => PrepareStep::ValidatingTracks,
        bae_core::import::PrepareStep::SavingToDatabase => PrepareStep::SavingToDatabase,
        bae_core::import::PrepareStep::ExtractingDurations => PrepareStep::ExtractingDurations,
    }
}

/// Handle import progress events and update Store
fn handle_import_progress(state: &Store<AppState>, event: ImportProgress) {
    match event {
        ImportProgress::Preparing {
            import_id,
            step,
            album_title,
            artist_name,
            cover_art_url,
        } => {
            state.active_imports().imports().with_mut(|list| {
                if let Some(import) = list.iter_mut().find(|i| i.import_id == import_id) {
                    import.current_step = Some(convert_prepare_step(step));
                    import.status = ImportOperationStatus::Preparing;
                } else {
                    list.push(ActiveImport {
                        import_id,
                        album_title,
                        artist_name,
                        status: ImportOperationStatus::Preparing,
                        current_step: Some(convert_prepare_step(step)),
                        progress_percent: None,
                        release_id: None,
                        cover_art_url,
                        cover_image_id: None,
                    });
                }
            });
        }
        ImportProgress::Started { id, import_id, .. } => {
            if let Some(ref iid) = import_id {
                state.active_imports().imports().with_mut(|list| {
                    if let Some(import) = list.iter_mut().find(|i| &i.import_id == iid) {
                        import.status = ImportOperationStatus::Importing;
                        import.current_step = None;
                        import.progress_percent = Some(0);
                        if import.release_id.is_none() {
                            import.release_id = Some(id.clone());
                        }
                    }
                });
            }
        }
        ImportProgress::Progress {
            id: track_id,
            percent,
            import_id,
            ..
        } => {
            // Update active imports
            if let Some(ref iid) = import_id {
                state.active_imports().imports().with_mut(|list| {
                    if let Some(import) = list.iter_mut().find(|i| &i.import_id == iid) {
                        import.progress_percent = Some(percent);
                    }
                });
            }

            // Update track in album_detail if present
            state.album_detail().tracks().with_mut(|tracks| {
                if let Some(track) = tracks.iter_mut().find(|t| t.id == track_id) {
                    track.import_state = TrackImportState::Importing(percent);
                }
            });

            // Update overall import progress for album detail
            state.album_detail().import_progress().set(Some(percent));
        }
        ImportProgress::Complete {
            id,
            import_id,
            release_id,
            cover_image_id,
            ..
        } => {
            // Update active imports
            if let Some(ref iid) = import_id {
                state.active_imports().imports().with_mut(|list| {
                    if let Some(import) = list.iter_mut().find(|i| &i.import_id == iid) {
                        import.status = ImportOperationStatus::Complete;
                        import.progress_percent = Some(100);
                        if release_id.is_some() {
                            import.release_id = release_id.clone();
                        }
                        if cover_image_id.is_some() {
                            import.cover_image_id = cover_image_id.clone();
                        }
                    }
                });
            }

            // Check if this is a track completion (release_id is Some) or release completion
            if release_id.is_some() {
                // Track completed - update track in album_detail
                state.album_detail().tracks().with_mut(|tracks| {
                    if let Some(track) = tracks.iter_mut().find(|t| t.id == id) {
                        track.import_state = TrackImportState::Complete;
                        track.is_available = true;
                    }
                });
            } else {
                // Release completed - clear import progress
                state.album_detail().import_progress().set(None);
                state.album_detail().import_error().set(None);
            }
        }
        ImportProgress::Failed {
            import_id, error, ..
        } => {
            if let Some(ref iid) = import_id {
                state.active_imports().imports().with_mut(|list| {
                    if let Some(import) = list.iter_mut().find(|i| &i.import_id == iid) {
                        import.status = ImportOperationStatus::Failed;
                    }
                });
            }

            // Update album_detail import error
            state.album_detail().import_progress().set(None);
            state.album_detail().import_error().set(Some(error));
        }
    }
}

/// Hook to access the AppService from any component
pub fn use_app() -> AppService {
    use_context::<AppService>()
}
