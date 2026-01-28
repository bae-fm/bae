//! Import view - main import page layout
//!
//! Two-panel layout with resizable left sidebar containing:
//! - Import header and source selector
//! - Workflow-specific sidebar content (releases, CD drives, etc.)

use super::source_selector::{ImportSource, ImportSourceSelectorView};
use super::workflow::{
    ReleaseSidebarView, DEFAULT_SIDEBAR_WIDTH, MAX_SIDEBAR_WIDTH, MIN_SIDEBAR_WIDTH,
};
use crate::components::{PanelPosition, ResizablePanel, ResizeDirection};
use crate::stores::import::{ImportState, ImportStateStoreExt};
use dioxus::prelude::*;

/// Import page view with resizable sidebar
#[component]
pub fn ImportView(
    selected_source: ImportSource,
    on_source_select: EventHandler<ImportSource>,
    /// Import state store
    state: ReadStore<ImportState>,
    /// Sidebar: called when a candidate is selected
    on_candidate_select: EventHandler<usize>,
    /// Sidebar: called to add a folder
    on_add_folder: EventHandler<()>,
    /// Sidebar: called to remove a candidate
    on_remove_candidate: EventHandler<usize>,
    /// Sidebar: called to clear all candidates
    on_clear_all: EventHandler<()>,
    children: Element,
) -> Element {
    // Determine if sidebar should be shown based on state
    let is_scanning = *state.is_scanning_candidates().read();
    let has_candidates = !state.read().detected_candidates.is_empty();
    let show_sidebar = has_candidates || is_scanning;

    // Only show source selector if multiple sources are available
    let show_source_selector = ImportSource::all().len() > 1;

    if show_sidebar {
        rsx! {
            div { class: "flex flex-grow min-h-0",
                // Left panel - resizable sidebar with header
                ResizablePanel {
                    storage_key: "import-sidebar-width",
                    min_size: MIN_SIDEBAR_WIDTH,
                    max_size: MAX_SIDEBAR_WIDTH,
                    default_size: DEFAULT_SIDEBAR_WIDTH,
                    grabber_span_ratio: 0.95,
                    direction: ResizeDirection::Horizontal,
                    position: PanelPosition::Relative,
                    div { class: "flex flex-col h-full",
                        // Header with title and source selector
                        div { class: "pt-6 px-5 pb-4 flex items-center gap-4",
                            h1 { class: "text-2xl font-bold text-white", "Import" }
                            if show_source_selector {
                                ImportSourceSelectorView { selected_source, on_source_select }
                            }
                        }
                        // Sidebar content
                        div { class: "flex-1 min-h-0",
                            ReleaseSidebarView {
                                state,
                                on_select: on_candidate_select,
                                on_add_folder,
                                on_remove: on_remove_candidate,
                                on_clear_all,
                            }
                        }
                    }
                }
                // Right panel - main content
                div { class: "flex-1 min-h-0 flex flex-col", {children} }
            }
        }
    } else {
        // No sidebar - header above full-width content
        rsx! {
            div { class: "flex flex-col flex-grow min-h-0",
                // Header with title and source selector
                div { class: "pt-6 px-5 pb-4 flex items-center gap-4",
                    h1 { class: "text-2xl font-bold text-white", "Import" }
                    if show_source_selector {
                        ImportSourceSelectorView { selected_source, on_source_select }
                    }
                }
                // Main content - full width
                div { class: "flex-1 min-h-0 flex flex-col", {children} }
            }
        }
    }
}
