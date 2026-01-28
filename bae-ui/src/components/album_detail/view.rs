//! Album detail view - main component
//!
//! ## Reactive State Pattern
//! Uses stores and lenses for granular reactivity:
//! - `state` provides album, releases, artists, etc.
//! - `tracks` store enables per-track reactivity via `.iter()`
//! - Each TrackRow only re-renders when its specific track changes

use super::album_cover_section::AlbumCoverSection;
use super::album_metadata::AlbumMetadata;
use super::delete_album_dialog::DeleteAlbumDialog;
use super::delete_release_dialog::DeleteReleaseDialog;
use super::export_error_toast::ExportErrorToast;
use super::play_album_button::PlayAlbumButton;
use super::release_info_modal::{ReleaseInfoModal, Tab};
use super::release_tabs_section::{ReleaseTabsSection, ReleaseTorrentInfo};
use super::track_row::TrackRow;
use crate::display_types::{File, Image, PlaybackDisplay, Track};
use crate::stores::album_detail::{AlbumDetailState, AlbumDetailStateStoreExt};
use dioxus::prelude::*;
use std::collections::HashSet;

/// Album detail view component
///
/// Accepts stores for granular reactivity.
#[component]
pub fn AlbumDetailView(
    /// Album detail state store (enables lensing into fields)
    state: ReadStore<AlbumDetailState>,
    /// Tracks store - passed separately for per-track reactivity via .iter()
    tracks: ReadStore<Vec<Track>>,
    /// Playback state (from separate playback store)
    playback: PlaybackDisplay,
    on_release_select: EventHandler<String>,
    on_album_deleted: EventHandler<()>,
    on_export_release: EventHandler<String>,
    on_delete_album: EventHandler<String>,
    on_delete_release: EventHandler<String>,
    on_track_play: EventHandler<String>,
    on_track_pause: EventHandler<()>,
    on_track_resume: EventHandler<()>,
    on_track_add_next: EventHandler<String>,
    on_track_add_to_queue: EventHandler<String>,
    on_track_export: EventHandler<String>,
    on_play_album: EventHandler<Vec<String>>,
    on_add_album_to_queue: EventHandler<Vec<String>>,
    #[props(default)] modal_files: Vec<File>,
    #[props(default)] modal_images: Vec<Image>,
    #[props(default)] modal_loading_files: bool,
    #[props(default)] modal_loading_images: bool,
    #[props(default)] modal_files_error: Option<String>,
    #[props(default)] modal_images_error: Option<String>,
    #[props(default)] torrent_info: std::collections::HashMap<String, ReleaseTorrentInfo>,
    #[props(default)] on_start_seeding: Option<EventHandler<String>>,
    #[props(default)] on_stop_seeding: Option<EventHandler<String>>,
) -> Element {
    // UI-local state for dialogs
    let is_deleting = use_signal(|| false);
    let is_exporting = use_signal(|| false);
    let mut export_error = use_signal(|| None::<String>);
    let mut show_album_delete_confirm = use_signal(|| false);
    let mut show_release_delete_confirm = use_signal(|| None::<String>);
    let mut show_release_info_modal = use_signal(|| None::<(String, Tab)>);

    // Check if album exists - only subscribe to this field via lens
    if state.album().read().is_none() {
        return rsx! {};
    }

    rsx! {
        // Scrollable container
        div {
            class: "flex-grow min-h-0 overflow-y-auto",
            "data-testid": "album-detail",

            // Content wrapper with flex layout and width containment
            div { class: "container mx-auto flex flex-col lg:flex-row gap-8 p-6",
                // Left column - album info, cover, metadata, play button (sticky on desktop)
                div { class: "w-full lg:flex-shrink-0 lg:w-[360px] lg:self-start lg:sticky lg:top-6",
                    AlbumInfoSection {
                        state,
                        is_deleting,
                        is_exporting,
                        on_export: on_export_release,
                        on_delete_album: EventHandler::new(move |_: String| {
                            show_album_delete_confirm.set(true);
                        }),
                        on_view_release_info: EventHandler::new(move |id: String| {
                            show_release_info_modal.set(Some((id, Tab::Details)));
                        }),
                        on_open_gallery: EventHandler::new(move |id: String| {
                            show_release_info_modal.set(Some((id, Tab::Gallery)));
                        }),
                        on_play_album,
                        on_add_to_queue: on_add_album_to_queue,
                    }
                }

                // Right column - release tabs + tracklist
                div { class: "flex-1 min-w-0",
                    ReleaseTabsSectionWrapper {
                        state,
                        is_deleting,
                        is_exporting,
                        export_error,
                        torrent_info: torrent_info.clone(),
                        on_release_select,
                        on_view_files: move |id| show_release_info_modal.set(Some((id, Tab::Details))),
                        on_delete_release: move |id| show_release_delete_confirm.set(Some(id)),
                        on_export: on_export_release,
                        on_start_seeding,
                        on_stop_seeding,
                    }

                    TrackListSection {
                        state,
                        tracks,
                        playback,
                        on_track_play,
                        on_track_pause,
                        on_track_resume,
                        on_track_add_next,
                        on_track_add_to_queue,
                        on_track_export,
                    }
                }
            }
        }

        // Dialogs - these read state only when shown
        DeleteAlbumDialogWrapper {
            state,
            show: show_album_delete_confirm,
            is_deleting,
            on_delete_album,
            on_album_deleted,
        }

        DeleteReleaseDialogWrapper {
            state,
            show: show_release_delete_confirm,
            is_deleting,
            on_delete_release,
            on_album_deleted,
        }

        ReleaseInfoModalWrapper {
            state,
            show: show_release_info_modal,
            modal_files,
            modal_images,
            modal_loading_files,
            modal_loading_images,
            modal_files_error,
            modal_images_error,
        }

        if let Some(ref error) = export_error() {
            ExportErrorToast {
                error: error.clone(),
                on_dismiss: move |_| export_error.set(None),
            }
        }
    }
}

// ============================================================================
// Sub-sections that each read only their portion of state
// ============================================================================

/// Album info section - uses lenses to read individual fields
#[component]
fn AlbumInfoSection(
    state: ReadStore<AlbumDetailState>,
    is_deleting: Signal<bool>,
    is_exporting: Signal<bool>,
    on_export: EventHandler<String>,
    on_delete_album: EventHandler<String>,
    on_view_release_info: EventHandler<String>,
    on_open_gallery: EventHandler<String>,
    on_play_album: EventHandler<Vec<String>>,
    on_add_to_queue: EventHandler<Vec<String>>,
) -> Element {
    // Use lenses to read individual fields - avoids subscribing to track changes
    let album = state.album().read().clone();
    let Some(album) = album else {
        return rsx! {};
    };
    let releases = state.releases().read().clone();
    let artists = state.artists().read().clone();
    let import_progress = *state.import_progress().read();
    let import_error = state.import_error().read().clone();
    let selected_release_id = state.selected_release_id().read().clone();

    // Use derived fields - these don't change during import progress updates
    let track_count = *state.track_count().read();
    let track_ids = state.track_ids().read().clone();

    rsx! {
        AlbumCoverSection {
            album: album.clone(),
            import_progress,
            is_deleting: *is_deleting.read(),
            is_exporting: *is_exporting.read(),
            first_release_id: releases.first().map(|r| r.id.clone()),
            has_single_release: releases.len() == 1,
            on_export,
            on_delete_album,
            on_view_release_info,
            on_open_gallery,
        }
        AlbumMetadata {
            album: album.clone(),
            artists,
            track_count,
            selected_release: releases.iter().find(|r| Some(r.id.clone()) == selected_release_id).cloned(),
        }
        PlayAlbumButton {
            track_ids,
            import_progress,
            import_error,
            is_deleting: *is_deleting.read(),
            on_play_album,
            on_add_to_queue,
        }
    }
}

/// Release tabs section wrapper - uses lenses
#[component]
fn ReleaseTabsSectionWrapper(
    state: ReadStore<AlbumDetailState>,
    is_deleting: Signal<bool>,
    is_exporting: Signal<bool>,
    export_error: Signal<Option<String>>,
    torrent_info: std::collections::HashMap<String, ReleaseTorrentInfo>,
    on_release_select: EventHandler<String>,
    on_view_files: EventHandler<String>,
    on_delete_release: EventHandler<String>,
    on_export: EventHandler<String>,
    on_start_seeding: Option<EventHandler<String>>,
    on_stop_seeding: Option<EventHandler<String>>,
) -> Element {
    // Use lenses
    let releases = state.releases().read().clone();
    let selected_release_id = state.selected_release_id().read().clone();

    if releases.len() <= 1 {
        return rsx! {};
    }

    rsx! {
        ReleaseTabsSection {
            releases,
            selected_release_id,
            on_release_select,
            is_deleting,
            is_exporting,
            export_error,
            on_view_files,
            on_delete_release,
            on_export,
            torrent_info,
            on_start_seeding,
            on_stop_seeding,
        }
    }
}

/// Track list section - iterates over tracks store for per-track reactivity
#[component]
fn TrackListSection(
    state: ReadStore<AlbumDetailState>,
    tracks: ReadStore<Vec<Track>>,
    playback: PlaybackDisplay,
    on_track_play: EventHandler<String>,
    on_track_pause: EventHandler<()>,
    on_track_resume: EventHandler<()>,
    on_track_add_next: EventHandler<String>,
    on_track_add_to_queue: EventHandler<String>,
    on_track_export: EventHandler<String>,
) -> Element {
    // Use lenses for individual fields - avoids subscribing to track import_state changes
    let artists = state.artists().read().clone();
    let is_compilation = state
        .album()
        .read()
        .as_ref()
        .map(|a| a.is_compilation)
        .unwrap_or(false);
    let release_id = state
        .selected_release_id()
        .read()
        .clone()
        .unwrap_or_default();

    // Extract current track ID from playback state
    let current_track_id = match &playback {
        PlaybackDisplay::Playing { track_id, .. } => Some(track_id.clone()),
        PlaybackDisplay::Paused { track_id, .. } => Some(track_id.clone()),
        PlaybackDisplay::Loading { track_id } => Some(track_id.clone()),
        PlaybackDisplay::Stopped => None,
    };

    // Use derived fields to avoid subscribing to track changes
    let track_count = *state.track_count().read();
    if track_count == 0 {
        return rsx! {
            div { class: "text-center py-8 text-gray-400",
                p { "No tracks found for this album." }
            }
        };
    }

    // Get disc info from derived field
    let disc_info = state.track_disc_info().read().clone();

    // Check for multiple discs
    let has_multiple_discs = disc_info
        .iter()
        .filter_map(|(d, _)| *d)
        .collect::<HashSet<_>>()
        .len()
        > 1;

    // Track which disc we're on for headers
    let mut current_disc: Option<i32> = None;

    rsx! {
        div { class: "space-y-1",
            // Zip disc_info with track stores for per-track reactivity
            for ((disc_number , track_id) , track_store) in disc_info.into_iter().zip(tracks.iter()) {
                {
                    // Check if we need a disc header
                    let show_disc_header = has_multiple_discs && disc_number != current_disc;
                    if show_disc_header {
                        current_disc = disc_number;
                    }
                    let disc_label = disc_number
                        .map(|d| format!("Disc {}", d))
                        .unwrap_or_else(|| "Disc 1".to_string());

                    // Playback state for this track
                    let is_this_track = current_track_id.as_ref() == Some(&track_id);
                    let is_playing = is_this_track

                        // Wrapper div with key for VDOM diffing
                        && matches!(playback, PlaybackDisplay::Playing { .. });
                    let is_paused = is_this_track
                        && matches!(playback, PlaybackDisplay::Paused { .. });
                    let is_loading = is_this_track
                        && matches!(playback, PlaybackDisplay::Loading { .. });
                    rsx! {
                        div { key: "track-{track_id}",
                            if show_disc_header {
                                h3 { class: "text-sm font-semibold text-gray-400 uppercase tracking-wide pt-4 pb-2 first:pt-0",
                                    "{disc_label}"
                                }
                            }
                            TrackRow {
                                track: track_store,
                                artists: artists.clone(),
                                release_id: release_id.clone(),
                                is_compilation,
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

// ============================================================================
// Dialog wrappers - only read state when shown
// ============================================================================

#[component]
fn DeleteAlbumDialogWrapper(
    state: ReadStore<AlbumDetailState>,
    show: Signal<bool>,
    is_deleting: Signal<bool>,
    on_delete_album: EventHandler<String>,
    on_album_deleted: EventHandler<()>,
) -> Element {
    // Use lenses
    let album_id = state
        .album()
        .read()
        .as_ref()
        .map(|a| a.id.clone())
        .unwrap_or_default();
    let release_count = state.releases().read().len();

    // Create read signal from show
    let is_open: ReadSignal<bool> = show.into();

    rsx! {
        DeleteAlbumDialog {
            is_open,
            album_id: album_id.clone(),
            release_count,
            is_deleting,
            on_confirm: move |album_id: String| {
                show.set(false);
                on_delete_album.call(album_id);
                on_album_deleted.call(());
            },
            on_cancel: move |_| show.set(false),
        }
    }
}

#[component]
fn DeleteReleaseDialogWrapper(
    state: ReadStore<AlbumDetailState>,
    show: Signal<Option<String>>,
    is_deleting: Signal<bool>,
    on_delete_release: EventHandler<String>,
    on_album_deleted: EventHandler<()>,
) -> Element {
    // Derive is_open from Option<String>
    let is_open_memo = use_memo(move || show().is_some());
    let is_open: ReadSignal<bool> = is_open_memo.into();

    let release_id_to_delete = show().unwrap_or_default();

    // Use lens
    let releases = state.releases().read().clone();
    let is_last = releases.len() == 1;

    rsx! {
        DeleteReleaseDialog {
            is_open,
            release_id: release_id_to_delete.clone(),
            is_last_release: is_last,
            is_deleting,
            on_confirm: move |release_id: String| {
                show.set(None);
                on_delete_release.call(release_id);
                if is_last {
                    on_album_deleted.call(());
                }
            },
            on_cancel: move |_| show.set(None),
        }
    }
}

#[component]
fn ReleaseInfoModalWrapper(
    state: ReadStore<AlbumDetailState>,
    show: Signal<Option<(String, Tab)>>,
    modal_files: Vec<File>,
    modal_images: Vec<Image>,
    modal_loading_files: bool,
    modal_loading_images: bool,
    modal_files_error: Option<String>,
    modal_images_error: Option<String>,
) -> Element {
    // Get release if available
    let Some((release_id, initial_tab)) = show() else {
        return rsx! {};
    };
    let release = state
        .releases()
        .read()
        .iter()
        .find(|r| r.id == release_id)
        .cloned();

    // Only render modal if we have a release
    let Some(release) = release else {
        return rsx! {};
    };

    // Get track stats
    let track_count = *state.track_count().read();
    let total_duration_ms: Option<i64> = {
        let sum: i64 = state
            .tracks()
            .read()
            .iter()
            .filter_map(|t| t.duration_ms)
            .sum();
        if sum > 0 {
            Some(sum)
        } else {
            None
        }
    };

    // Derive is_open - always true when we have a release since show().is_some()
    let is_open_memo = use_memo(move || show().is_some());
    let is_open: ReadSignal<bool> = is_open_memo.into();

    rsx! {
        ReleaseInfoModal {
            is_open,
            release,
            on_close: move |_| show.set(None),
            files: modal_files,
            images: modal_images,
            is_loading_files: modal_loading_files,
            is_loading_images: modal_loading_images,
            files_error: modal_files_error,
            images_error: modal_images_error,
            initial_tab,
            track_count,
            total_duration_ms,
        }
    }
}
