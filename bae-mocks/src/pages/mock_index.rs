//! Mock pages with URL state persistence

use crate::mocks::{AlbumDetailMock, FolderImportMock, LibraryMock};
use crate::Route;
use dioxus::prelude::*;

#[component]
pub fn MockIndex() -> Element {
    rsx! {
        div { class: "min-h-screen bg-gray-900 text-white p-8",
            h1 { class: "text-2xl font-bold mb-6", "Component Mocks" }
            div { class: "space-y-2",
                Link {
                    to: Route::MockLibrary { state: None },
                    class: "block p-4 bg-gray-800 rounded-lg hover:bg-gray-700 transition-colors",
                    div { class: "font-medium", "LibraryView" }
                    div { class: "text-sm text-gray-400", "Album grid with loading/error/empty states" }
                }
                Link {
                    to: Route::MockAlbumDetail { state: None },
                    class: "block p-4 bg-gray-800 rounded-lg hover:bg-gray-700 transition-colors",
                    div { class: "font-medium", "AlbumDetailView" }
                    div { class: "text-sm text-gray-400", "Album detail page with tracks and controls" }
                }
                Link {
                    to: Route::MockFolderImport { state: None },
                    class: "block p-4 bg-gray-800 rounded-lg hover:bg-gray-700 transition-colors",
                    div { class: "font-medium", "FolderImportView" }
                    div { class: "text-sm text-gray-400", "Folder import workflow with all phases" }
                }
            }
        }
    }
}

// ============================================================================
// FolderImport page wrapper
// ============================================================================

#[component]
pub fn MockFolderImport(state: Option<String>) -> Element {
    rsx! {
        FolderImportMock { initial_state: state }
    }
}

// ============================================================================
// AlbumDetail page wrapper
// ============================================================================

#[component]
pub fn MockAlbumDetail(state: Option<String>) -> Element {
    rsx! {
        AlbumDetailMock { initial_state: state }
    }
}

// ============================================================================
// Library page wrapper
// ============================================================================

#[component]
pub fn MockLibrary(state: Option<String>) -> Element {
    rsx! {
        LibraryMock { initial_state: state }
    }
}
