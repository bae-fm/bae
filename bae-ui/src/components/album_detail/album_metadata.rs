//! Album metadata display component

use crate::display_types::{Album, Artist, Release};
use dioxus::prelude::*;

#[component]
pub fn AlbumMetadata(
    album: Album,
    artists: Vec<Artist>,
    track_count: usize,
    selected_release: Option<Release>,
) -> Element {
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

    rsx! {
        div {
            h1 { class: "text-2xl font-bold text-white mb-2", "{album.title}" }
            p { class: "text-lg text-gray-300 mb-2",
                "{artist_name}"
                if let Some(year) = album.year {
                    " · {year}"
                }
            }
        }
    }
}
