//! Library page component
//!
//! Uses bae-ui's LibraryView with app-specific navigation callbacks.

use crate::ui::components::album_detail::utils::get_album_track_ids;
use crate::ui::components::use_playback_service;
use crate::ui::display_types::{album_from_db_ref, artist_from_db_ref, Album, Artist};
use crate::ui::use_library_manager;
use crate::ui::Route;
use bae_ui::LibraryView;
use dioxus::prelude::*;
use std::collections::HashMap;
use tracing::debug;

/// Library page component - loads data and passes to bae-ui's LibraryView
#[component]
pub fn LibraryPage() -> Element {
    let mut albums = use_signal(Vec::<Album>::new);
    let mut artists_by_album = use_signal(HashMap::<String, Vec<Artist>>::new);
    let mut loading = use_signal(|| true);
    let mut error = use_signal(|| None::<String>);

    let library_manager = use_library_manager();
    let playback = use_playback_service();

    let library_manager_for_effect = library_manager.clone();
    use_effect(move || {
        debug!("Starting load_albums effect");
        let library_manager = library_manager_for_effect.clone();
        spawn(async move {
            debug!("Inside async spawn, fetching albums");
            loading.set(true);
            error.set(None);
            match library_manager.get().get_albums().await {
                Ok(album_list) => {
                    let mut artists_map = HashMap::new();
                    for album in &album_list {
                        if let Ok(db_artists) =
                            library_manager.get().get_artists_for_album(&album.id).await
                        {
                            let artists: Vec<Artist> =
                                db_artists.iter().map(artist_from_db_ref).collect();
                            artists_map.insert(album.id.clone(), artists);
                        }
                    }
                    let display_albums: Vec<Album> =
                        album_list.iter().map(album_from_db_ref).collect();
                    artists_by_album.set(artists_map);
                    albums.set(display_albums);
                    loading.set(false);
                }
                Err(e) => {
                    error.set(Some(format!("Failed to load library: {}", e)));
                    loading.set(false);
                }
            }
        });
    });

    // Navigation callback - navigate to album detail
    let on_album_click = move |album_id: String| {
        navigator().push(Route::AlbumDetail {
            album_id,
            release_id: String::new(),
        });
    };

    // Play album callback
    let on_play_album = {
        let library_manager = library_manager.clone();
        let playback = playback.clone();
        move |album_id: String| {
            let library_manager = library_manager.clone();
            let playback = playback.clone();
            spawn(async move {
                if let Ok(track_ids) = get_album_track_ids(&library_manager, &album_id).await {
                    playback.play_album(track_ids);
                }
            });
        }
    };

    // Add to queue callback
    let on_add_album_to_queue = {
        let library_manager = library_manager.clone();
        let playback = playback.clone();
        move |album_id: String| {
            let library_manager = library_manager.clone();
            let playback = playback.clone();
            spawn(async move {
                if let Ok(track_ids) = get_album_track_ids(&library_manager, &album_id).await {
                    playback.add_to_queue(track_ids);
                }
            });
        }
    };

    // Empty state action - navigate to import workflow
    let on_empty_action = move |_| {
        navigator().push(Route::ImportWorkflowManager {});
    };

    rsx! {
        LibraryView {
            albums: albums(),
            artists_by_album: artists_by_album(),
            loading: loading(),
            error: error(),
            on_album_click,
            on_play_album,
            on_add_album_to_queue,
            on_empty_action,
        }
    }
}

// Keep the old name as an alias for backwards compatibility with routes
pub use LibraryPage as Library;
