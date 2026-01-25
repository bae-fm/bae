//! Library page

use crate::demo_data;
use crate::Route;
use bae_ui::stores::LibraryState;
use bae_ui::{Album, Artist, LibraryView};
use dioxus::prelude::*;
use std::collections::HashMap;

// TODO: Remove this - temporary for testing large libraries
const ALBUM_COUNT: usize = 2000;

#[component]
pub fn Library() -> Element {
    let (albums, artists_by_album) = generate_albums(ALBUM_COUNT);

    let state = use_store(|| LibraryState {
        albums,
        artists_by_album,
        loading: false,
        error: None,
    });

    rsx! {
        LibraryView {
            state,
            on_album_click: move |album_id: String| {
                navigator().push(Route::AlbumDetail { album_id });
            },
            on_play_album: |_| {},
            on_add_album_to_queue: |_| {},
            on_empty_action: |_| {},
        }
    }
}

fn generate_albums(count: usize) -> (Vec<Album>, HashMap<String, Vec<Artist>>) {
    let base_albums = demo_data::get_albums();
    let base_artists = demo_data::get_artists_by_album();

    let mut albums = Vec::with_capacity(count);
    let mut artists_by_album = HashMap::new();

    for i in 0..count {
        let idx = i % base_albums.len();
        let base = &base_albums[idx];
        let id = format!("album-{}", i + 1);

        albums.push(Album {
            id: id.clone(),
            title: base.title.clone(),
            year: base.year,
            cover_url: base.cover_url.clone(),
            is_compilation: base.is_compilation,
        });

        if let Some(artists) = base_artists.get(&base.id) {
            artists_by_album.insert(id, artists.clone());
        }
    }

    (albums, artists_by_album)
}
