//! Queue Sidebar component
//!
//! Wrapper component that connects bae-ui's QueueSidebarView to app state.

use super::playback_hooks::{use_playback_queue, use_playback_service, use_playback_state};
use crate::ui::display_types::{track_from_db_ref, QueueItem};
use crate::ui::use_library_manager;
use crate::ui::{image_url, Route};
use bae_core::db::{DbAlbum, DbTrack};
use bae_core::playback::PlaybackState;
use bae_ui::QueueSidebarView;
use dioxus::prelude::*;

// Re-export QueueSidebarState so other modules can use it
pub use bae_ui::QueueSidebarState;

/// Queue Sidebar wrapper that handles state subscription
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
    let current_track_album = use_signal(|| Option::<DbAlbum>::None);
    use_effect({
        let library_manager = library_manager.clone();
        move || {
            let mut current_track_album = current_track_album;
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
    let queue_details =
        use_signal(std::collections::HashMap::<String, (DbTrack, Option<DbAlbum>)>::new);
    use_effect({
        let library_manager = library_manager.clone();
        move || {
            let library_manager = library_manager.clone();
            let queue_val = queue_track_ids.read().clone();
            let mut queue_details = queue_details;
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
                track: track_from_db_ref(&track),
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
                        track: track_from_db_ref(track),
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
