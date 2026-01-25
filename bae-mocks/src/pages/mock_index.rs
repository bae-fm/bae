//! Mock pages with URL state persistence

use crate::mocks::framework::MockPage;
use crate::mocks::{AlbumDetailMock, ButtonMock, FolderImportMock, LibraryMock, TitleBarMock};
use crate::ui::LinkCard;
use crate::Route;
use dioxus::prelude::*;

#[component]
pub fn MockIndex() -> Element {
    rsx! {
        div { class: "min-h-screen bg-gray-900 text-white p-8",
            h1 { class: "text-2xl font-bold mb-6", "bae mocks" }

            h2 { class: "text-lg font-semibold text-gray-400 mb-3", "Demo App" }
            div { class: "space-y-2 mb-8",
                LinkCard {
                    to: Route::Library {},
                    title: "Full App",
                    description: "Complete app with nav, playback bar, and all pages",
                }
            }

            h2 { class: "text-lg font-semibold text-gray-400 mb-3", "Components" }
            div { class: "space-y-2",
                for page in MockPage::ALL {
                    LinkCard {
                        to: page.to_route(None),
                        title: page.label(),
                        description: page.description(),
                    }
                }
            }
        }
    }
}

// ============================================================================
// Button page wrapper
// ============================================================================

#[component]
pub fn MockButton(state: Option<String>) -> Element {
    rsx! {
        ButtonMock { initial_state: state }
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

// ============================================================================
// TitleBar page wrapper
// ============================================================================

#[component]
pub fn MockTitleBar(state: Option<String>) -> Element {
    rsx! {
        TitleBarMock { initial_state: state }
    }
}
