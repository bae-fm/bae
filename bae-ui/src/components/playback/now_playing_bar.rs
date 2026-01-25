//! Now Playing Bar view component
//!
//! ## Reactive State Pattern
//! Accepts `ReadStore<PlaybackUiState>` and passes lenses to sub-components.
//! Each sub-component reads only the fields it needs for granular reactivity.

use crate::components::error_toast::ErrorToast;
use crate::components::icons::{MenuIcon, PauseIcon, PlayIcon, SkipBackIcon, SkipForwardIcon};
use crate::stores::playback::{PlaybackStatus, PlaybackUiState, PlaybackUiStateStoreExt};
use dioxus::prelude::*;

/// Now playing bar view - accepts store for granular reactivity
#[component]
pub fn NowPlayingBarView(
    /// Playback state store - sub-components read only what they need
    state: ReadStore<PlaybackUiState>,
    // Callbacks - all required
    on_previous: EventHandler<()>,
    on_pause: EventHandler<()>,
    on_resume: EventHandler<()>,
    on_next: EventHandler<()>,
    on_seek: EventHandler<u64>,
    on_toggle_queue: EventHandler<()>,
    on_track_click: EventHandler<String>,
    #[props(default)] on_dismiss_error: Option<EventHandler<()>>,
) -> Element {
    rsx! {
        div { class: "right-0 bg-gray-800 text-white p-4 border-t border-gray-700",
            div { class: "flex items-center gap-4",
                PlaybackControlsSection {
                    state,
                    on_previous,
                    on_pause,
                    on_resume,
                    on_next,
                }

                AlbumCoverSection { state, on_track_click }

                TrackInfoSection { state, on_track_click }

                PositionSection { state, on_seek }

                button {
                    class: "px-3 py-2 bg-gray-700 rounded hover:bg-gray-600",
                    onclick: move |_| on_toggle_queue.call(()),
                    MenuIcon { class: "w-5 h-5" }
                }
            }
        }

        PlaybackErrorSection { state, on_dismiss_error }
    }
}

/// Playback controls - reads only status
#[component]
fn PlaybackControlsSection(
    state: ReadStore<PlaybackUiState>,
    on_previous: EventHandler<()>,
    on_pause: EventHandler<()>,
    on_resume: EventHandler<()>,
    on_next: EventHandler<()>,
) -> Element {
    // Read only status via lens
    let status = *state.status().read();

    let is_playing = status == PlaybackStatus::Playing;
    let is_loading = status == PlaybackStatus::Loading;
    let is_stopped = status == PlaybackStatus::Stopped;
    let show_spinner = is_loading;

    let main_btn_base = "w-10 h-10 rounded flex items-center justify-center";

    rsx! {
        div { class: "flex items-center gap-2",
            button {
                class: if is_loading { "px-3 py-2 bg-gray-700 rounded opacity-50" } else { "px-3 py-2 bg-gray-700 rounded hover:bg-gray-600" },
                disabled: is_loading,
                onclick: move |_| on_previous.call(()),
                SkipBackIcon { class: "w-5 h-5" }
            }
            if is_playing {
                if show_spinner {
                    button {
                        class: "{main_btn_base} bg-blue-600 opacity-50",
                        disabled: true,
                        div { class: "animate-spin rounded-full h-4 w-4 border-b-2 border-white" }
                    }
                } else {
                    button {
                        class: "{main_btn_base} bg-blue-600 hover:bg-blue-500",
                        onclick: move |_| on_pause.call(()),
                        PauseIcon { class: "w-5 h-5" }
                    }
                }
            } else {
                if is_stopped {
                    button {
                        class: "{main_btn_base} bg-gray-700 opacity-50",
                        disabled: true,
                        PlayIcon { class: "w-5 h-5" }
                    }
                } else if show_spinner {
                    button {
                        class: "{main_btn_base} bg-green-600 opacity-50",
                        disabled: true,
                        div { class: "animate-spin rounded-full h-4 w-4 border-b-2 border-white" }
                    }
                } else {
                    button {
                        class: "{main_btn_base} bg-green-600 hover:bg-green-500",
                        onclick: move |_| on_resume.call(()),
                        PlayIcon { class: "w-5 h-5" }
                    }
                }
            }
            button {
                class: if is_loading { "px-3 py-2 bg-gray-700 rounded opacity-50" } else { "px-3 py-2 bg-gray-700 rounded hover:bg-gray-600" },
                disabled: is_loading,
                onclick: move |_| on_next.call(()),
                SkipForwardIcon { class: "w-5 h-5" }
            }
        }
    }
}

/// Album cover thumbnail - reads only cover_url and current_track_id
#[component]
fn AlbumCoverSection(
    state: ReadStore<PlaybackUiState>,
    on_track_click: EventHandler<String>,
) -> Element {
    // Read only the fields we need
    let cover_url = state.cover_url().read().clone();
    let track_id = state.current_track_id().read().clone();

    rsx! {
        div {
            class: "w-10 h-10 bg-gray-700 rounded-sm flex items-center justify-center overflow-hidden flex-shrink-0 cursor-pointer hover:opacity-80 transition-opacity",
            onclick: move |_| {
                if let Some(ref id) = track_id {
                    on_track_click.call(id.clone());
                }
            },
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

/// Track info display - reads current_track, artist_name, status
#[component]
fn TrackInfoSection(
    state: ReadStore<PlaybackUiState>,
    on_track_click: EventHandler<String>,
) -> Element {
    // Read only the fields we need
    let current_track = state.current_track().read().clone();
    let artist_name = state.artist_name().read().clone();
    let status = *state.status().read();
    let is_loading = status == PlaybackStatus::Loading;

    let track = current_track.map(|qi| qi.track);
    let track_id = track.as_ref().map(|t| t.id.clone());

    rsx! {
        div { class: "flex-1",
            if let Some(ref track) = track {
                div {
                    class: "font-semibold cursor-pointer hover:text-blue-300 transition-colors",
                    onclick: move |_| {
                        if let Some(ref id) = track_id {
                            on_track_click.call(id.clone());
                        }
                    },
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

/// Position/seek bar - reads position_ms, duration_ms, pregap_ms
#[component]
fn PositionSection(state: ReadStore<PlaybackUiState>, on_seek: EventHandler<u64>) -> Element {
    // Read position fields via lenses
    let position_ms = *state.position_ms().read();
    let duration_ms = *state.duration_ms().read();
    let pregap_ms = *state.pregap_ms().read();

    // Local position used during and briefly after seeking to prevent flicker
    let mut seek_position_ms = use_signal(|| None::<u64>);
    let mut is_seeking = use_signal(|| false);

    // Clear seek position once the actual position catches up (within 500ms tolerance)
    if let Some(seek_pos) = seek_position_ms() {
        if !is_seeking() && (position_ms as i64 - seek_pos as i64).abs() < 500 {
            seek_position_ms.set(None);
        }
    }

    // Use seek position if set, otherwise use the prop
    let display_position_ms = seek_position_ms().unwrap_or(position_ms);

    let has_position = position_ms > 0 || duration_ms > 0;

    rsx! {
        if has_position {
            div { class: "flex items-center gap-2 text-sm text-gray-400",
                span { class: "w-12 text-right", "{format_display_time(display_position_ms, pregap_ms)}" }
                if duration_ms > 0 {
                    {
                        let pregap = pregap_ms.unwrap_or(0).max(0) as u64;
                        let adjusted_pos = display_position_ms.saturating_sub(pregap);
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
                                onmousedown: move |_| {
                                    is_seeking.set(true);
                                    seek_position_ms.set(Some(position_ms));
                                },
                                onmouseup: move |_| {
                                    if is_seeking() {
                                        if let Some(pos) = seek_position_ms() {
                                            on_seek.call(pos);
                                        }
                                        is_seeking.set(false);
                                    }
                                },
                                oninput: move |evt| {
                                    if let Ok(secs) = evt.value().parse::<u64>() {
                                        let pregap_ms_val = pregap_ms.unwrap_or(0).max(0) as u64;
                                        seek_position_ms.set(Some(secs * 1000 + pregap_ms_val));
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

/// Playback error toast - reads only playback_error
#[component]
fn PlaybackErrorSection(
    state: ReadStore<PlaybackUiState>,
    on_dismiss_error: Option<EventHandler<()>>,
) -> Element {
    // Read only error via lens
    let error = state.playback_error().read().clone();

    if let Some(error_msg) = error {
        rsx! {
            ErrorToast {
                title: None,
                message: error_msg,
                on_dismiss: move |_| {
                    if let Some(handler) = on_dismiss_error {
                        handler.call(());
                    }
                },
            }
        }
    } else {
        rsx! {}
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
