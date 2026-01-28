//! Candidate sidebar view component for import

use crate::components::icons::{CheckIcon, EllipsisIcon, FolderIcon, LoaderIcon, PlusIcon, XIcon};
use crate::components::{Dropdown, Placement};
use crate::display_types::DetectedCandidateStatus;
use crate::stores::import::{ImportState, ImportStateStoreExt};
use dioxus::prelude::*;

pub const MIN_SIDEBAR_WIDTH: f64 = 350.0;
pub const MAX_SIDEBAR_WIDTH: f64 = 500.0;
pub const DEFAULT_SIDEBAR_WIDTH: f64 = 375.0; // w-100

/// Sidebar showing detected candidates with selection and status
///
/// Accepts `ReadStore<ImportState>` - reads at leaf level for granular reactivity.
#[component]
pub fn ReleaseSidebarView(
    /// Import state store - read at leaf level
    state: ReadStore<ImportState>,
    /// Called when a candidate is selected
    on_select: EventHandler<usize>,
    /// Called to add a folder
    on_add_folder: EventHandler<()>,
    /// Called to remove a folder from the list
    on_remove: EventHandler<usize>,
    /// Called to clear all folders
    on_clear_all: EventHandler<()>,
) -> Element {
    // Use lens for is_scanning, computed values need full read
    let is_scanning = *state.is_scanning_candidates().read();
    let st = state.read();
    let candidates = st.get_detected_candidates_display();
    let selected_index = st.get_selected_candidate_index();
    drop(st);

    let mut show_menu = use_signal(|| false);
    let is_open: ReadSignal<bool> = show_menu.into();
    // Static ID - only one sidebar menu per page
    let anchor_id = "release-sidebar-menu";

    rsx! {
        // Panel container - padding comes from parent ImportView
        div { class: "flex-1 px-2 pb-2 min-w-0 h-full",
            div { class: "flex flex-col h-full min-w-0 bg-surface-raised rounded-xl shadow-lg shadow-black/10 overflow-clip",
                // Header
                div { class: "relative px-3 py-2.5 flex items-center justify-between",
                    span { class: "text-xs font-medium text-gray-300",
                        {
                            let count = candidates.len();
                            if count == 0 {
                                "".to_string()
                            } else if count == 1 {
                                "1 possible release".to_string()
                            } else {
                                format!("{} possible releases", count)
                            }
                        }
                    }
                    div { class: "flex items-center gap-1.5",
                        if is_scanning {
                            LoaderIcon { class: "w-3.5 h-3.5 text-gray-400 animate-spin" }
                        }
                        if candidates.is_empty() {
                            button {
                                class: "p-1.5 text-gray-400 hover:text-white transition-colors rounded-md hover:bg-white/5",
                                onclick: move |_| on_add_folder.call(()),
                                title: "Scan folder",
                                PlusIcon { class: "w-4 h-4" }
                            }
                        } else {
                            button {
                                id: "{anchor_id}",
                                class: "p-1.5 text-gray-400 hover:text-white transition-colors rounded-md hover:bg-white/5",
                                onclick: move |evt| {
                                    evt.stop_propagation();
                                    show_menu.set(!show_menu());
                                },
                                title: "More",
                                EllipsisIcon { class: "w-4 h-4" }
                            }
                        }
                    }
                }

                // Dropdown menu for candidates list
                if !candidates.is_empty() {
                    Dropdown {
                        anchor_id: anchor_id.to_string(),
                        is_open,
                        on_close: move |_| show_menu.set(false),
                        placement: Placement::BottomEnd,
                        class: "bg-gray-800 border border-gray-700 rounded-lg shadow-xl min-w-[120px] overflow-clip",
                        button {
                            class: "w-full px-3 py-2 text-left text-sm text-white hover:bg-gray-700 transition-colors",
                            onclick: move |evt| {
                                evt.stop_propagation();
                                show_menu.set(false);
                                on_add_folder.call(());
                            },
                            span { "Add" }
                        }
                        button {
                            class: "w-full px-3 py-2 text-left text-sm text-white hover:bg-gray-700 transition-colors",
                            onclick: move |evt| {
                                evt.stop_propagation();
                                show_menu.set(false);
                                if let Some(window) = web_sys_x::window() {
                                    if window.confirm_with_message("Clear all folders?").unwrap_or(false) {
                                        on_clear_all.call(());
                                    }
                                }
                            },
                            span { "Clear" }
                        }
                    }
                }

                // Divider
                div { class: "mx-1.5 mb-1.5 border-b border-white/10" }

                // Folder list
                div { class: "flex-1 overflow-y-auto p-1.5 pt-0 space-y-0.5 min-w-0",
                    for (index , candidate) in candidates.iter().enumerate() {
                        {
                            let is_selected = selected_index == Some(index);
                            let status = candidate.status;
                            rsx! {
                                div {
                                    key: "{index}",
                                    class: format!(
                                        "group w-full flex items-center gap-2.5 px-3 py-2 rounded-lg transition-all duration-150 min-w-0 cursor-pointer {}",
                                        if is_selected {
                                            "bg-hover text-white"
                                        } else {
                                            "text-gray-200 hover:bg-surface-overlay hover:text-white"
                                        },
                                    ),
                                    title: "{candidate.path}",
                                    onclick: move |_| on_select.call(index),
                                    // Status icon: folder (pending), spinner (importing), check (imported)
                                    match status {
                                        DetectedCandidateStatus::Pending => rsx! {
                                            FolderIcon { class: "w-4 h-4 flex-shrink-0 text-gray-400" }
                                        },
                                        DetectedCandidateStatus::Importing => rsx! {
                                            LoaderIcon { class: "w-4 h-4 flex-shrink-0 text-blue-400 animate-spin" }
                                        },
                                        DetectedCandidateStatus::Imported => rsx! {
                                            CheckIcon { class: "w-4 h-4 flex-shrink-0 text-green-500" }
                                        },
                                    }
                                    div { class: "flex-1 text-xs truncate min-w-0", {candidate.name.clone()} }
                                    // Only show remove button for pending candidates
                                    if status == DetectedCandidateStatus::Pending {
                                        button {
                                            class: "opacity-0 group-hover:opacity-100 p-1 text-gray-400 hover:text-white rounded transition-opacity",
                                            title: "Remove",
                                            onclick: move |e: MouseEvent| {
                                                e.stop_propagation();
                                                on_remove.call(index);
                                            },
                                            XIcon { class: "w-3.5 h-3.5" }
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
