//! Queue Sidebar component
//!
//! Props-based component for displaying the playback queue.

use super::album_detail::utils::format_duration;
use crate::ui::display_types::QueueItem;
use dioxus::prelude::*;

/// Shared state for queue sidebar visibility
#[derive(Clone)]
pub struct QueueSidebarState {
    pub is_open: Signal<bool>,
}

/// Queue sidebar view (pure, props-based)
/// All callbacks are required - pass noops if not needed.
#[component]
pub fn QueueSidebarView(
    is_open: bool,
    current_track: Option<QueueItem>,
    queue: Vec<QueueItem>,
    current_track_id: Option<String>,
    // Callbacks - all required
    on_close: EventHandler<()>,
    on_clear: EventHandler<()>,
    on_remove: EventHandler<usize>,
    on_track_click: EventHandler<String>,
) -> Element {
    if !is_open {
        return rsx! {};
    }

    rsx! {
        div { class: "fixed top-0 right-0 h-full w-80 bg-gray-900 border-l border-gray-700 z-50 flex flex-col shadow-2xl",
            div { class: "flex-1 overflow-y-auto",
                // Now playing section
                div {
                    div { class: "px-4 pt-4 pb-2",
                        h3 { class: "text-sm font-semibold text-gray-400 uppercase tracking-wide",
                            "Now playing"
                        }
                    }
                    if let Some(ref item) = current_track {
                        QueueItemView {
                            item: item.clone(),
                            index: 0,
                            is_current: true,
                            on_click: on_track_click,
                            on_remove,
                        }
                    } else {
                        div { class: "px-4 py-3 text-gray-500 text-sm", "Nothing playing" }
                    }
                }
                // Up next section
                div {
                    div { class: "px-4 pt-4 pb-2",
                        h3 { class: "text-sm font-semibold text-gray-400 uppercase tracking-wide",
                            "Up next"
                        }
                    }
                    if !queue.is_empty() {
                        for (index , item) in queue.iter().enumerate() {
                            QueueItemView {
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
            // Footer with controls
            div { class: "flex items-center justify-between p-4 border-t border-gray-700",
                button {
                    class: "px-3 py-2 bg-gray-700 rounded hover:bg-gray-600 text-sm",
                    onclick: move |_| on_clear.call(()),
                    "Clear"
                }
                button {
                    class: "px-3 py-2 bg-gray-700 rounded hover:bg-gray-600",
                    onclick: move |_| on_close.call(()),
                    "☰"
                }
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
                    div { class: "w-full h-full flex items-center justify-center text-gray-500 text-xl",
                        "🎵"
                    }
                }
            }
            // Track info
            div { class: "flex-1 min-w-0",
                div { class: "flex items-center gap-2",
                    button {
                        class: if is_current { "font-medium text-blue-300 hover:text-blue-200 text-left truncate flex-1" } else { "font-medium text-white hover:text-blue-300 text-left truncate flex-1" },
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
                button {
                    class: "px-2 py-1 text-sm text-gray-400 hover:text-red-400 rounded opacity-0 group-hover:opacity-100 transition-opacity",
                    onclick: move |_| on_remove.call(index),
                    "✕"
                }
            }
        }
    }
}

// ============================================================================
// Wrapper - handles state subscription (non-demo only, included via Navbar cfg)
// ============================================================================

#[cfg(not(feature = "demo"))]
use super::playback_hooks::{use_playback_queue, use_playback_service, use_playback_state};
#[cfg(not(feature = "demo"))]
use crate::db::{DbAlbum, DbTrack};
#[cfg(not(feature = "demo"))]
use crate::library::use_library_manager;
#[cfg(not(feature = "demo"))]
use crate::playback::PlaybackState;
#[cfg(not(feature = "demo"))]
use crate::ui::display_types::Track;
#[cfg(not(feature = "demo"))]
use crate::ui::{image_url, Route};

#[cfg(not(feature = "demo"))]
#[component]
pub fn QueueSidebar() -> Element {
    let sidebar_state = use_context::<QueueSidebarState>();
    let mut is_open = sidebar_state.is_open;
    let queue_hook = use_playback_queue();
    let playback_state = use_playback_state();
    let library_manager = use_library_manager();
    let playback = use_playback_service();
    let clear_fn = queue_hook.clear;

    // Current track from playback state
    let current_db_track = use_memo(move || match playback_state() {
        PlaybackState::Playing { ref track, .. } | PlaybackState::Paused { ref track, .. } => {
            Some(track.clone())
        }
        _ => None,
    });

    // Load album info for current track
    let mut current_track_album = use_signal(|| Option::<DbAlbum>::None);
    use_effect({
        let library_manager = library_manager.clone();
        move || {
            if let Some(track) = current_db_track() {
                let track_id = track.id.clone();
                let library_manager = library_manager.clone();
                spawn(async move {
                    if let Ok(album_id) = library_manager
                        .get()
                        .get_album_id_for_track(&track_id)
                        .await
                    {
                        if let Ok(Some(album)) =
                            library_manager.get().get_album_by_id(&album_id).await
                        {
                            current_track_album.set(Some(album));
                        }
                    }
                });
            } else {
                current_track_album.set(None);
            }
        }
    });

    // Queue track IDs
    let queue_track_ids = queue_hook.tracks;

    // Load track/album details for queue
    let mut queue_details =
        use_signal(std::collections::HashMap::<String, (DbTrack, Option<DbAlbum>)>::new);
    use_effect({
        let library_manager = library_manager.clone();
        move || {
            let library_manager = library_manager.clone();
            let queue_val = queue_track_ids.read().clone();
            spawn(async move {
                let mut details = std::collections::HashMap::new();
                for track_id in queue_val.iter() {
                    if let Ok(Some(track)) = library_manager.get().get_track(track_id).await {
                        let album = if let Ok(album_id) =
                            library_manager.get().get_album_id_for_track(track_id).await
                        {
                            library_manager
                                .get()
                                .get_album_by_id(&album_id)
                                .await
                                .ok()
                                .flatten()
                        } else {
                            None
                        };
                        details.insert(track_id.clone(), (track, album));
                    }
                }
                queue_details.set(details);
            });
        }
    });

    // Convert to display types
    let current_track = use_memo(move || {
        current_db_track().map(|track| {
            let album = current_track_album();
            let cover_url = album.as_ref().and_then(|a| {
                a.cover_image_id
                    .as_ref()
                    .map(|id| image_url(id))
                    .or_else(|| a.cover_art_url.clone())
            });
            QueueItem {
                track: Track::from(&track),
                album_title: album
                    .map(|a| a.title)
                    .unwrap_or_else(|| "Unknown Album".to_string()),
                cover_url,
            }
        })
    });

    let queue_items = use_memo(move || {
        let ids = queue_track_ids.read();
        let details = queue_details.read();
        ids.iter()
            .filter_map(|id| {
                details.get(id).map(|(track, album)| {
                    let cover_url = album.as_ref().and_then(|a| {
                        a.cover_image_id
                            .as_ref()
                            .map(|id| image_url(id))
                            .or_else(|| a.cover_art_url.clone())
                    });
                    QueueItem {
                        track: Track::from(track),
                        album_title: album
                            .as_ref()
                            .map(|a| a.title.clone())
                            .unwrap_or_else(|| "Unknown Album".to_string()),
                        cover_url,
                    }
                })
            })
            .collect::<Vec<_>>()
    });

    let current_track_id = use_memo(move || current_db_track().map(|t| t.id));

    // Navigation callback
    let on_track_click = {
        let library_manager = library_manager.clone();
        move |track_id: String| {
            let library_manager = library_manager.clone();
            spawn(async move {
                if let Ok(album_id) = library_manager
                    .get()
                    .get_album_id_for_track(&track_id)
                    .await
                {
                    navigator().push(Route::AlbumDetail {
                        album_id,
                        release_id: String::new(),
                    });
                }
            });
        }
    };

    rsx! {
        QueueSidebarView {
            is_open: is_open(),
            current_track: current_track(),
            queue: queue_items(),
            current_track_id: current_track_id(),
            on_close: move |_| is_open.set(false),
            on_clear: move |_| (clear_fn)(),
            on_remove: move |idx: usize| playback.remove_from_queue(idx),
            on_track_click,
        }
    }
}
