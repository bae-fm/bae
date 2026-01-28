//! Album card component - pure view with callbacks

use crate::components::icons::{EllipsisIcon, ImageIcon, PlayIcon, PlusIcon};
use crate::components::{Dropdown, Placement};
use crate::display_types::{Album, Artist};
use dioxus::prelude::*;

/// Individual album card component
///
/// Pure view component - displays album info with hover dropdown for actions.
/// Navigation is handled via on_click callback, not direct router calls.
#[component]
pub fn AlbumCard(
    album: Album,
    artists: Vec<Artist>,
    // Navigation callback - called with album_id when card is clicked
    on_click: EventHandler<String>,
    // Action callbacks
    on_play: EventHandler<String>,
    on_add_to_queue: EventHandler<String>,
) -> Element {
    let album_id = album.id.clone();
    let album_title = album.title.clone();
    let album_year = album.year;
    let cover_url = album.cover_url.clone();

    let mut show_dropdown = use_signal(|| false);
    let is_open: ReadSignal<bool> = show_dropdown.into();
    // Use album_id for anchor to ensure uniqueness even if component is recycled
    let anchor_id = format!("album-card-btn-{}", album_id);

    let artist_name = if artists.is_empty() {
        "Unknown Artist".to_string()
    } else if artists.len() == 1 {
        artists[0].name.clone()
    } else {
        artists
            .iter()
            .map(|a| a.name.as_str())
            .collect::<Vec<_>>()
            .join(", ")
    };

    // Note: use overflow-clip (not overflow-hidden) to clip rounded corners without blocking scroll propagation
    let card_class = "bg-gray-800 rounded-lg overflow-clip shadow-lg hover:shadow-xl transition-shadow duration-300 cursor-pointer group relative";

    rsx! {
        div {
            class: "{card_class}",
            "data-testid": "album-card",
            onclick: {
                let album_id = album_id.clone();
                move |_| {
                    if !show_dropdown() {
                        on_click.call(album_id.clone());
                    }
                }
            },
            div { class: "aspect-square bg-gray-700 flex items-center justify-center relative",
                if let Some(url) = &cover_url {
                    img {
                        src: "{url}",
                        alt: "Album cover for {album_title}",
                        class: "w-full h-full object-cover",
                    }
                } else {
                    ImageIcon { class: "w-12 h-12 text-gray-500" }
                }

                // Hover overlay with dropdown trigger - stays visible when dropdown is open
                div {
                    class: "absolute inset-0 transition-colors flex items-start justify-end p-2",
                    class: if show_dropdown() { "bg-black/40" } else { "bg-black/0 group-hover:bg-black/40" },
                    button {
                        id: "{anchor_id}",
                        class: "transition-opacity bg-gray-900/80 hover:bg-gray-800 rounded-full w-8 h-8 flex items-center justify-center text-white",
                        class: if show_dropdown() { "opacity-100" } else { "opacity-0 group-hover:opacity-100" },
                        onclick: move |evt| {
                            evt.stop_propagation();
                            show_dropdown.set(!show_dropdown());
                        },
                        EllipsisIcon { class: "w-5 h-5" }
                    }
                }
            }
            div { class: "p-4",
                h3 {
                    class: "font-bold text-white text-lg mb-1 truncate",
                    title: "{album_title}",
                    "{album_title}"
                }
                p {
                    class: "text-gray-400 text-sm truncate",
                    title: "{artist_name}",
                    "{artist_name}"
                }
                if let Some(year) = album_year {
                    p { class: "text-gray-500 text-xs mt-1", "{year}" }
                }
            }

            // Dropdown menu
            Dropdown {
                anchor_id: anchor_id.clone(),
                is_open,
                on_close: move |_| show_dropdown.set(false),
                placement: Placement::BottomEnd,
                class: "bg-gray-800 border border-gray-700 rounded-lg shadow-xl min-w-[140px] overflow-clip",
                button {
                    class: "w-full px-4 py-2 text-left text-white hover:bg-gray-700 transition-colors flex items-center gap-2",
                    onclick: {
                        let album_id = album_id.clone();
                        move |evt| {
                            evt.stop_propagation();
                            show_dropdown.set(false);
                            on_play.call(album_id.clone());
                        }
                    },
                    PlayIcon { class: "w-4 h-4" }
                    span { "Play" }
                }
                button {
                    class: "w-full px-4 py-2 text-left text-white hover:bg-gray-700 transition-colors flex items-center gap-2",
                    onclick: {
                        let album_id = album_id.clone();
                        move |evt| {
                            evt.stop_propagation();
                            show_dropdown.set(false);
                            on_add_to_queue.call(album_id.clone());
                        }
                    },
                    PlusIcon { class: "w-4 h-4" }
                    span { "Add to Queue" }
                }
            }
        }
    }
}
