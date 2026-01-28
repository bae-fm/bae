//! Track row component - displays a single track in the tracklist
//!
//! Accepts `ReadStore<Track>` for per-track reactivity.
//! Only this row re-renders when its track's import state changes.

use crate::components::icons::{EllipsisIcon, PauseIcon, PlayIcon};
use crate::components::utils::format_duration;
use crate::components::{ChromelessButton, Dropdown, Placement};
use crate::display_types::{Artist, TrackImportState};
use dioxus::prelude::*;

/// Individual track row component - reads from its track store for granular reactivity
#[component]
pub fn TrackRow(
    // Track data - ReadStore for per-track reactivity
    track: ReadStore<crate::display_types::Track>,
    artists: Vec<Artist>,
    release_id: String,
    // Album context
    is_compilation: bool,
    // Playback state (from external playback store)
    is_playing: bool,
    is_paused: bool,
    is_loading: bool,
    show_spinner: bool,
    // Callbacks
    on_play: EventHandler<String>,
    on_pause: EventHandler<()>,
    on_resume: EventHandler<()>,
    on_add_next: EventHandler<String>,
    on_add_to_queue: EventHandler<String>,
    on_export: EventHandler<String>,
) -> Element {
    // Read track data at this leaf level
    let track = track.read();

    let is_active = is_playing || is_paused;

    // Determine availability from import state
    let is_available = match track.import_state {
        TrackImportState::Complete => true,
        TrackImportState::Importing(_) => false,
        TrackImportState::None => track.is_available,
    };

    // Get import progress percentage if importing
    let import_progress = match track.import_state {
        TrackImportState::Importing(pct) => Some(pct),
        _ => None,
    };

    let row_class = if is_available {
        if is_active {
            "relative flex items-center py-2 px-4 rounded-lg group overflow-clip bg-blue-500/10 hover:bg-blue-500/15 transition-colors cursor-pointer"
        } else {
            "relative flex items-center py-2 px-4 rounded-lg group overflow-clip hover:bg-gray-700 transition-colors cursor-pointer"
        }
    } else {
        "relative flex items-center py-2 px-4 rounded-lg group overflow-clip"
    };

    // For styling: unavailable tracks look like "importing"
    let is_importing = !is_available;

    let track_id = track.id.clone();
    let track_id_for_play = track_id.clone();
    let track_id_for_menu = track_id.clone();

    rsx! {
        div { class: "{row_class}",
            // Play/pause button area
            if is_available {
                if show_spinner {
                    div { class: "w-6 flex items-center justify-center",
                        div { class: "animate-spin rounded-full h-4 w-4 border-b-2 border-blue-400" }
                    }
                } else if is_playing {
                    ChromelessButton {
                        class: Some(
                            "w-6 h-6 rounded-full border border-blue-400 opacity-0 group-hover:opacity-100 transition-opacity flex items-center justify-center text-blue-400 hover:text-blue-300 hover:bg-blue-400/10"
                                .to_string(),
                        ),
                        aria_label: Some("Pause".to_string()),
                        onclick: move |_| on_pause.call(()),
                        PauseIcon { class: "w-3 h-3" }
                    }
                } else if is_paused {
                    ChromelessButton {
                        class: Some(
                            "w-6 h-6 rounded-full border border-blue-400 flex items-center justify-center text-blue-400 hover:text-blue-300 hover:bg-blue-400/10 transition-colors"
                                .to_string(),
                        ),
                        aria_label: Some("Resume".to_string()),
                        onclick: move |_| on_resume.call(()),
                        PlayIcon { class: "w-3 h-3" }
                    }
                } else {
                    ChromelessButton {
                        class: Some(
                            "w-6 h-6 rounded-full border border-blue-400 opacity-0 group-hover:opacity-100 transition-opacity flex items-center justify-center text-blue-400 hover:text-blue-300 hover:bg-blue-400/10"
                                .to_string(),
                        ),
                        aria_label: Some("Play".to_string()),
                        onclick: {
                            let track_id = track_id_for_play.clone();
                            move |_| on_play.call(track_id.clone())
                        },
                        PlayIcon { class: "w-3 h-3" }
                    }
                }
            } else {
                div { class: "w-6" }
            }

            // Track number
            div {
                class: "w-12 text-right text-sm font-mono",
                class: if is_importing { "text-gray-600" } else { "text-gray-500" },
                if let Some(track_num) = track.track_number {
                    "{track_num}."
                } else {
                    "—"
                }
            }

            // Track title and artists
            div { class: "flex-1 min-w-0 ml-4 mr-4",
                h3 {
                    class: "font-medium transition-colors",
                    class: if is_importing { "text-gray-500" } else if is_active { "text-blue-300" } else { "text-white group-hover:text-blue-300" },
                    "{track.title}"
                }
                if is_compilation && !artists.is_empty() {
                    p {
                        class: "text-sm",
                        class: if is_importing { "text-gray-600" } else { "text-gray-400" },
                        {
                            if artists.len() == 1 {
                                artists[0].name.clone()
                            } else {
                                artists.iter().map(|a| a.name.as_str()).collect::<Vec<_>>().join(", ")
                            }
                        }
                    }
                }
            }

            // Duration / Import progress
            div {
                class: "text-sm font-mono",
                class: if is_importing { "text-gray-600" } else { "text-gray-400" },
                if let Some(pct) = import_progress {
                    // Show import progress percentage
                    "{pct}%"
                } else if let Some(duration_ms) = track.duration_ms {
                    {format_duration(duration_ms)}
                } else {
                    "—:—"
                }
            }

            // Context menu
            if is_available {
                TrackMenu {
                    track_id: track_id_for_menu,
                    on_export,
                    on_add_next,
                    on_add_to_queue,
                }
            }
        }
    }
}

/// Track context menu (export, play next, add to queue)
#[component]
fn TrackMenu(
    track_id: String,
    on_export: EventHandler<String>,
    on_add_next: EventHandler<String>,
    on_add_to_queue: EventHandler<String>,
) -> Element {
    let mut show_menu = use_signal(|| false);
    let is_open: ReadSignal<bool> = show_menu.into();
    // Use track_id for anchor to ensure uniqueness even if component is recycled
    let anchor_id = format!("track-menu-{}", track_id);

    rsx! {
        ChromelessButton {
            id: Some(anchor_id.clone()),
            class: Some(
                "px-2 py-1 text-gray-400 hover:text-white opacity-0 group-hover:opacity-100 transition-opacity"
                    .to_string(),
            ),
            aria_label: Some("Track menu".to_string()),
            onclick: move |_| show_menu.set(!show_menu()),
            EllipsisIcon { class: "w-4 h-4" }
        }

        Dropdown {
            anchor_id: anchor_id.clone(),
            is_open,
            on_close: move |_| show_menu.set(false),
            placement: Placement::BottomEnd,
            class: "bg-gray-800 border border-gray-700 rounded shadow-lg min-w-32",
            ChromelessButton {
                class: Some(
                    "w-full text-left px-3 py-2 text-sm hover:bg-gray-700 rounded-none".to_string(),
                ),
                onclick: {
                    let track_id = track_id.clone();
                    move |_| {
                        show_menu.set(false);
                        on_export.call(track_id.clone());
                    }
                },
                "Export File"
            }
            ChromelessButton {
                class: Some(
                    "w-full text-left px-3 py-2 text-sm hover:bg-gray-700 rounded-none".to_string(),
                ),
                onclick: {
                    let track_id = track_id.clone();
                    move |_| {
                        show_menu.set(false);
                        on_add_next.call(track_id.clone());
                    }
                },
                "Play Next"
            }
            ChromelessButton {
                class: Some(
                    "w-full text-left px-3 py-2 text-sm hover:bg-gray-700 rounded-none".to_string(),
                ),
                onclick: {
                    let track_id = track_id.clone();
                    move |_| {
                        show_menu.set(false);
                        on_add_to_queue.call(track_id.clone());
                    }
                },
                "Add to Queue"
            }
        }
    }
}
