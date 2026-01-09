use crate::ui::components::album_card::AlbumCard;
use crate::ui::display_types::{Album, Artist};
use crate::ui::Route;
use dioxus::prelude::*;
use std::collections::HashMap;

/// Library view component - pure rendering, no data fetching
/// All callbacks are required - pass noops if not needed.
#[component]
pub fn LibraryView(
    albums: Vec<Album>,
    artists_by_album: HashMap<String, Vec<Artist>>,
    loading: bool,
    error: Option<String>,
    // Callbacks - all required
    on_play_album: EventHandler<String>,
    on_add_album_to_queue: EventHandler<String>,
) -> Element {
    rsx! {
        div { class: "container mx-auto p-6",
            h1 { class: "text-3xl font-bold text-white mb-6", "Music Library" }
            if loading {
                div { class: "flex justify-center items-center py-12",
                    div { class: "animate-spin rounded-full h-12 w-12 border-b-2 border-blue-500" }
                    p { class: "ml-4 text-gray-300", "Loading your music library..." }
                }
            } else if let Some(err) = error {
                div { class: "bg-red-900 border border-red-700 text-red-100 px-4 py-3 rounded mb-4",
                    p { "{err}" }
                    p { class: "text-sm mt-2", "Make sure you've imported some albums first!" }
                }
            } else if albums.is_empty() {
                div { class: "text-center py-12",
                    div { class: "text-gray-400 text-6xl mb-4", "🎵" }
                    h2 { class: "text-2xl font-bold text-gray-300 mb-2",
                        "No albums in your library yet"
                    }
                    p { class: "text-gray-500 mb-4", "Import your first album to get started!" }
                    Link {
                        to: Route::ImportWorkflowManager {},
                        class: "inline-block bg-blue-600 hover:bg-blue-700 text-white font-bold py-2 px-4 rounded",
                        "Import Album"
                    }
                }
            } else {
                AlbumGrid {
                    albums,
                    artists_by_album,
                    on_play_album,
                    on_add_album_to_queue,
                }
            }
        }
    }
}

/// Grid component to display albums
#[component]
fn AlbumGrid(
    albums: Vec<Album>,
    artists_by_album: HashMap<String, Vec<Artist>>,
    on_play_album: EventHandler<String>,
    on_add_album_to_queue: EventHandler<String>,
) -> Element {
    rsx! {
        div { class: "grid grid-cols-1 sm:grid-cols-2 md:grid-cols-3 lg:grid-cols-4 xl:grid-cols-5 gap-6",
            for album in albums {
                AlbumCard {
                    album: album.clone(),
                    artists: artists_by_album.get(&album.id).cloned().unwrap_or_default(),
                    on_play: on_play_album,
                    on_add_to_queue: on_add_album_to_queue,
                }
            }
        }
    }
}

/// Library page component - loads data and passes to LibraryView
#[cfg(not(feature = "demo"))]
#[component]
pub fn LibraryPage() -> Element {
    use crate::library::use_library_manager;
    use crate::ui::components::album_detail::utils::get_album_track_ids;
    use crate::ui::components::use_playback_service;
    use tracing::debug;

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
                                db_artists.iter().map(Artist::from).collect();
                            artists_map.insert(album.id.clone(), artists);
                        }
                    }
                    let display_albums: Vec<Album> = album_list.iter().map(Album::from).collect();
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

    // Callbacks
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

    rsx! {
        LibraryView {
            albums: albums(),
            artists_by_album: artists_by_album(),
            loading: loading(),
            error: error(),
            on_play_album,
            on_add_album_to_queue,
        }
    }
}

/// Demo library page - uses static fixture data
#[cfg(feature = "demo")]
#[component]
pub fn LibraryPage() -> Element {
    use crate::ui::demo_data;

    let albums = demo_data::get_albums();
    let artists_by_album = demo_data::get_artists_by_album();

    rsx! {
        LibraryView {
            albums,
            artists_by_album,
            loading: false,
            error: None,
            on_play_album: |_| {},
            on_add_album_to_queue: |_| {},
        }
    }
}

// Keep the old name as an alias for backwards compatibility with routes
pub use LibraryPage as Library;
