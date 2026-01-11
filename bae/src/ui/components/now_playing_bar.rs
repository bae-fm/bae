//! Now Playing Bar component
//!
//! Wrapper component that connects bae-ui's NowPlayingBarView to app state.

use super::playback_hooks::use_playback_service;
use crate::db::DbTrack;
use crate::library::use_library_manager;
use crate::playback::{PlaybackProgress, PlaybackState};
use crate::ui::display_types::{PlaybackDisplay, Track};
use crate::ui::{image_url, Route};
use bae_ui::{NowPlayingBarView, QueueSidebarState};
use dioxus::prelude::*;

/// Now Playing Bar wrapper that handles state subscription
#[component]
pub fn NowPlayingBar() -> Element {
    let playback = use_playback_service();
    let library_manager = use_library_manager();
    let state = use_signal(|| PlaybackState::Stopped);
    let current_artist = use_signal(|| "Unknown Artist".to_string());
    let cover_art_url = use_signal(|| Option::<String>::None);
    let mut playback_error = use_signal(|| Option::<String>::None);

    // Subscribe to playback progress
    use_effect({
        let playback = playback.clone();
        let library_manager = library_manager.clone();
        move || {
            let playback = playback.clone();
            let library_manager = library_manager.clone();
            // Explicitly capture signals for the async block
            let mut state = state;
            let mut current_artist = current_artist;
            let mut cover_art_url = cover_art_url;
            let playback_error = playback_error;
            spawn(async move {
                let mut progress_rx = playback.subscribe_progress();
                while let Some(progress) = progress_rx.recv().await {
                    match progress {
                        PlaybackProgress::StateChanged { state: new_state } => {
                            state.set(new_state.clone());
                            if let PlaybackState::Playing { ref track, .. }
                            | PlaybackState::Paused { ref track, .. } = new_state
                            {
                                load_track_metadata(
                                    &library_manager,
                                    track,
                                    &mut current_artist,
                                    &mut cover_art_url,
                                );
                            }
                        }
                        PlaybackProgress::PositionUpdate { position, .. } => {
                            update_position(&mut state, position);
                        }
                        PlaybackProgress::PlaybackError { message } => {
                            let mut playback_error = playback_error;
                            playback_error.set(Some(message.clone()));
                            spawn(async move {
                                tokio::time::sleep(std::time::Duration::from_secs(5)).await;
                                playback_error.set(None);
                            });
                        }
                        PlaybackProgress::Seeked {
                            position,
                            was_paused,
                            ..
                        } => {
                            update_position_after_seek(&mut state, position, was_paused);
                        }
                        _ => {}
                    }
                }
            });
        }
    });

    // Extract values from state for props
    let track = use_memo(move || match state() {
        PlaybackState::Playing { ref track, .. } | PlaybackState::Paused { ref track, .. } => {
            Some(Track::from(track))
        }
        _ => None,
    });

    let playback_display = use_memo(move || PlaybackDisplay::from(&state()));

    let position_ms = use_memo(move || match state() {
        PlaybackState::Playing { position, .. } | PlaybackState::Paused { position, .. } => {
            position.as_millis() as u64
        }
        _ => 0,
    });

    let duration_ms = use_memo(move || match state() {
        PlaybackState::Playing { duration, .. } | PlaybackState::Paused { duration, .. } => {
            duration.map(|d| d.as_millis() as u64).unwrap_or(0)
        }
        _ => 0,
    });

    let pregap_ms = use_memo(move || match state() {
        PlaybackState::Playing { pregap_ms, .. } | PlaybackState::Paused { pregap_ms, .. } => {
            pregap_ms
        }
        _ => None,
    });

    // Callbacks
    let playback_for_prev = playback.clone();
    let playback_for_pause = playback.clone();
    let playback_for_resume = playback.clone();
    let playback_for_next = playback.clone();
    let playback_for_seek = playback.clone();

    let mut queue_sidebar_open = use_context::<QueueSidebarState>();

    let on_track_click = {
        let library_manager = library_manager.clone();
        move |_track_id: String| {
            let state_val = state();
            if let PlaybackState::Playing { ref track, .. }
            | PlaybackState::Paused { ref track, .. } = state_val
            {
                let release_id = track.release_id.clone();
                let library_manager = library_manager.clone();
                spawn(async move {
                    if let Ok(album_id) = library_manager
                        .get()
                        .get_album_id_for_release(&release_id)
                        .await
                    {
                        navigator().push(Route::AlbumDetail {
                            album_id,
                            release_id,
                        });
                    }
                });
            }
        }
    };

    rsx! {
        NowPlayingBarView {
            track: track(),
            artist_name: current_artist(),
            cover_url: cover_art_url(),
            playback: playback_display(),
            position_ms: position_ms(),
            duration_ms: duration_ms(),
            pregap_ms: pregap_ms(),
            playback_error: playback_error(),
            on_dismiss_error: move |_| playback_error.set(None),
            on_previous: move |_| playback_for_prev.previous(),
            on_pause: move |_| playback_for_pause.pause(),
            on_resume: move |_| playback_for_resume.resume(),
            on_next: move |_| playback_for_next.next(),
            on_seek: move |ms: u64| playback_for_seek.seek(std::time::Duration::from_millis(ms)),
            on_toggle_queue: move |_| {
                let current = *queue_sidebar_open.is_open.read();
                queue_sidebar_open.is_open.set(!current);
            },
            on_track_click,
        }
    }
}

fn load_track_metadata(
    library_manager: &crate::library::SharedLibraryManager,
    track: &DbTrack,
    current_artist: &mut Signal<String>,
    cover_art_url: &mut Signal<Option<String>>,
) {
    let release_id = track.release_id.clone();
    let library_manager = library_manager.clone();
    let mut current_artist = *current_artist;
    let mut cover_art_url = *cover_art_url;
    spawn(async move {
        if let Ok(album_id) = library_manager
            .get()
            .get_album_id_for_release(&release_id)
            .await
        {
            if let Ok(artists) = library_manager.get().get_artists_for_album(&album_id).await {
                if !artists.is_empty() {
                    let names: Vec<_> = artists.iter().map(|a| a.name.as_str()).collect();
                    current_artist.set(names.join(", "));
                } else {
                    current_artist.set("Unknown Artist".to_string());
                }
            }
            if let Ok(Some(album)) = library_manager.get().get_album_by_id(&album_id).await {
                let url = album
                    .cover_image_id
                    .as_ref()
                    .map(|id| image_url(id))
                    .or(album.cover_art_url);
                cover_art_url.set(url);
            }
        }
    });
}

fn update_position(state: &mut Signal<PlaybackState>, position: std::time::Duration) {
    let current = state();
    match current {
        PlaybackState::Playing {
            ref track,
            duration,
            decoded_duration,
            pregap_ms,
            ..
        } => {
            state.set(PlaybackState::Playing {
                track: track.clone(),
                position,
                duration,
                decoded_duration,
                pregap_ms,
            });
        }
        PlaybackState::Paused {
            ref track,
            duration,
            decoded_duration,
            pregap_ms,
            ..
        } => {
            state.set(PlaybackState::Paused {
                track: track.clone(),
                position,
                duration,
                decoded_duration,
                pregap_ms,
            });
        }
        _ => {}
    }
}

fn update_position_after_seek(
    state: &mut Signal<PlaybackState>,
    position: std::time::Duration,
    was_paused: bool,
) {
    let current = state();
    match current {
        PlaybackState::Playing {
            ref track,
            duration,
            decoded_duration,
            pregap_ms,
            ..
        }
        | PlaybackState::Paused {
            ref track,
            duration,
            decoded_duration,
            pregap_ms,
            ..
        } => {
            let new_state = if was_paused {
                PlaybackState::Paused {
                    track: track.clone(),
                    position,
                    duration,
                    decoded_duration,
                    pregap_ms,
                }
            } else {
                PlaybackState::Playing {
                    track: track.clone(),
                    position,
                    duration,
                    decoded_duration,
                    pregap_ms,
                }
            };
            state.set(new_state);
        }
        _ => {}
    }
}
