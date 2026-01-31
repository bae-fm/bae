use bae_core::library::SharedLibraryManager;
use bae_core::playback::{PlaybackHandle, PlaybackProgress, PlaybackState};
use souvlaki::{
    MediaControlEvent, MediaControls, MediaMetadata, MediaPlayback, MediaPosition, PlatformConfig,
};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use tracing::{error, info, trace};
/// Initialize media controls for macOS
/// This handles system media key events (play/pause FN key)
/// Returns the MediaControls handle which must be kept alive for the app lifetime
pub fn setup_media_controls(
    playback_handle: PlaybackHandle,
    library_manager: SharedLibraryManager,
    runtime_handle: tokio::runtime::Handle,
) -> Result<Arc<Mutex<MediaControls>>, souvlaki::Error> {
    let current_state = Arc::new(Mutex::new(PlaybackState::Stopped));
    let playback_handle_for_controls = playback_handle.clone();
    let playback_handle_for_progress = playback_handle.clone();
    let library_manager_for_metadata = library_manager.clone();
    let current_state_for_controls = current_state.clone();
    let current_state_for_progress = current_state.clone();
    let config = PlatformConfig {
        dbus_name: "com.bae.app",
        display_name: "bae",
        hwnd: None,
    };
    let mut controls = MediaControls::new(config)?;
    controls.attach(move |event: MediaControlEvent| {
        let playback = playback_handle_for_controls.clone();
        let state = current_state_for_controls.clone();
        match event {
            MediaControlEvent::Toggle => {
                info!("Media key event received: Toggle");
                let state = state.lock().unwrap();
                info!("Media key Toggle pressed, current state: {:?}", *state);
                match *state {
                    PlaybackState::Playing { .. } => {
                        info!("Media key: Pausing playback");
                        playback.pause();
                    }
                    PlaybackState::Paused { .. } => {
                        info!("Media key: Resuming playback");
                        playback.resume();
                    }
                    PlaybackState::Stopped | PlaybackState::Loading { .. } => {
                        info!("Media key: Ignored (stopped or loading)");
                    }
                }
            }
            MediaControlEvent::Play => {
                info!("Media control event received: Play");
                let state = state.lock().unwrap();
                match *state {
                    PlaybackState::Paused { .. } => {
                        info!("Media control: Resuming playback");
                        playback.resume();
                    }
                    PlaybackState::Stopped | PlaybackState::Loading { .. } => {
                        info!("Media control: Ignored (stopped or loading)");
                    }
                    PlaybackState::Playing { .. } => {
                        info!("Media control: Already playing, ignoring Play");
                    }
                }
            }
            MediaControlEvent::Pause => {
                info!("Media control event received: Pause");
                let state = state.lock().unwrap();
                match *state {
                    PlaybackState::Playing { .. } => {
                        info!("Media control: Pausing playback");
                        playback.pause();
                    }
                    PlaybackState::Paused { .. }
                    | PlaybackState::Stopped
                    | PlaybackState::Loading { .. } => {
                        info!("Media control: Already paused/stopped, ignoring Pause");
                    }
                }
            }
            MediaControlEvent::Next => {
                info!("Media key event received: Next");
                playback.next();
            }
            MediaControlEvent::Previous => {
                info!("Media key event received: Previous");
                playback.previous();
            }
            MediaControlEvent::SetPosition(media_position) => {
                let position = media_position.0;
                info!("Media control: SetPosition requested: {:?}", position);
                playback.seek(position);
            }
            MediaControlEvent::Stop => {
                info!("Media key event received: Stop");
                playback.stop();
            }
            _ => {
                info!("Media key event received: {:?}", event);
            }
        }
    })?;
    let controls_shared = Arc::new(Mutex::new(controls));
    {
        let controls_shared = controls_shared.clone();
        runtime_handle.spawn(async move {
            let mut progress_rx = playback_handle_for_progress.subscribe_progress();
            let current_state = current_state_for_progress;
            let library_manager = library_manager_for_metadata;
            while let Some(progress) = progress_rx.recv().await {
                match progress {
                    PlaybackProgress::StateChanged { state } => {
                        info!("Media controls: Received state change: {:?}", state);
                        {
                            let mut state_guard = current_state.lock().unwrap();
                            *state_guard = state.clone();
                            info!(
                                "Media controls: Updated tracked state to: {:?}",
                                *state_guard
                            );
                        }
                        {
                            let mut controls = controls_shared.lock().unwrap();
                            let playback_state = match state {
                                PlaybackState::Playing { position, .. } => MediaPlayback::Playing {
                                    progress: Some(MediaPosition(position)),
                                },
                                PlaybackState::Paused { position, .. } => MediaPlayback::Paused {
                                    progress: Some(MediaPosition(position)),
                                },
                                PlaybackState::Stopped | PlaybackState::Loading { .. } => {
                                    MediaPlayback::Stopped
                                }
                            };
                            if let Err(e) = controls.set_playback(playback_state) {
                                error!("Failed to set playback state: {:?}", e);
                            } else {
                                info!("Media controls: Set playback state successfully");
                            }
                        }
                        match state {
                            PlaybackState::Playing {
                                ref track,
                                duration,
                                ..
                            }
                            | PlaybackState::Paused {
                                ref track,
                                duration,
                                ..
                            } => {
                                update_media_metadata(
                                    &controls_shared,
                                    &library_manager,
                                    track,
                                    duration,
                                )
                                .await;
                            }
                            PlaybackState::Stopped | PlaybackState::Loading { .. } => {
                                let mut controls = controls_shared.lock().unwrap();
                                if let Err(e) = controls.set_metadata(MediaMetadata::default()) {
                                    error!("Failed to clear media metadata: {:?}", e);
                                }
                            }
                        }
                    }
                    PlaybackProgress::PositionUpdate { position, .. } => {
                        update_playback_position(&controls_shared, &current_state, position);
                    }
                    PlaybackProgress::Seeked { position, .. } => {
                        update_playback_position(&controls_shared, &current_state, position);
                    }
                    _ => {}
                }
            }
        });
    }
    info!("Media controls initialized");
    Ok(controls_shared)
}
/// Update playback position in macOS media controls
fn update_playback_position(
    controls_shared: &Arc<Mutex<MediaControls>>,
    current_state: &Arc<Mutex<PlaybackState>>,
    position: std::time::Duration,
) {
    let state_guard = current_state.lock().unwrap();
    let playback_state = match *state_guard {
        PlaybackState::Playing { .. } => MediaPlayback::Playing {
            progress: Some(MediaPosition(position)),
        },
        PlaybackState::Paused { .. } => MediaPlayback::Paused {
            progress: Some(MediaPosition(position)),
        },
        PlaybackState::Stopped | PlaybackState::Loading { .. } => {
            return;
        }
    };
    drop(state_guard);
    let mut controls = controls_shared.lock().unwrap();
    if let Err(e) = controls.set_playback(playback_state) {
        error!("Failed to update playback position: {:?}", e);
    } else {
        trace!(
            "Media controls: Updated playback position to {:?}",
            position
        );
    }
}
/// Update media metadata in system media controls
async fn update_media_metadata(
    controls: &Arc<Mutex<MediaControls>>,
    library_manager: &SharedLibraryManager,
    track: &bae_core::db::DbTrack,
    duration: Option<std::time::Duration>,
) {
    let artist_name = match library_manager.get().get_artists_for_track(&track.id).await {
        Ok(artists) => {
            if artists.is_empty() {
                None
            } else if artists.len() == 1 {
                Some(artists[0].name.clone())
            } else {
                Some(
                    artists
                        .iter()
                        .map(|a| a.name.as_str())
                        .collect::<Vec<_>>()
                        .join(", "),
                )
            }
        }
        Err(e) => {
            error!("Failed to fetch artists for track {}: {}", track.id, e);
            None
        }
    };
    let (album_name, cover_image_id, cover_art_url) = match library_manager
        .get()
        .get_album_id_for_release(&track.release_id)
        .await
    {
        Ok(album_id) => match library_manager.get().get_album_by_id(&album_id).await {
            Ok(Some(album)) => (Some(album.title), album.cover_image_id, album.cover_art_url),
            Ok(None) => {
                error!(
                    "Album {} not found for release {}",
                    album_id, track.release_id
                );
                (None, None, None)
            }
            Err(e) => {
                error!("Failed to fetch album {}: {}", album_id, e);
                (None, None, None)
            }
        },
        Err(e) => {
            error!(
                "Failed to get album ID for release {}: {}",
                track.release_id, e
            );
            (None, None, None)
        }
    };

    let cover_url = resolve_cover_file_url(library_manager, &track.release_id, cover_image_id)
        .await
        .or(cover_art_url);
    let title = track.title.clone();
    let artist_str = artist_name.as_deref();
    let album_str = album_name.as_deref();
    let cover_str = cover_url.as_deref();
    let metadata = MediaMetadata {
        title: Some(title.as_str()),
        artist: artist_str,
        album: album_str,
        cover_url: cover_str,
        duration,
    };
    let mut controls = controls.lock().unwrap();
    if let Err(e) = controls.set_metadata(metadata) {
        error!("Failed to set media metadata: {:?}", e);
    } else {
        trace!(
            "Updated media metadata: track={}, artist={:?}, album={:?}, cover_url={:?}",
            track.title,
            artist_name,
            album_name,
            cover_url
        );
    }
}

/// Resolve cover art to a file:// URL for macOS media controls.
/// Downloads from cloud and/or decrypts if needed, caching the result.
async fn resolve_cover_file_url(
    library_manager: &SharedLibraryManager,
    release_id: &str,
    cover_image_id: Option<String>,
) -> Option<String> {
    let image_id = cover_image_id?;

    // Check if we can use the file directly (local unencrypted)
    if let Some(path) = get_direct_file_path(library_manager, release_id, &image_id).await {
        return Some(format!("file://{}", path));
    }

    // For cloud or encrypted files, we need to cache a decrypted copy
    let cover_path = get_media_cover_cache_path(&image_id);

    // Check if already cached
    if cover_path.exists() {
        return Some(format!("file://{}", cover_path.display()));
    }

    // Fetch bytes (handles S3 download + decryption)
    let data = match library_manager.get().fetch_image_bytes(&image_id).await {
        Ok(d) => d,
        Err(e) => {
            error!("Failed to fetch cover image: {}", e);
            return None;
        }
    };

    // Write to cache
    if let Some(parent) = cover_path.parent() {
        if let Err(e) = tokio::fs::create_dir_all(parent).await {
            error!("Failed to create cover cache dir: {}", e);
            return None;
        }
    }

    if let Err(e) = tokio::fs::write(&cover_path, &data).await {
        error!("Failed to write cover to cache: {}", e);
        return None;
    }

    Some(format!("file://{}", cover_path.display()))
}

/// Check if the image can be served directly from a local unencrypted file.
/// Returns the file path if so, None otherwise.
async fn get_direct_file_path(
    library_manager: &SharedLibraryManager,
    release_id: &str,
    image_id: &str,
) -> Option<String> {
    let image = library_manager
        .get()
        .get_image_by_id(image_id)
        .await
        .ok()??;

    let filename_only = std::path::Path::new(&image.filename)
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or(&image.filename);

    let file = library_manager
        .get()
        .get_file_by_release_and_filename(release_id, filename_only)
        .await
        .ok()??;

    let source_path = file.source_path?;

    // Cloud storage needs download
    if source_path.starts_with("s3://") {
        return None;
    }

    // Check if encrypted
    if let Ok(Some(profile)) = library_manager
        .get()
        .get_storage_profile_for_release(release_id)
        .await
    {
        if profile.encrypted {
            return None;
        }
    }

    Some(source_path)
}

/// Get the cache path for a media control cover image
fn get_media_cover_cache_path(image_id: &str) -> PathBuf {
    dirs::home_dir()
        .expect("Failed to get home directory")
        .join(".bae")
        .join("media-covers")
        .join(image_id)
}
