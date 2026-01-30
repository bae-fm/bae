//! Candidate sidebar view component for import

use crate::components::helpers::{ConfirmDialogView, Tooltip, TOOLTIP_PADDING_X};
use crate::components::icons::{
    CheckIcon, EllipsisIcon, FolderIcon, LoaderIcon, PlusIcon, TrashIcon, XIcon,
};
use crate::components::{MenuDropdown, MenuItem};
use crate::display_types::DetectedCandidateStatus;
use crate::floating_ui::Placement;
use crate::platform;
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
    /// Called to open a folder in the native file manager
    on_open_folder: EventHandler<String>,
) -> Element {
    // Use lens for is_scanning, computed values need full read
    let is_scanning = *state.is_scanning_candidates().read();
    let st = state.read();
    let candidates = st.get_detected_candidates_display();
    let selected_index = st.get_selected_candidate_index();
    drop(st);

    let mut show_menu = use_signal(|| false);
    let is_open: ReadSignal<bool> = show_menu.into();
    let mut show_clear_confirm = use_signal(|| false);
    let is_clear_confirm_open: ReadSignal<bool> = show_clear_confirm.into();
    // Static ID - only one sidebar menu per page
    let anchor_id = "release-sidebar-menu";

    rsx! {
        // Panel container - padding comes from parent ImportView
        div { class: "flex-1 px-2 pb-2 min-w-0 h-full",
            div { class: "flex flex-col h-full min-w-0 bg-surface-raised rounded-xl shadow-lg shadow-black/10 overflow-clip",
                // Header
                div { class: "relative pt-2.5 px-3 pb-1 flex items-center justify-between",
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
                    MenuDropdown {
                        anchor_id: anchor_id.to_string(),
                        is_open,
                        on_close: move |_| show_menu.set(false),
                        MenuItem {
                            onclick: move |_| {
                                show_menu.set(false);
                                on_add_folder.call(());
                            },
                            FolderIcon { class: "w-3.5 h-3.5 text-gray-400" }
                            span { "Add" }
                        }
                        MenuItem {
                            onclick: move |_| {
                                show_menu.set(false);
                                show_clear_confirm.set(true);
                            },
                            TrashIcon { class: "w-3.5 h-3.5 text-gray-400" }
                            span { "Clear" }
                        }
                    }
                }

                // Divider
                div { class: "mx-1.5 mb-1.5 border-b border-white/10" }

                // Folder list
                div { class: "flex-1 flex flex-col overflow-y-auto p-1.5 pt-0 space-y-0.5 min-w-0",
                    for (index , candidate) in candidates.iter().enumerate() {
                        CandidateRow {
                            key: "{index}",
                            index,
                            name: candidate.name.clone(),
                            path: candidate.path.clone(),
                            status: candidate.status,
                            is_selected: selected_index == Some(index),
                            on_select,
                            on_open_folder,
                            on_remove,
                        }
                    }
                }
            }
        }

        // Confirm dialog rendered outside the panel so it's not clipped
        ConfirmDialogView {
            is_open: is_clear_confirm_open,
            title: "Clear all folders?".to_string(),
            message: "This will remove all detected folders from the list.".to_string(),
            confirm_label: "Clear".to_string(),
            cancel_label: "Cancel".to_string(),
            is_destructive: true,
            on_confirm: move |_| {
                show_clear_confirm.set(false);
                on_clear_all.call(());
            },
            on_cancel: move |_| {
                show_clear_confirm.set(false);
            },
        }
    }
}

/// Single candidate row with tooltip on the folder icon.
#[component]
fn CandidateRow(
    index: usize,
    name: String,
    path: String,
    status: DetectedCandidateStatus,
    is_selected: bool,
    on_select: EventHandler<usize>,
    on_open_folder: EventHandler<String>,
    on_remove: EventHandler<usize>,
) -> Element {
    rsx! {
        div {
            class: format!(
                "group w-full flex items-center gap-2.5 pl-3 pr-2 py-2 rounded-lg transition-all duration-150 min-w-0 cursor-pointer {}",
                if is_selected {
                    "bg-hover text-white"
                } else {
                    "text-gray-200 hover:bg-surface-overlay hover:text-white"
                },
            ),
            onclick: move |_| on_select.call(index),

            match status {
                DetectedCandidateStatus::Pending => {
                    let open_path = path.clone();
                    rsx! {
                        Tooltip {
                            text: platform::reveal_in_file_manager().to_string(),
                            placement: Placement::TopStart,
                            nowrap: true,
                            cross_axis_offset: -TOOLTIP_PADDING_X,
                            button {
                                class: "flex-shrink-0 text-gray-400 hover:text-white transition-colors",
                                onclick: move |e: MouseEvent| {
                                    e.stop_propagation();
                                    on_open_folder.call(open_path.clone());
                                },
                                FolderIcon { class: "w-4 h-4" }
                            }
                        }
                    }
                }
                DetectedCandidateStatus::Importing => rsx! {
                    LoaderIcon { class: "w-4 h-4 flex-shrink-0 text-blue-400 animate-spin" }
                },
                DetectedCandidateStatus::Imported => rsx! {
                    CheckIcon { class: "w-4 h-4 flex-shrink-0 text-green-500" }
                },
            }

            div { class: "flex-1 text-xs truncate min-w-0", {name} }

            if status == DetectedCandidateStatus::Pending {
                button {
                    class: "flex-shrink-0 opacity-0 group-hover:opacity-100 p-1 text-gray-400 hover:text-white rounded transition-opacity",
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
