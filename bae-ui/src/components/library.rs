//! Library view component - pure rendering, no data fetching
//!
//! ## Reactive State Pattern
//! Accepts `ReadStore<LibraryState>` and uses lenses for granular reactivity.
//! Only subscribes to specific fields needed for routing decisions.

use crate::components::album_card::AlbumCard;
use crate::components::helpers::{ErrorDisplay, LoadingSpinner};
use crate::components::icons::ImageIcon;
use crate::components::Button;
use crate::display_types::{Album, Artist};
use crate::stores::library::{LibraryState, LibraryStateStoreExt};
use dioxus::prelude::*;
use dioxus_virtual_scroll::{KeyFn, RenderFn, ScrollTarget, VirtualGrid, VirtualGridConfig};
use std::collections::HashMap;
use std::rc::Rc;

/// Item type for the virtual album grid
#[derive(Clone, PartialEq)]
struct AlbumGridItem {
    album: Album,
    artists: Vec<Artist>,
}

/// Library view component - pure rendering, no data fetching
///
/// Accepts `ReadStore<LibraryState>` and uses lenses for granular reactivity.
#[component]
pub fn LibraryView(
    state: ReadStore<LibraryState>,
    // Navigation callback - called with album_id when an album is clicked
    on_album_click: EventHandler<String>,
    // Action callbacks
    on_play_album: EventHandler<String>,
    on_add_album_to_queue: EventHandler<String>,
    // Empty state action (e.g., navigate to import)
    on_empty_action: EventHandler<()>,
) -> Element {
    // Use lenses to subscribe only to specific fields for routing decisions
    let loading = *state.loading().read();
    let error = state.error().read().clone();
    let albums = state.albums().read().clone();
    let artists_by_album = state.artists_by_album().read().clone();

    let mut scroll_target: Signal<Option<Rc<MountedData>>> = use_signal(|| None);

    rsx! {
        div {
            class: "flex-grow overflow-y-auto flex flex-col py-10",
            onmounted: move |evt| scroll_target.set(Some(evt.data())),
            div { class: "container mx-auto flex flex-col",
                h1 { class: "text-3xl font-bold text-white mb-6", "Music Library" }
                if loading {
                    LoadingSpinner { message: "Loading your music library...".to_string() }
                } else if let Some(err) = error {
                    ErrorDisplay { message: err }
                    p { class: "text-sm mt-2 text-gray-400",
                        "An error occurred while loading your music library."
                    }
                } else if albums.is_empty() {
                    div { class: "text-center py-12",
                        div { class: "text-gray-400 mb-4",
                            ImageIcon { class: "w-16 h-16 mx-auto" }
                        }
                        h2 { class: "text-2xl font-bold text-gray-300 mb-2",
                            "No albums in your library yet"
                        }
                        p { class: "text-gray-500 mb-4", "Import your first album to get started!" }
                        Button { onclick: move |_| on_empty_action.call(()), "Import Album" }
                    }
                } else {
                    AlbumGrid {
                        albums,
                        artists_by_album,
                        on_album_click,
                        on_play_album,
                        on_add_album_to_queue,
                        scroll_target: ScrollTarget::Element(scroll_target.into()),
                    }
                }
            }
        }
    }
}

/// Grid component to display albums with virtual scrolling
#[component]
fn AlbumGrid(
    albums: Vec<Album>,
    artists_by_album: HashMap<String, Vec<Artist>>,
    on_album_click: EventHandler<String>,
    on_play_album: EventHandler<String>,
    on_add_album_to_queue: EventHandler<String>,
    scroll_target: ScrollTarget,
) -> Element {
    // Prepare items by joining albums with their artists
    let items: Vec<AlbumGridItem> = albums
        .into_iter()
        .map(|album| {
            let artists = artists_by_album.get(&album.id).cloned().unwrap_or_default();
            AlbumGridItem { album, artists }
        })
        .collect();

    let config = VirtualGridConfig {
        item_width: 200.0,
        item_height: 280.0,
        buffer_rows: 2,
        gap: 24.0,
    };

    // Create render function that captures the event handlers
    let render_item = RenderFn(Rc::new(move |item: AlbumGridItem, _idx: usize| {
        rsx! {
            AlbumCard {
                key: "{item.album.id}",
                album: item.album,
                artists: item.artists,
                on_click: on_album_click,
                on_play: on_play_album,
                on_add_to_queue: on_add_album_to_queue,
            }
        }
    }));

    // Key function extracts album ID for stable DOM keys
    let key_fn = KeyFn(Rc::new(|item: &AlbumGridItem| item.album.id.clone()));

    rsx! {
        VirtualGrid {
            items,
            config,
            render_item,
            key_fn,
            scroll_target,
        }
    }
}
