//! Now Playing Bar view component
//!
//! Pure, props-based component for displaying current playback state.

use crate::components::error_toast::ErrorToast;
use crate::components::icons::{MenuIcon, PauseIcon, PlayIcon, SkipBackIcon, SkipForwardIcon};
use crate::display_types::{PlaybackDisplay, Track};
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
    // Error display
    #[props(default)] playback_error: Option<String>,
    #[props(default)] on_dismiss_error: Option<EventHandler<()>>,
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
                    MenuIcon { class: "w-5 h-5" }
                }
            }
        }
        if let Some(error) = playback_error {
            ErrorToast {
                title: None,
                message: error,
                on_dismiss: move |_| {
                    if let Some(handler) = on_dismiss_error {
                        handler.call(());
                    }
                },
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
    // Show spinner immediately when loading (no delay in shared component)
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
                                        // Keep seek_position_ms set - it will clear once position catches up
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
