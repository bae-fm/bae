//! Now Playing Bar component
//!
//! Props-based component for displaying current playback state.

use super::super::LOADING_SPINNER_DELAY_MS;
use super::queue_sidebar::QueueSidebarState;
use crate::ui::display_types::{PlaybackDisplay, Track};
use dioxus::prelude::*;

/// Now playing bar view (pure, props-based)
/// All callbacks are required - pass noops if not needed.
#[component]
pub fn NowPlayingBarView(
    // Track info
    track: Option<Track>,
    artist_name: String,
    cover_url: Option<String>,
    // Playback state
    playback: PlaybackDisplay,
    position_ms: u64,
    duration_ms: u64,
    #[props(default)] pregap_ms: Option<i64>,
    // Callbacks - all required
    on_previous: EventHandler<()>,
    on_pause: EventHandler<()>,
    on_resume: EventHandler<()>,
    on_next: EventHandler<()>,
    on_seek: EventHandler<u64>,
    on_toggle_queue: EventHandler<()>,
    on_track_click: EventHandler<String>,
) -> Element {
    let is_playing = matches!(playback, PlaybackDisplay::Playing { .. });
    let is_paused = matches!(playback, PlaybackDisplay::Paused { .. });
    let is_loading = matches!(playback, PlaybackDisplay::Loading { .. });
    let is_stopped = matches!(playback, PlaybackDisplay::Stopped);

    rsx! {
        div { class: "fixed bottom-0 left-0 right-0 bg-gray-800 text-white p-4 border-t border-gray-700",
            div { class: "flex items-center gap-4",
                PlaybackControlsView {
                    is_playing,
                    is_paused,
                    is_loading,
                    is_stopped,
                    on_previous,
                    on_pause,
                    on_resume,
                    on_next,
                }

                AlbumCoverThumbnailView {
                    cover_url: cover_url.clone(),
                    on_click: {
                        let track_id = track.as_ref().map(|t| t.id.clone());
                        EventHandler::new(move |_: ()| {
                            if let Some(ref id) = track_id {
                                on_track_click.call(id.clone());
                            }
                        })
                    },
                }

                TrackInfoView {
                    track: track.clone(),
                    artist_name: artist_name.clone(),
                    is_loading,
                    on_click: {
                        let track_id = track.as_ref().map(|t| t.id.clone());
                        EventHandler::new(move |_: ()| {
                            if let Some(ref id) = track_id {
                                on_track_click.call(id.clone());
                            }
                        })
                    },
                }

                PositionView {
                    position_ms,
                    duration_ms,
                    pregap_ms,
                    on_seek,
                }

                button {
                    class: "px-3 py-2 bg-gray-700 rounded hover:bg-gray-600",
                    onclick: move |_| on_toggle_queue.call(()),
                    "☰"
                }
            }
        }
    }
}

#[component]
fn PlaybackControlsView(
    is_playing: bool,
    is_paused: bool,
    is_loading: bool,
    is_stopped: bool,
    on_previous: EventHandler<()>,
    on_pause: EventHandler<()>,
    on_resume: EventHandler<()>,
    on_next: EventHandler<()>,
) -> Element {
    let mut show_spinner = use_signal(|| false);
    let is_loading_signal = use_signal(move || is_loading);

    use_effect(move || {
        if is_loading {
            let is_loading_signal = is_loading_signal;
            spawn(async move {
                tokio::time::sleep(std::time::Duration::from_millis(LOADING_SPINNER_DELAY_MS))
                    .await;
                if is_loading_signal() {
                    show_spinner.set(true);
                }
            });
        } else {
            show_spinner.set(false);
        }
    });

    let main_btn_base = "w-10 h-10 rounded flex items-center justify-center";

    rsx! {
        div { class: "flex items-center gap-2",
            button {
                class: if is_loading { "px-3 py-2 bg-gray-700 rounded opacity-50" } else { "px-3 py-2 bg-gray-700 rounded hover:bg-gray-600" },
                disabled: is_loading,
                onclick: move |_| on_previous.call(()),
                "⏮"
            }
            if is_playing {
                if show_spinner() {
                    button {
                        class: "{main_btn_base} bg-blue-600 opacity-50",
                        disabled: true,
                        div { class: "animate-spin rounded-full h-4 w-4 border-b-2 border-white" }
                    }
                } else {
                    button {
                        class: "{main_btn_base} bg-blue-600 hover:bg-blue-500",
                        onclick: move |_| on_pause.call(()),
                        "⏸"
                    }
                }
            } else {
                if is_stopped {
                    button {
                        class: "{main_btn_base} bg-gray-700 opacity-50",
                        disabled: true,
                        "▶"
                    }
                } else if show_spinner() {
                    button {
                        class: "{main_btn_base} bg-green-600 opacity-50",
                        disabled: true,
                        div { class: "animate-spin rounded-full h-4 w-4 border-b-2 border-white" }
                    }
                } else {
                    button {
                        class: "{main_btn_base} bg-green-600 hover:bg-green-500",
                        onclick: move |_| on_resume.call(()),
                        "▶"
                    }
                }
            }
            button {
                class: if is_loading { "px-3 py-2 bg-gray-700 rounded opacity-50" } else { "px-3 py-2 bg-gray-700 rounded hover:bg-gray-600" },
                disabled: is_loading,
                onclick: move |_| on_next.call(()),
                "⏭"
            }
        }
    }
}

#[component]
fn AlbumCoverThumbnailView(cover_url: Option<String>, on_click: EventHandler<()>) -> Element {
    rsx! {
        div {
            class: "w-10 h-10 bg-gray-700 rounded-sm flex items-center justify-center overflow-hidden flex-shrink-0 cursor-pointer hover:opacity-80 transition-opacity",
            onclick: move |_| on_click.call(()),
            if let Some(ref url) = cover_url {
                img {
                    src: "{url}",
                    alt: "Album cover",
                    class: "w-full h-full object-cover",
                }
            } else {
                div { class: "text-gray-500 text-sm", "" }
            }
        }
    }
}

#[component]
fn TrackInfoView(
    track: Option<Track>,
    artist_name: String,
    is_loading: bool,
    on_click: EventHandler<()>,
) -> Element {
    rsx! {
        div { class: "flex-1",
            if let Some(ref track) = track {
                div {
                    class: "font-semibold cursor-pointer hover:text-blue-300 transition-colors",
                    onclick: move |_| on_click.call(()),
                    "{track.title}"
                }
                div { class: "text-sm text-gray-400", "{artist_name}" }
            } else if is_loading {
                div { class: "font-semibold text-gray-400", "Loading..." }
                div { class: "text-sm text-gray-500", "Loading" }
            } else {
                div { class: "font-semibold text-gray-400", "No track playing" }
                div { class: "text-sm text-gray-500", "" }
            }
        }
    }
}

fn format_duration_ms(ms: u64) -> String {
    let total_secs = ms / 1000;
    let mins = total_secs / 60;
    let secs = total_secs % 60;
    format!("{:02}:{:02}", mins, secs)
}

fn format_display_time(position_ms: u64, pregap_ms: Option<i64>) -> String {
    let pregap = pregap_ms.unwrap_or(0).max(0) as u64;
    if position_ms < pregap {
        let remaining_ms = pregap - position_ms;
        let total_secs = remaining_ms / 1000;
        let mins = total_secs / 60;
        let secs = total_secs % 60;
        format!("-{:02}:{:02}", mins, secs)
    } else {
        let adjusted_ms = position_ms - pregap;
        let total_secs = adjusted_ms / 1000;
        let mins = total_secs / 60;
        let secs = total_secs % 60;
        format!("{:02}:{:02}", mins, secs)
    }
}

#[component]
fn PositionView(
    position_ms: u64,
    duration_ms: u64,
    pregap_ms: Option<i64>,
    on_seek: EventHandler<u64>,
) -> Element {
    let mut local_position_ms = use_signal(|| position_ms);
    let mut is_seeking = use_signal(|| false);

    use_effect(move || {
        if !is_seeking() {
            local_position_ms.set(position_ms);
        }
    });

    let has_position = position_ms > 0 || duration_ms > 0;

    rsx! {
        if has_position {
            div { class: "flex items-center gap-2 text-sm text-gray-400",
                span { class: "w-12 text-right", "{format_display_time(local_position_ms(), pregap_ms)}" }
                if duration_ms > 0 {
                    {
                        let pregap = pregap_ms.unwrap_or(0).max(0) as u64;
                        let adjusted_pos = local_position_ms().saturating_sub(pregap);
                        let progress_percent = if duration_ms > 0 {
                            (adjusted_pos as f64 / duration_ms as f64 * 100.0).min(100.0)
                        } else {
                            0.0
                        };

                        rsx! {
                            input {
                                r#type: "range",
                                class: "w-64 h-2 bg-gray-700 rounded-lg appearance-none cursor-pointer",
                                style: "background: linear-gradient(to right, #3b82f6 0%, #3b82f6 {progress_percent}%, #374151 {progress_percent}%, #374151 100%);",
                                min: "0",
                                max: "{duration_ms / 1000}",
                                value: "{adjusted_pos / 1000}",
                                onmousedown: move |_| is_seeking.set(true),
                                onmouseup: move |_| {
                                    if is_seeking() {
                                        on_seek.call(local_position_ms());
                                        is_seeking.set(false);
                                    }
                                },
                                oninput: move |evt| {
                                    if let Ok(secs) = evt.value().parse::<u64>() {
                                        let pregap_ms_val = pregap_ms.unwrap_or(0).max(0) as u64;
                                        local_position_ms.set(secs * 1000 + pregap_ms_val);
                                    }
                                },
                            }
                            span { class: "w-12", "{format_duration_ms(duration_ms)}" }
                        }
                    }
                } else {
                    div { class: "w-64 h-2 bg-gray-700 rounded-lg",
                        div {
                            class: "h-full bg-blue-600 rounded-lg",
                            style: "width: 50%;",
                        }
                    }
                    span { class: "w-12", "--:--" }
                }
            }
        } else {
            div { class: "w-72" }
        }
    }
}

// ============================================================================
// Wrapper - handles state subscription (non-demo only, included via Navbar cfg)
// ============================================================================

#[cfg(not(feature = "demo"))]
use super::use_playback_service;
#[cfg(not(feature = "demo"))]
use crate::db::DbTrack;
#[cfg(not(feature = "demo"))]
use crate::library::use_library_manager;
#[cfg(not(feature = "demo"))]
use crate::playback::{PlaybackProgress, PlaybackState};
#[cfg(not(feature = "demo"))]
use crate::ui::{image_url, Route};

#[cfg(not(feature = "demo"))]
#[component]
pub fn NowPlayingBar() -> Element {
    let playback = use_playback_service();
    let library_manager = use_library_manager();
    let mut state = use_signal(|| PlaybackState::Stopped);
    let mut current_artist = use_signal(|| "Unknown Artist".to_string());
    let mut cover_art_url = use_signal(|| Option::<String>::None);
    let mut playback_error = use_signal(|| Option::<String>::None);

    // Subscribe to playback progress
    use_effect({
        let playback = playback.clone();
        let library_manager = library_manager.clone();
        move || {
            let playback = playback.clone();
            let library_manager = library_manager.clone();
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
        if let Some(error) = playback_error() {
            div { class: "fixed bottom-20 right-4 bg-red-600 text-white px-6 py-4 rounded-lg shadow-lg z-50 max-w-md",
                div { class: "flex items-center justify-between gap-4",
                    span { {error} }
                    button {
                        class: "text-white hover:text-gray-200",
                        onclick: move |_| playback_error.set(None),
                        "✕"
                    }
                }
            }
        }
    }
}

#[cfg(not(feature = "demo"))]
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

#[cfg(not(feature = "demo"))]
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

#[cfg(not(feature = "demo"))]
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
