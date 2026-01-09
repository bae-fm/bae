use super::album_cover_section::AlbumCoverSection;
use super::album_metadata::AlbumMetadata;
use super::delete_album_dialog::DeleteAlbumDialog;
use super::delete_release_dialog::DeleteReleaseDialog;
use super::export_error_toast::ExportErrorToast;
use super::play_album_button::PlayAlbumButton;
use super::release_tabs_section::ReleaseTabsSection;
use super::track_row::TrackRow;
use super::ReleaseInfoModal;
use crate::ui::display_types::{Album, Artist, File, Image, PlaybackDisplay, Release, Track};
use dioxus::prelude::*;

/// Album detail view component (pure, props-based)
/// All callbacks are required - pass noops if actions are not needed.
#[component]
pub fn AlbumDetailView(
    // Album data (display types)
    album: Album,
    releases: Vec<Release>,
    artists: Vec<Artist>,
    tracks: Vec<Track>,
    selected_release_id: Option<String>,
    // UI state
    import_progress: ReadSignal<Option<u8>>,
    import_error: ReadSignal<Option<String>>,
    // Playback state for highlighting current track
    #[props(default)] playback: PlaybackDisplay,
    // Navigation callback
    on_release_select: EventHandler<String>,
    // Album-level callbacks
    on_album_deleted: EventHandler<()>,
    on_export_release: EventHandler<String>,
    on_delete_album: EventHandler<String>,
    on_delete_release: EventHandler<String>,
    // Track playback callbacks
    on_track_play: EventHandler<String>,
    on_track_pause: EventHandler<()>,
    on_track_resume: EventHandler<()>,
    on_track_add_next: EventHandler<String>,
    on_track_add_to_queue: EventHandler<String>,
    on_track_export: EventHandler<String>,
    // Album playback callbacks
    on_play_album: EventHandler<Vec<String>>,
    on_add_album_to_queue: EventHandler<Vec<String>>,
    // Release info modal data (loaded by page, passed here)
    #[props(default)] modal_files: Vec<File>,
    #[props(default)] modal_images: Vec<Image>,
    #[props(default)] modal_loading_files: bool,
    #[props(default)] modal_loading_images: bool,
    #[props(default)] modal_files_error: Option<String>,
    #[props(default)] modal_images_error: Option<String>,
) -> Element {
    let is_deleting = use_signal(|| false);
    let is_exporting = use_signal(|| false);
    let mut export_error = use_signal(|| None::<String>);
    let mut show_album_delete_confirm = use_signal(|| false);
    let mut show_release_delete_confirm = use_signal(|| None::<String>);
    let mut show_release_info_modal = use_signal(|| None::<String>);

    // Extract current track ID from playback state
    let current_track_id = match &playback {
        PlaybackDisplay::Playing { track_id, .. } => Some(track_id.clone()),
        PlaybackDisplay::Paused { track_id, .. } => Some(track_id.clone()),
        PlaybackDisplay::Loading { track_id } => Some(track_id.clone()),
        PlaybackDisplay::Stopped => None,
    };

    // Track IDs for play album button
    let track_ids: Vec<String> = tracks.iter().map(|t| t.id.clone()).collect();

    rsx! {
        div { class: "grid grid-cols-1 lg:grid-cols-3 gap-8",
            div { class: "lg:col-span-1",
                div { class: "bg-gray-800 rounded-lg p-6",
                    AlbumCoverSection {
                        album: album.clone(),
                        import_progress,
                        is_deleting,
                        is_exporting,
                        first_release_id: releases.first().map(|r| r.id.clone()),
                        has_single_release: releases.len() == 1,
                        on_export: on_export_release,
                        on_delete_album: EventHandler::new(move |_: String| {
                            show_album_delete_confirm.set(true);
                        }),
                        on_view_release_info: EventHandler::new(move |id: String| {
                            show_release_info_modal.set(Some(id));
                        }),
                    }
                    AlbumMetadata {
                        album: album.clone(),
                        artists: artists.clone(),
                        track_count: tracks.len(),
                        selected_release: releases.iter().find(|r| Some(r.id.clone()) == selected_release_id).cloned(),
                    }
                    PlayAlbumButton {
                        track_ids: track_ids.clone(),
                        import_progress,
                        import_error,
                        is_deleting,
                        on_play_album,
                        on_add_to_queue: on_add_album_to_queue,
                    }
                }
            }
            div { class: "lg:col-span-2",
                div { class: "bg-gray-800 rounded-lg p-6",
                    if releases.len() > 1 {
                        ReleaseTabsSection {
                            releases: releases.clone(),
                            selected_release_id: selected_release_id.clone(),
                            on_release_select,
                            is_deleting,
                            is_exporting,
                            export_error,
                            on_view_files: move |id| show_release_info_modal.set(Some(id)),
                            on_delete_release: move |id| show_release_delete_confirm.set(Some(id)),
                        }
                    }
                    h2 { class: "text-xl font-bold text-white mb-4", "Tracklist" }
                    if tracks.is_empty() {
                        div { class: "text-center py-8 text-gray-400",
                            p { "No tracks found for this album." }
                        }
                    } else {
                        {
                            let has_multiple_discs = tracks
                                .iter()
                                .filter_map(|t| t.disc_number)
                                .collect::<std::collections::HashSet<_>>()
                                .len() > 1;
                            let release_id = selected_release_id.clone().unwrap_or_default();
                            if has_multiple_discs {
                                let mut current_disc: Option<i32> = None;
                                rsx! {
                                    div { class: "space-y-2",
                                        for track in &tracks {
                                            if track.disc_number != current_disc {
                                                {
                                                    current_disc = track.disc_number;
                                                    let disc_label = track
                                                        .disc_number
                                                        .map(|d| format!("Disc {}", d))
                                                        .unwrap_or_else(|| "Disc 1".to_string());
                                                    rsx! {
                                                        h3 { class: "text-sm font-semibold text-gray-400 uppercase tracking-wide pt-4 pb-2 first:pt-0",
                                                            "{disc_label}"
                                                        }
                                                    }
                                                }
                                            }
                                            {
                                                let is_this_track = current_track_id.as_ref() == Some(&track.id);
                                                let is_playing = is_this_track
                                                    && matches!(playback, PlaybackDisplay::Playing { .. });
                                                let is_paused = is_this_track
                                                    && matches!(playback, PlaybackDisplay::Paused { .. });
                                                let is_loading = is_this_track
                                                    && matches!(playback, PlaybackDisplay::Loading { .. });
                                                rsx! {
                                                    TrackRow {
                                                        track: track.clone(),
                                                        artists: artists.clone(),
                                                        release_id: release_id.clone(),
                                                        is_compilation: album.is_compilation,
                                                        is_playing,
                                                        is_paused,
                                                        is_loading,
                                                        show_spinner: is_loading,
                                                        on_play: on_track_play,
                                                        on_pause: on_track_pause,
                                                        on_resume: on_track_resume,
                                                        on_add_next: on_track_add_next,
                                                        on_add_to_queue: on_track_add_to_queue,
                                                        on_export: on_track_export,
                                                    }
                                                }
                                            }
                                        }
                                    }
                                }
                            } else {
                                rsx! {
                                    div { class: "space-y-2",
                                        for track in &tracks {
                                            {
                                                let is_this_track = current_track_id.as_ref() == Some(&track.id);
                                                let is_playing = is_this_track
                                                    && matches!(playback, PlaybackDisplay::Playing { .. });
                                                let is_paused = is_this_track
                                                    && matches!(playback, PlaybackDisplay::Paused { .. });
                                                let is_loading = is_this_track
                                                    && matches!(playback, PlaybackDisplay::Loading { .. });
                                                rsx! {
                                                    TrackRow {
                                                        track: track.clone(),
                                                        artists: artists.clone(),
                                                        release_id: release_id.clone(),
                                                        is_compilation: album.is_compilation,
                                                        is_playing,
                                                        is_paused,
                                                        is_loading,
                                                        show_spinner: is_loading,
                                                        on_play: on_track_play,
                                                        on_pause: on_track_pause,
                                                        on_resume: on_track_resume,
                                                        on_add_next: on_track_add_next,
                                                        on_add_to_queue: on_track_add_to_queue,
                                                        on_export: on_track_export,
                                                    }
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
        // Delete album dialog
        if show_album_delete_confirm() {
            DeleteAlbumDialog {
                album_id: album.id.clone(),
                release_count: releases.len(),
                is_deleting,
                on_confirm: move |album_id: String| {
                    show_album_delete_confirm.set(false);
                    on_delete_album.call(album_id);
                    on_album_deleted.call(());
                },
                on_cancel: move |_| show_album_delete_confirm.set(false),
            }
        }
        // Delete release dialog
        if let Some(release_id_to_delete) = show_release_delete_confirm() {
            if releases.iter().any(|r| r.id == release_id_to_delete) {
                DeleteReleaseDialog {
                    release_id: release_id_to_delete.clone(),
                    is_last_release: releases.len() == 1,
                    is_deleting,
                    on_confirm: {
                        let is_last = releases.len() == 1;
                        move |release_id: String| {
                            show_release_delete_confirm.set(None);
                            on_delete_release.call(release_id);
                            if is_last {
                                on_album_deleted.call(());
                            }
                        }
                    },
                    on_cancel: move |_| show_release_delete_confirm.set(None),
                }
            }
        }
        if let Some(release_id) = show_release_info_modal() {
            if let Some(release) = releases.iter().find(|r| r.id == release_id) {
                ReleaseInfoModal {
                    release: release.clone(),
                    on_close: move |_| show_release_info_modal.set(None),
                    files: modal_files.clone(),
                    images: modal_images.clone(),
                    is_loading_files: modal_loading_files,
                    is_loading_images: modal_loading_images,
                    files_error: modal_files_error.clone(),
                    images_error: modal_images_error.clone(),
                }
            }
        }
        if let Some(ref error) = export_error() {
            ExportErrorToast {
                error: error.clone(),
                on_dismiss: move |_| export_error.set(None),
            }
        }
    }
}
