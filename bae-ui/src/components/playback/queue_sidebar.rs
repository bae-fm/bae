//! Queue Sidebar view component
//!
//! ## Reactive State Pattern
//! Accepts `ReadStore<PlaybackUiState>` and reads fields via lenses.
//! Each section only re-renders when its specific data changes.

use crate::components::icons::{ImageIcon, MenuIcon, XIcon};
use crate::components::utils::format_duration;
use crate::components::{Button, ButtonSize, ButtonVariant, ChromelessButton};
use crate::display_types::QueueItem;
use crate::stores::playback::{PlaybackUiState, PlaybackUiStateStoreExt};
use crate::stores::ui::{SidebarState, SidebarStateStoreExt};
use dioxus::prelude::*;

/// Shared state for queue sidebar visibility
#[derive(Clone)]
pub struct QueueSidebarState {
    pub is_open: Signal<bool>,
}

/// Queue sidebar view - accepts stores for granular reactivity
#[component]
pub fn QueueSidebarView(
    /// Sidebar UI state (for is_open)
    sidebar: ReadStore<SidebarState>,
    /// Playback state store
    playback: ReadStore<PlaybackUiState>,
    // Callbacks
    on_close: EventHandler<()>,
    on_clear: EventHandler<()>,
    on_remove: EventHandler<usize>,
    on_track_click: EventHandler<String>,
) -> Element {
    // Read is_open via lens - only this check re-runs when visibility changes
    let is_open = *sidebar.is_open().read();

    if !is_open {
        return rsx! {};
    }

    rsx! {
        div { class: "fixed top-0 right-0 h-full w-80 bg-gray-900 border-l border-gray-700 z-50 flex flex-col shadow-2xl",
            div { class: "flex-1 overflow-y-auto",
                NowPlayingSection { playback, on_track_click, on_remove }

                UpNextSection { playback, on_track_click, on_remove }
            }

            // Footer with controls
            div { class: "flex items-center justify-between p-4 border-t border-gray-700",
                Button {
                    variant: ButtonVariant::Secondary,
                    size: ButtonSize::Small,
                    onclick: move |_| on_clear.call(()),
                    "Clear"
                }
                Button {
                    variant: ButtonVariant::Secondary,
                    onclick: move |_| on_close.call(()),
                    MenuIcon { class: "w-5 h-5" }
                }
            }
        }
    }
}

/// Now playing section - reads only current_track
#[component]
fn NowPlayingSection(
    playback: ReadStore<PlaybackUiState>,
    on_track_click: EventHandler<String>,
    on_remove: EventHandler<usize>,
) -> Element {
    // Read only current_track via lens
    let current_track = playback.current_track().read().clone();

    rsx! {
        div {
            div { class: "px-4 pt-4 pb-2",
                h3 { class: "text-sm font-semibold text-gray-400 uppercase tracking-wide",
                    "Now playing"
                }
            }
            if let Some(item) = current_track {
                QueueItemView {
                    item,
                    index: 0,
                    is_current: true,
                    on_click: on_track_click,
                    on_remove,
                }
            } else {
                div { class: "px-4 py-3 text-gray-500 text-sm", "Nothing playing" }
            }
        }
    }
}

/// Up next section - reads only queue_items
#[component]
fn UpNextSection(
    playback: ReadStore<PlaybackUiState>,
    on_track_click: EventHandler<String>,
    on_remove: EventHandler<usize>,
) -> Element {
    // Read only queue_items via lens
    let queue = playback.queue_items().read().clone();

    rsx! {
        div {
            div { class: "px-4 pt-4 pb-2",
                h3 { class: "text-sm font-semibold text-gray-400 uppercase tracking-wide",
                    "Up next"
                }
            }
            if !queue.is_empty() {
                for (index , item) in queue.iter().enumerate() {
                    QueueItemView {
                        key: "{item.track.id}",
                        item: item.clone(),
                        index,
                        is_current: false,
                        on_click: on_track_click,
                        on_remove,
                    }
                }
            } else {
                div { class: "px-4 py-3 text-gray-500 text-sm", "No tracks queued" }
            }
        }
    }
}

#[component]
fn QueueItemView(
    item: QueueItem,
    index: usize,
    is_current: bool,
    on_click: EventHandler<String>,
    on_remove: EventHandler<usize>,
) -> Element {
    rsx! {
        div { class: if is_current { "flex items-center gap-3 p-3 border-b border-gray-700 bg-blue-500/10 hover:bg-blue-500/15 group" } else { "flex items-center gap-3 p-3 border-b border-gray-700 hover:bg-gray-800 group" },
            // Album cover
            div { class: "w-12 h-12 flex-shrink-0 bg-gray-700 rounded overflow-hidden",
                if let Some(ref url) = item.cover_url {
                    img {
                        src: "{url}",
                        alt: "Album cover",
                        class: "w-full h-full object-cover",
                    }
                } else {
                    div { class: "w-full h-full flex items-center justify-center text-gray-500",
                        ImageIcon { class: "w-6 h-6" }
                    }
                }
            }
            // Track info
            div { class: "flex-1 min-w-0",
                div { class: "flex items-center gap-2",
                    ChromelessButton {
                        class: Some(
                            if is_current {
                                "font-medium text-blue-300 hover:text-blue-200 text-left truncate flex-1"
                                    .to_string()
                            } else {
                                "font-medium text-white hover:text-blue-300 text-left truncate flex-1"
                                    .to_string()
                            },
                        ),
                        onclick: {
                            let track_id = item.track.id.clone();
                            move |_| on_click.call(track_id.clone())
                        },
                        "{item.track.title}"
                    }
                    span { class: "text-sm text-gray-400 flex-shrink-0",
                        if let Some(duration_ms) = item.track.duration_ms {
                            {format_duration(duration_ms)}
                        } else {
                            "—:—"
                        }
                    }
                }
                div { class: "text-sm text-gray-400 truncate", "{item.album_title}" }
            }
            // Remove button (only for non-current tracks)
            if !is_current {
                ChromelessButton {
                    class: Some(
                        "px-2 py-1 text-gray-400 hover:text-red-400 rounded opacity-0 group-hover:opacity-100 transition-opacity"
                            .to_string(),
                    ),
                    aria_label: Some("Remove from queue".to_string()),
                    onclick: move |_| on_remove.call(index),
                    XIcon { class: "w-4 h-4" }
                }
            }
        }
    }
}
