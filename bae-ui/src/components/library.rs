//! Library view component - pure rendering, no data fetching

use crate::components::album_card::AlbumCard;
use crate::components::helpers::{ErrorDisplay, LoadingSpinner, PageContainer};
use crate::display_types::{Album, Artist};
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
    // Navigation callback - called with album_id when an album is clicked
    on_album_click: EventHandler<String>,
    // Action callbacks
    on_play_album: EventHandler<String>,
    on_add_album_to_queue: EventHandler<String>,
    // Empty state action (e.g., navigate to import)
    #[props(default)] on_empty_action: Option<EventHandler<()>>,
) -> Element {
    rsx! {
        PageContainer {
            h1 { class: "text-3xl font-bold text-white mb-6", "Music Library" }
            if loading {
                LoadingSpinner { message: "Loading your music library...".to_string() }
            } else if let Some(err) = error {
                ErrorDisplay { message: err }
                p { class: "text-sm mt-2 text-gray-400",
                    "Make sure you've imported some albums first!"
                }
            } else if albums.is_empty() {
                div { class: "text-center py-12",
                    div { class: "text-gray-400 text-6xl mb-4", "🎵" }
                    h2 { class: "text-2xl font-bold text-gray-300 mb-2",
                        "No albums in your library yet"
                    }
                    p { class: "text-gray-500 mb-4", "Import your first album to get started!" }
                    if let Some(handler) = on_empty_action {
                        button {
                            class: "inline-block bg-blue-600 hover:bg-blue-700 text-white font-bold py-2 px-4 rounded",
                            onclick: move |_| handler.call(()),
                            "Import Album"
                        }
                    }
                }
            } else {
                AlbumGrid {
                    albums,
                    artists_by_album,
                    on_album_click,
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
    on_album_click: EventHandler<String>,
    on_play_album: EventHandler<String>,
    on_add_album_to_queue: EventHandler<String>,
) -> Element {
    rsx! {
        div { class: "grid grid-cols-1 sm:grid-cols-2 md:grid-cols-3 lg:grid-cols-4 xl:grid-cols-5 gap-6",
            for album in albums {
                AlbumCard {
                    key: "{album.id}",
                    album: album.clone(),
                    artists: artists_by_album.get(&album.id).cloned().unwrap_or_default(),
                    on_click: on_album_click,
                    on_play: on_play_album,
                    on_add_to_queue: on_add_album_to_queue,
                }
            }
        }
    }
}
