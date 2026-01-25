//! Album detail page

use crate::demo_data;
use crate::Route;
use bae_ui::stores::{AlbumDetailState, AlbumDetailStateStoreExt};
use bae_ui::{AlbumDetailView, BackButton, ErrorDisplay, PlaybackDisplay};
use dioxus::prelude::*;

#[component]
pub fn AlbumDetail(album_id: String) -> Element {
    let album = demo_data::get_album(&album_id);
    let artists = demo_data::get_artists_for_album(&album_id);
    let releases = demo_data::get_releases_for_album(&album_id);
    let tracks = demo_data::get_tracks_for_album(&album_id);
    let selected_release_id = releases.first().map(|r| r.id.clone());
    let has_album = album.is_some();

    // Derive count/ids/disc_info before moving tracks
    let track_count = tracks.len();
    let track_ids: Vec<String> = tracks.iter().map(|t| t.id.clone()).collect();
    let track_disc_info: Vec<(Option<i32>, String)> = tracks
        .iter()
        .map(|t| (t.disc_number, t.id.clone()))
        .collect();

    // Create state store for lens support
    let state = use_store(move || AlbumDetailState {
        album,
        artists,
        tracks,
        track_count,
        track_ids,
        track_disc_info,
        releases,
        files: vec![],
        images: vec![],
        selected_release_id,
        loading: false,
        error: None,
        import_progress: None,
        import_error: None,
    });

    // Get tracks lens for per-track reactivity
    let tracks = state.tracks();

    rsx! {
        BackButton {
            on_click: move |_| {
                navigator().push(Route::Library {});
            },
        }

        if has_album {
            AlbumDetailView {
                state,
                tracks,
                playback: PlaybackDisplay::Stopped,
                on_release_select: |_release_id: String| {},
                on_album_deleted: |_| {},
                on_export_release: |_| {},
                on_delete_album: |_| {},
                on_delete_release: |_| {},
                on_track_play: |_| {},
                on_track_pause: |_| {},
                on_track_resume: |_| {},
                on_track_add_next: |_| {},
                on_track_add_to_queue: |_| {},
                on_track_export: |_| {},
                on_play_album: |_| {},
                on_add_album_to_queue: |_| {},
            }
        } else {
            ErrorDisplay { message: "Album not found in demo data".to_string() }
        }
    }
}
