use super::queue_sidebar::QueueSidebarState;
use super::use_playback_service;
use crate::db::DbTrack;
use crate::library::use_library_manager;
use crate::playback::{PlaybackProgress, PlaybackState};
use crate::ui::{image_url, Route};
use dioxus::prelude::*;
#[component]
fn PlaybackControlsZone(
    on_previous: EventHandler<()>,
    on_pause: EventHandler<()>,
    on_resume: EventHandler<()>,
    on_next: EventHandler<()>,
    is_playing: ReadSignal<bool>,
    is_paused: ReadSignal<bool>,
    is_loading: ReadSignal<bool>,
    is_stopped: ReadSignal<bool>,
) -> Element {
    rsx! {
        div { class: "flex items-center gap-2",
            button {
                class: if is_loading() { "px-3 py-2 bg-gray-700 rounded opacity-50" } else { "px-3 py-2 bg-gray-700 rounded hover:bg-gray-600" },
                disabled: is_loading(),
                onclick: move |_| on_previous.call(()),
                "⏮"
            }
            if is_playing() {
                if is_loading() {
                    button {
                        class: "px-4 py-2 bg-blue-600 rounded opacity-50 flex items-center justify-center",
                        disabled: true,
                        div { class: "animate-spin rounded-full h-4 w-4 border-b-2 border-white" }
                    }
                } else {
                    button {
                        class: "px-4 py-2 bg-blue-600 rounded hover:bg-blue-500",
                        onclick: move |_| on_pause.call(()),
                        "⏸"
                    }
                }
            } else {
                if is_stopped() {
                    button {
                        class: "px-4 py-2 bg-gray-700 rounded opacity-50",
                        disabled: true,
                        "▶"
                    }
                } else if is_loading() {
                    button {
                        class: "px-4 py-2 bg-green-600 rounded opacity-50 flex items-center justify-center",
                        disabled: true,
                        div { class: "animate-spin rounded-full h-4 w-4 border-b-2 border-white" }
                    }
                } else {
                    button {
                        class: "px-4 py-2 bg-green-600 rounded hover:bg-green-500",
                        onclick: move |_| on_resume.call(()),
                        "▶"
                    }
                }
            }
            button {
                class: if is_loading() { "px-3 py-2 bg-gray-700 rounded opacity-50" } else { "px-3 py-2 bg-gray-700 rounded hover:bg-gray-600" },
                disabled: is_loading(),
                onclick: move |_| on_next.call(()),
                "⏭"
            }
        }
    }
}
#[component]
fn AlbumCoverThumbnail(
    cover_url: ReadSignal<Option<String>>,
    track: ReadSignal<Option<DbTrack>>,
) -> Element {
    let library_manager = use_library_manager();
    rsx! {
        div {
            class: "w-10 h-10 bg-gray-700 rounded-sm flex items-center justify-center overflow-hidden flex-shrink-0 cursor-pointer hover:opacity-80 transition-opacity",
            onclick: {
                let library_manager = library_manager.clone();
                let navigator = navigator();
                move |_| {
                    let track = track();
                    if let Some(track) = track {
                        let track = track.clone();
                        let library_manager = library_manager.clone();
                        spawn(async move {
                            if let Ok(album_id) = library_manager
                                .get()
                                .get_album_id_for_release(&track.release_id)
                                .await
                            {
                                navigator
                                    .push(Route::AlbumDetail {
                                        album_id,
                                        release_id: track.release_id.clone(),
                                    });
                            }
                        });
                    }
                }
            },
            if let Some(url) = cover_url() {
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
fn TrackInfoZone(
    track: ReadSignal<Option<DbTrack>>,
    artist_name: ReadSignal<String>,
    is_loading: ReadSignal<bool>,
) -> Element {
    let library_manager = use_library_manager();
    rsx! {
        div { class: "flex-1",
            if let Some(track) = track() {
                div {
                    class: "font-semibold cursor-pointer hover:text-blue-300 transition-colors",
                    onclick: {
                        let track = track.clone();
                        let library_manager = library_manager.clone();
                        let navigator = navigator();
                        move |_| {
                            let track = track.clone();
                            let library_manager = library_manager.clone();
                            spawn(async move {
                                if let Ok(album_id) = library_manager
                                    .get()
                                    .get_album_id_for_release(&track.release_id)
                                    .await
                                {
                                    navigator
                                        .push(Route::AlbumDetail {
                                            album_id,
                                            release_id: track.release_id.clone(),
                                        });
                                }
                            });
                        }
                    },
                    "{track.title}"
                }
                div { class: "text-sm text-gray-400", "{artist_name()}" }
            } else if is_loading() {
                div { class: "font-semibold text-gray-400", "Loading..." }
                div { class: "text-sm text-gray-500", "Loading" }
            } else {
                div { class: "font-semibold text-gray-400", "No track playing" }
                div { class: "text-sm text-gray-500", "" }
            }
        }
    }
}
fn format_duration(duration: std::time::Duration) -> String {
    let total_secs = duration.as_secs();
    let mins = total_secs / 60;
    let secs = total_secs % 60;
    format!("{:02}:{:02}", mins, secs)
}

/// Format duration with pregap support (shows negative time during pregap)
fn format_display_time(position_ms: u64, pregap_ms: Option<i64>) -> String {
    let pregap = pregap_ms.unwrap_or(0) as u64;
    if position_ms < pregap {
        // In pregap: show negative time
        let remaining_ms = pregap - position_ms;
        let total_secs = remaining_ms / 1000;
        let mins = total_secs / 60;
        let secs = total_secs % 60;
        format!("-{:02}:{:02}", mins, secs)
    } else {
        // Past pregap: show normal time (position - pregap)
        let adjusted_ms = position_ms - pregap;
        let total_secs = adjusted_ms / 1000;
        let mins = total_secs / 60;
        let secs = total_secs % 60;
        format!("{:02}:{:02}", mins, secs)
    }
}

#[component]
fn PositionZone(
    position: ReadSignal<Option<std::time::Duration>>,
    duration: ReadSignal<Option<std::time::Duration>>,
    pregap_ms: ReadSignal<Option<i64>>,
    is_paused: ReadSignal<bool>,
    on_seek: EventHandler<std::time::Duration>,
    is_seeking: Signal<bool>,
) -> Element {
    let mut local_position = use_signal(|| *position.read());
    use_effect(move || {
        if !is_seeking() {
            local_position.set(*position.read());
        }
    });
    rsx! {
        if let Some(pos) = local_position() {
            div { class: "flex items-center gap-2 text-sm text-gray-400",
                span { class: "w-12 text-right",
                    "{format_display_time(pos.as_millis() as u64, pregap_ms())}"
                }
                if let Some(duration) = duration() {
                    {
                        // Calculate progress bar position with pregap offset
                        let pregap = pregap_ms().unwrap_or(0).max(0) as u64;
                        let pos_ms = pos.as_millis() as u64;
                        let adjusted_pos = pos_ms.saturating_sub(pregap);
                        let duration_ms = duration.as_millis() as u64;
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
                                max: "{duration.as_secs()}",
                                value: "{adjusted_pos / 1000}",
                                onmousedown: move |_| {
                                    is_seeking.set(true);
                                },
                                onmouseup: move |_| {
                                    if is_seeking() {
                                        spawn(async move {
                                            tokio::time::sleep(std::time::Duration::from_millis(50)).await;
                                            if is_seeking() {
                                                is_seeking.set(false);
                                            }
                                        });
                                    }
                                },
                                oninput: move |evt| {
                                    if let Ok(secs) = evt.value().parse::<u64>() {
                                        // Add pregap back when user scrubs
                                        let pregap_secs = pregap_ms().unwrap_or(0).max(0) as u64 / 1000;
                                        local_position.set(Some(std::time::Duration::from_secs(secs + pregap_secs)));
                                    }
                                },
                                onchange: move |evt| {
                                    if let Ok(secs) = evt.value().parse::<u64>() {
                                        // Add pregap back when seeking
                                        let pregap_secs = pregap_ms().unwrap_or(0).max(0) as u64 / 1000;
                                        on_seek.call(std::time::Duration::from_secs(secs + pregap_secs));
                                    }
                                },
                            }
                            span { class: "w-12", "{format_duration(duration)}" }
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
#[component]
pub fn NowPlayingBar() -> Element {
    let playback = use_playback_service();
    let library_manager = use_library_manager();
    let mut state = use_signal(|| PlaybackState::Stopped);
    let mut current_artist = use_signal(|| "Unknown Artist".to_string());
    let mut cover_art_url = use_signal(|| Option::<String>::None);
    let mut is_seeking = use_signal(|| false);
    let mut playback_error = use_signal(|| Option::<String>::None);
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
                        PlaybackProgress::SeekError {
                            requested_position: _,
                            track_duration: _,
                        } => {
                            tracing::warn!("Seek failed: requested position past track end");
                        }
                        PlaybackProgress::Seeked {
                            position,
                            track_id: _,
                            was_paused,
                        } => {
                            if is_seeking() {
                                is_seeking.set(false);
                            }
                            match state() {
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
                        PlaybackProgress::SeekSkipped {
                            requested_position: _,
                            current_position: _,
                        } => {
                            if is_seeking() {
                                is_seeking.set(false);
                            }
                        }
                        PlaybackProgress::StateChanged { state: new_state } => {
                            state.set(new_state.clone());
                            if is_seeking() {
                                is_seeking.set(false);
                            }
                            if let PlaybackState::Playing { ref track, .. }
                            | PlaybackState::Paused { ref track, .. } = new_state
                            {
                                let release_id = track.release_id.clone();
                                let library_manager_for_artist = library_manager.clone();
                                spawn(async move {
                                    match library_manager_for_artist
                                        .get()
                                        .get_album_id_for_release(&release_id)
                                        .await
                                    {
                                        Ok(album_id) => {
                                            match library_manager_for_artist
                                                .get()
                                                .get_artists_for_album(&album_id)
                                                .await
                                            {
                                                Ok(artists) => {
                                                    if !artists.is_empty() {
                                                        let artist_names: Vec<_> = artists
                                                            .iter()
                                                            .map(|a| a.name.as_str())
                                                            .collect();
                                                        current_artist.set(artist_names.join(", "));
                                                    } else {
                                                        current_artist
                                                            .set("Unknown Artist".to_string());
                                                    }
                                                }
                                                Err(e) => {
                                                    tracing::error!(
                                                        "Failed to fetch album artists for album {}: {}", album_id,
                                                        e
                                                    );
                                                    current_artist
                                                        .set("Unknown Artist".to_string());
                                                }
                                            }
                                        }
                                        Err(e) => {
                                            tracing::warn!(
                                                "No album found for release {}: {}",
                                                release_id,
                                                e
                                            );
                                            current_artist.set("Unknown Artist".to_string());
                                        }
                                    }
                                });
                            }
                        }
                        PlaybackProgress::PositionUpdate { position, .. } => {
                            if is_seeking() {
                                continue;
                            }
                            if let PlaybackState::Playing {
                                ref track,
                                duration,
                                decoded_duration,
                                pregap_ms,
                                ..
                            } = state()
                            {
                                state.set(PlaybackState::Playing {
                                    track: track.clone(),
                                    position,
                                    duration,
                                    decoded_duration,
                                    pregap_ms,
                                });
                            } else if let PlaybackState::Paused {
                                ref track,
                                duration,
                                decoded_duration,
                                pregap_ms,
                                ..
                            } = state()
                            {
                                state.set(PlaybackState::Paused {
                                    track: track.clone(),
                                    position,
                                    duration,
                                    decoded_duration,
                                    pregap_ms,
                                });
                            }
                        }
                        PlaybackProgress::TrackCompleted { .. } => {}
                        PlaybackProgress::QueueUpdated { .. } => {}
                        PlaybackProgress::PlaybackError { message } => {
                            playback_error.set(Some(message.clone()));
                            spawn(async move {
                                tokio::time::sleep(std::time::Duration::from_secs(5)).await;
                                playback_error.set(None);
                            });
                        }
                    }
                }
            });
        }
    });
    let track = use_memo(move || match state() {
        PlaybackState::Playing { ref track, .. } | PlaybackState::Paused { ref track, .. } => {
            Some(track.clone())
        }
        _ => None,
    });
    let position = use_memo(move || match state() {
        PlaybackState::Playing { position, .. } | PlaybackState::Paused { position, .. } => {
            Some(position)
        }
        _ => None,
    });
    let duration = use_memo(move || match state() {
        PlaybackState::Playing { duration, .. } | PlaybackState::Paused { duration, .. } => {
            duration
        }
        _ => None,
    });
    let pregap_ms = use_memo(move || match state() {
        PlaybackState::Playing { pregap_ms, .. } | PlaybackState::Paused { pregap_ms, .. } => {
            pregap_ms
        }
        _ => None,
    });
    let is_playing = use_memo(move || matches!(state(), PlaybackState::Playing { .. }));
    let is_paused = use_memo(move || matches!(state(), PlaybackState::Paused { .. }));
    let is_loading = use_memo(move || matches!(state(), PlaybackState::Loading { .. }));
    let is_stopped = use_memo(move || matches!(state(), PlaybackState::Stopped));
    use_effect({
        let library_manager = library_manager.clone();
        move || {
            let library_manager = library_manager.clone();
            let track_val = track();
            if let Some(track) = track_val {
                let release_id = track.release_id.clone();
                spawn(async move {
                    match library_manager
                        .get()
                        .get_album_id_for_release(&release_id)
                        .await
                    {
                        Ok(album_id) => {
                            match library_manager.get().get_artists_for_album(&album_id).await {
                                Ok(artists) => {
                                    if !artists.is_empty() {
                                        let artist_names: Vec<_> =
                                            artists.iter().map(|a| a.name.as_str()).collect();
                                        current_artist.set(artist_names.join(", "));
                                    } else {
                                        current_artist.set("Unknown Artist".to_string());
                                    }
                                }
                                Err(e) => {
                                    tracing::error!(
                                        "Failed to fetch album artists for album {}: {}",
                                        album_id,
                                        e
                                    );
                                    current_artist.set("Unknown Artist".to_string());
                                }
                            }
                        }
                        Err(e) => {
                            tracing::warn!("No album found for release {}: {}", release_id, e);
                            current_artist.set("Unknown Artist".to_string());
                        }
                    }
                });
            } else {
                current_artist.set("Unknown Artist".to_string());
            }
        }
    });
    let artist_name = use_memo(move || current_artist.read().clone());
    use_effect({
        let library_manager = library_manager.clone();
        move || {
            let library_manager = library_manager.clone();
            let track_val = track();
            if let Some(track) = track_val {
                let release_id = track.release_id.clone();
                spawn(async move {
                    match library_manager
                        .get()
                        .get_album_id_for_release(&release_id)
                        .await
                    {
                        Ok(album_id) => {
                            match library_manager.get().get_album_by_id(&album_id).await {
                                Ok(Some(album)) => {
                                    let url = album
                                        .cover_image_id
                                        .as_ref()
                                        .map(|id| image_url(id))
                                        .or_else(|| album.cover_art_url.clone());
                                    cover_art_url.set(url);
                                }
                                Ok(None) => {
                                    cover_art_url.set(None);
                                }
                                Err(e) => {
                                    tracing::error!("Failed to fetch album {}: {}", album_id, e);
                                    cover_art_url.set(None);
                                }
                            }
                        }
                        Err(e) => {
                            tracing::warn!("No album found for release {}: {}", release_id, e);
                            cover_art_url.set(None);
                        }
                    }
                });
            } else {
                cover_art_url.set(None);
            }
        }
    });
    let cover_url = use_memo(move || cover_art_url.read().clone());
    let playback_prev = playback.clone();
    let playback_pause = playback.clone();
    let playback_resume = playback.clone();
    let playback_next = playback.clone();
    let playback_seek = playback.clone();
    let mut queue_sidebar_open = use_context::<QueueSidebarState>();
    rsx! {
        div { class: "fixed bottom-0 left-0 right-0 bg-gray-800 text-white p-4 border-t border-gray-700",
            div { class: "flex items-center gap-4",
                PlaybackControlsZone {
                    on_previous: move |_| playback_prev.previous(),
                    on_pause: move |_| playback_pause.pause(),
                    on_resume: move |_| playback_resume.resume(),
                    on_next: move |_| playback_next.next(),
                    is_playing,
                    is_paused,
                    is_loading,
                    is_stopped,
                }
                AlbumCoverThumbnail { cover_url, track }
                TrackInfoZone { track, artist_name, is_loading }
                PositionZone {
                    position,
                    duration,
                    pregap_ms,
                    is_paused,
                    on_seek: move |duration| playback_seek.seek(duration),
                    is_seeking,
                }
                button {
                    class: "px-3 py-2 bg-gray-700 rounded hover:bg-gray-600",
                    onclick: move |_| {
                        let current = *queue_sidebar_open.is_open.read();
                        queue_sidebar_open.is_open.set(!current);
                    },
                    "☰"
                }
            }
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
