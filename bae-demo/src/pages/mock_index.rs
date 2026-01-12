//! Mock pages with URL state persistence

use crate::mocks::url_state::{get_state_bool, get_state_enum, parse_state, StateBuilder};
use crate::mocks::{AlbumDetailMock, FolderImportMock};
use crate::Route;
use bae_ui::ImportPhase;
use dioxus::prelude::*;

#[component]
pub fn MockIndex() -> Element {
    rsx! {
        div { class: "min-h-screen bg-gray-900 text-white p-8",
            h1 { class: "text-2xl font-bold mb-6", "Component Mocks" }
            div { class: "space-y-2",
                Link {
                    to: Route::MockFolderImport {
                        state: None,
                    },
                    class: "block p-4 bg-gray-800 rounded-lg hover:bg-gray-700 transition-colors",
                    div { class: "font-medium", "FolderImportView" }
                    div { class: "text-sm text-gray-400", "Folder import workflow with all phases" }
                }
                Link {
                    to: Route::MockAlbumDetail {
                        state: None,
                    },
                    class: "block p-4 bg-gray-800 rounded-lg hover:bg-gray-700 transition-colors",
                    div { class: "font-medium", "AlbumDetailView" }
                    div { class: "text-sm text-gray-400", "Album detail page with tracks and controls" }
                }
            }
        }
    }
}

// ============================================================================
// FolderImport page wrapper
// ============================================================================

fn parse_import_phase(s: &str) -> Option<ImportPhase> {
    match s {
        "FolderSelection" => Some(ImportPhase::FolderSelection),
        "ReleaseSelection" => Some(ImportPhase::ReleaseSelection),
        "MetadataDetection" => Some(ImportPhase::MetadataDetection),
        "ExactLookup" => Some(ImportPhase::ExactLookup),
        "ManualSearch" => Some(ImportPhase::ManualSearch),
        "Confirmation" => Some(ImportPhase::Confirmation),
        _ => None,
    }
}

#[component]
pub fn MockFolderImport(state: Option<String>) -> Element {
    let state_pairs = state.as_deref().map(parse_state).unwrap_or_default();

    // Initialize signals from URL state
    let phase = use_signal(|| {
        get_state_enum(
            &state_pairs,
            "phase",
            ImportPhase::FolderSelection,
            parse_import_phase,
        )
    });
    let is_dragging = use_signal(|| get_state_bool(&state_pairs, "dragging", false));
    let is_detecting_metadata = use_signal(|| get_state_bool(&state_pairs, "detecting", false));
    let is_loading_exact_matches = use_signal(|| get_state_bool(&state_pairs, "loading", false));
    let is_retrying_discid_lookup = use_signal(|| get_state_bool(&state_pairs, "retrying", false));
    let is_searching = use_signal(|| get_state_bool(&state_pairs, "searching", false));
    let has_searched = use_signal(|| get_state_bool(&state_pairs, "results", false));
    let is_importing = use_signal(|| get_state_bool(&state_pairs, "importing", false));
    let show_error = use_signal(|| get_state_bool(&state_pairs, "error", false));
    let show_discid_error = use_signal(|| get_state_bool(&state_pairs, "discid_error", false));

    // Sync state changes to URL
    let mut is_mounted = use_signal(|| false);
    use_effect(move || {
        let p = phase();
        let drag = is_dragging();
        let detect = is_detecting_metadata();
        let load = is_loading_exact_matches();
        let retry = is_retrying_discid_lookup();
        let search = is_searching();
        let results = has_searched();
        let import = is_importing();
        let err = show_error();
        let discid_err = show_discid_error();

        if !*is_mounted.peek() {
            is_mounted.set(true);
            return;
        }

        let mut builder = StateBuilder::new();
        builder.set_enum("phase", &p, &ImportPhase::FolderSelection);
        builder.set_bool("dragging", drag, false);
        builder.set_bool("detecting", detect, false);
        builder.set_bool("loading", load, false);
        builder.set_bool("retrying", retry, false);
        builder.set_bool("searching", search, false);
        builder.set_bool("results", results, false);
        builder.set_bool("importing", import, false);
        builder.set_bool("error", err, false);
        builder.set_bool("discid_error", discid_err, false);

        navigator().replace(Route::MockFolderImport {
            state: builder.build_option(),
        });
    });

    rsx! {
        FolderImportMock {
            phase,
            is_dragging,
            is_detecting_metadata,
            is_loading_exact_matches,
            is_retrying_discid_lookup,
            is_searching,
            has_searched,
            is_importing,
            show_error,
            show_discid_error,
        }
    }
}

// ============================================================================
// AlbumDetail page wrapper
// ============================================================================

#[derive(Clone, Copy, Debug, PartialEq, Default)]
pub enum PlaybackState {
    #[default]
    Stopped,
    Playing,
    Paused,
    Loading,
}

fn parse_playback_state(s: &str) -> Option<PlaybackState> {
    match s {
        "Stopped" => Some(PlaybackState::Stopped),
        "Playing" => Some(PlaybackState::Playing),
        "Paused" => Some(PlaybackState::Paused),
        "Loading" => Some(PlaybackState::Loading),
        _ => None,
    }
}

#[component]
pub fn MockAlbumDetail(state: Option<String>) -> Element {
    let state_pairs = state.as_deref().map(parse_state).unwrap_or_default();

    let playback_state = use_signal(|| {
        get_state_enum(
            &state_pairs,
            "playback",
            PlaybackState::Stopped,
            parse_playback_state,
        )
    });

    // Sync state changes to URL
    let mut is_mounted = use_signal(|| false);
    use_effect(move || {
        let pb = playback_state();

        if !*is_mounted.peek() {
            is_mounted.set(true);
            return;
        }

        let mut builder = StateBuilder::new();
        builder.set_enum("playback", &pb, &PlaybackState::Stopped);
        navigator().replace(Route::MockAlbumDetail {
            state: builder.build_option(),
        });
    });

    rsx! {
        AlbumDetailMock { playback_state }
    }
}
