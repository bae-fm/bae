//! Imports dropdown view component
//!
//! Pure, props-based dropdown showing list of active imports with progress.

use crate::components::icons::{CheckIcon, DownloadIcon, FileTextIcon, ImageIcon, XIcon};
use crate::display_types::{ActiveImport, ImportStatus};
use dioxus::prelude::*;

/// Dropdown showing list of active imports with progress
#[component]
pub fn ImportsDropdownView(
    imports: Vec<ActiveImport>,
    is_open: bool,
    on_close: EventHandler<()>,
    on_import_click: EventHandler<String>,
    on_import_dismiss: EventHandler<String>,
    on_clear_all: EventHandler<()>,
) -> Element {
    if !is_open {
        return rsx! {};
    }

    let import_count = imports.len();

    rsx! {
        // Click-outside overlay
        div {
            class: "fixed inset-0 z-[1600]",
            onclick: move |_| on_close.call(()),
        }

        // Dropdown panel
        div { class: "absolute top-full right-0 mt-2 w-96 bg-gray-900 border border-gray-700 rounded-xl shadow-2xl z-[1700] overflow-hidden",

            // Header
            div { class: "px-4 py-3 bg-gray-800/50 border-b border-gray-700 flex items-center justify-between",
                div { class: "flex items-center gap-2",
                    DownloadIcon { class: "h-4 w-4 text-indigo-400" }
                    h3 { class: "text-sm font-semibold text-white", "Imports" }
                    span { class: "text-xs text-gray-500", "({import_count})" }
                }

                if import_count > 0 {
                    button {
                        class: "text-xs text-gray-400 hover:text-red-400 transition-colors px-2 py-1 rounded hover:bg-gray-700/50",
                        onclick: move |e: Event<MouseData>| {
                            e.stop_propagation();
                            on_clear_all.call(());
                        },
                        "Clear all"
                    }
                }
            }

            // Content
            if imports.is_empty() {
                div { class: "px-4 py-8 text-center",
                    FileTextIcon { class: "h-10 w-10 text-gray-600 mx-auto mb-3" }
                    p { class: "text-gray-500 text-sm", "No active imports" }
                }
            } else {
                div { class: "max-h-96 overflow-y-auto divide-y divide-gray-800",
                    for import in imports.iter() {
                        ImportItemView {
                            key: "{import.import_id}",
                            import: import.clone(),
                            on_click: on_import_click,
                            on_dismiss: on_import_dismiss,
                        }
                    }
                }
            }
        }
    }
}

/// Single import item in the dropdown
#[component]
fn ImportItemView(
    import: ActiveImport,
    on_click: EventHandler<String>,
    on_dismiss: EventHandler<String>,
) -> Element {
    let is_complete = import.status == ImportStatus::Complete;
    let is_failed = import.status == ImportStatus::Failed;
    let is_importing = import.status == ImportStatus::Importing;
    let progress_percent = import.progress_percent.unwrap_or(0);

    let status_color = match import.status {
        ImportStatus::Preparing => "text-yellow-500",
        ImportStatus::Importing => "text-indigo-400",
        ImportStatus::Complete => "text-green-500",
        ImportStatus::Failed => "text-red-500",
    };

    let status_text = match import.status {
        ImportStatus::Preparing => import
            .current_step_text
            .clone()
            .unwrap_or_else(|| "Preparing...".to_string()),
        ImportStatus::Importing => {
            if progress_percent > 0 {
                format!("{}% complete", progress_percent)
            } else {
                "Starting...".to_string()
            }
        }
        ImportStatus::Complete => "Import complete".to_string(),
        ImportStatus::Failed => "Import failed".to_string(),
    };

    let cursor_class = if is_complete {
        "cursor-pointer"
    } else {
        "cursor-default"
    };

    let import_id = import.import_id.clone();
    let import_id_for_dismiss = import.import_id.clone();

    rsx! {
        div {
            class: "group px-4 py-3 hover:bg-gray-800/50 transition-colors {cursor_class}",
            onclick: move |_| {
                if is_complete {
                    on_click.call(import_id.clone());
                }
            },

            div { class: "flex items-start gap-3",
                // Cover art
                div {
                    class: "flex-shrink-0 w-10 h-10 bg-gray-700 rounded overflow-hidden relative",
                    if let Some(ref url) = import.cover_url {
                        img {
                            src: "{url}",
                            alt: "Album cover",
                            class: "w-full h-full object-cover",
                        }
                    } else {
                        div { class: "w-full h-full flex items-center justify-center text-gray-500",
                            ImageIcon { class: "w-5 h-5" }
                        }
                    }

                    // Status badge
                    if is_complete {
                        div { class: "absolute -bottom-0.5 -right-0.5 w-4 h-4 bg-green-500 rounded-full flex items-center justify-center",
                            CheckIcon { class: "h-2.5 w-2.5 text-white" }
                        }
                    } else if is_failed {
                        div { class: "absolute -bottom-0.5 -right-0.5 w-4 h-4 bg-red-500 rounded-full flex items-center justify-center",
                            XIcon { class: "h-2.5 w-2.5 text-white" }
                        }
                    } else {
                        // Animated spinner for in-progress
                        div { class: "absolute -bottom-0.5 -right-0.5 w-4 h-4 bg-indigo-500 rounded-full flex items-center justify-center",
                            svg {
                                class: "h-2.5 w-2.5 text-white animate-spin",
                                fill: "none",
                                view_box: "0 0 24 24",
                                circle {
                                    class: "opacity-25",
                                    cx: "12",
                                    cy: "12",
                                    r: "10",
                                    stroke: "currentColor",
                                    stroke_width: "4",
                                }
                                path {
                                    class: "opacity-75",
                                    fill: "currentColor",
                                    d: "M4 12a8 8 0 018-8V0C5.373 0 0 5.373 0 12h4zm2 5.291A7.962 7.962 0 014 12H0c0 3.042 1.135 5.824 3 7.938l3-2.647z",
                                }
                            }
                        }
                    }
                }

                // Info
                div { class: "flex-1 min-w-0",
                    p { class: "text-sm font-medium text-white truncate", "{import.album_title}" }
                    if !import.artist_name.is_empty() {
                        p { class: "text-xs text-gray-400 truncate", "{import.artist_name}" }
                    }
                    p { class: "text-xs {status_color} mt-1", "{status_text}" }

                    // Progress bar
                    if is_importing && progress_percent > 0 {
                        div { class: "mt-2 h-1.5 bg-gray-700 rounded-full overflow-hidden",
                            div {
                                class: "h-full bg-gradient-to-r from-indigo-500 to-indigo-400 transition-all duration-300 ease-out",
                                style: "width: {progress_percent}%",
                            }
                        }
                    }
                }

                // Dismiss button
                button {
                    class: "flex-shrink-0 p-1.5 text-gray-600 hover:text-white hover:bg-gray-700 rounded-lg transition-all opacity-0 group-hover:opacity-100",
                    onclick: move |e: Event<MouseData>| {
                        e.stop_propagation();
                        on_dismiss.call(import_id_for_dismiss.clone());
                    },
                    title: "Dismiss",
                    XIcon { class: "h-4 w-4" }
                }
            }
        }
    }
}
