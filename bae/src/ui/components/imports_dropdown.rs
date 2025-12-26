use super::active_imports_context::{use_active_imports, ActiveImport};
use crate::db::ImportOperationStatus;
use crate::ui::Route;
use dioxus::prelude::*;

/// Dropdown showing list of active imports with progress
#[component]
pub fn ImportsDropdown(mut is_open: Signal<bool>) -> Element {
    let active_imports = use_active_imports();
    let imports = active_imports.imports.read();
    let navigator = use_navigator();

    let import_count = imports.len();

    if !*is_open.read() {
        return rsx! {};
    }

    rsx! {
        // Click-outside overlay
        div {
            class: "fixed inset-0 z-[1600]",
            onclick: move |_| is_open.set(false),
        }

        // Dropdown panel
        div {
            class: "absolute top-full right-0 mt-2 w-96 bg-gray-900 border border-gray-700 rounded-xl shadow-2xl z-[1700] overflow-hidden",

            // Header
            div { class: "px-4 py-3 bg-gray-800/50 border-b border-gray-700 flex items-center justify-between",
                div { class: "flex items-center gap-2",
                    // Download icon
                    svg {
                        class: "h-4 w-4 text-indigo-400",
                        fill: "none",
                        stroke: "currentColor",
                        view_box: "0 0 24 24",
                        stroke_width: "2",
                        path {
                            stroke_linecap: "round",
                            stroke_linejoin: "round",
                            d: "M4 16v1a3 3 0 003 3h10a3 3 0 003-3v-1m-4-4l-4 4m0 0l-4-4m4 4V4"
                        }
                    }
                    h3 { class: "text-sm font-semibold text-white", "Imports" }
                    span { class: "text-xs text-gray-500", "({import_count})" }
                }
                if import_count > 0 {
                    button {
                        class: "text-xs text-gray-400 hover:text-red-400 transition-colors px-2 py-1 rounded hover:bg-gray-700/50",
                        onclick: {
                            let mut imports_signal = active_imports.imports;
                            move |e: Event<MouseData>| {
                                e.stop_propagation();
                                imports_signal.with_mut(|list| list.clear());
                            }
                        },
                        "Clear all"
                    }
                }
            }

            // Import list
            if imports.is_empty() {
                div { class: "px-4 py-8 text-center",
                    svg {
                        class: "h-10 w-10 text-gray-600 mx-auto mb-3",
                        fill: "none",
                        stroke: "currentColor",
                        view_box: "0 0 24 24",
                        stroke_width: "1.5",
                        path {
                            stroke_linecap: "round",
                            stroke_linejoin: "round",
                            d: "M9 12h6m-6 4h6m2 5H7a2 2 0 01-2-2V5a2 2 0 012-2h5.586a1 1 0 01.707.293l5.414 5.414a1 1 0 01.293.707V19a2 2 0 01-2 2z"
                        }
                    }
                    p { class: "text-gray-500 text-sm", "No active imports" }
                }
            } else {
                div { class: "max-h-96 overflow-y-auto divide-y divide-gray-800",
                    for import in imports.iter() {
                        {
                            let import_id = import.import_id.clone();
                            rsx! {
                                ImportItem {
                                    key: "{import_id}",
                                    import: import.clone(),
                                    on_click: {
                                        let navigator = navigator.clone();
                                        let release_id = import.release_id.clone();
                                        let mut is_open = is_open;
                                        move |_| {
                                            is_open.set(false);
                                            if let Some(ref rid) = release_id {
                                                navigator.push(Route::AlbumDetail {
                                                    album_id: rid.clone(),
                                                    release_id: String::new(),
                                                });
                                            }
                                        }
                                    },
                                    on_dismiss: {
                                        let import_id = import_id.clone();
                                        move |_| {
                                            active_imports.dismiss(&import_id);
                                        }
                                    },
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

/// Single import item in the dropdown
#[component]
fn ImportItem(
    import: ActiveImport,
    on_click: EventHandler<()>,
    on_dismiss: EventHandler<()>,
) -> Element {
    let is_complete = import.status == ImportOperationStatus::Complete;
    let is_failed = import.status == ImportOperationStatus::Failed;
    let is_importing = import.status == ImportOperationStatus::Importing;
    let progress_percent = import.progress_percent.unwrap_or(0);

    let status_color = match import.status {
        ImportOperationStatus::Preparing => "text-yellow-500",
        ImportOperationStatus::Importing => "text-indigo-400",
        ImportOperationStatus::Complete => "text-green-500",
        ImportOperationStatus::Failed => "text-red-500",
    };

    let status_text = match import.status {
        ImportOperationStatus::Preparing => {
            if let Some(step) = import.current_step {
                step.display_text().to_string()
            } else {
                "Preparing...".to_string()
            }
        }
        ImportOperationStatus::Importing => {
            if progress_percent > 0 {
                format!("{}% complete", progress_percent)
            } else {
                "Starting...".to_string()
            }
        }
        ImportOperationStatus::Complete => "Import complete".to_string(),
        ImportOperationStatus::Failed => "Import failed".to_string(),
    };

    let cursor_class = if is_complete {
        "cursor-pointer"
    } else {
        "cursor-default"
    };

    rsx! {
        div {
            class: "group px-4 py-3 hover:bg-gray-800/50 transition-colors {cursor_class}",
            onclick: move |_| {
                if is_complete {
                    on_click.call(());
                }
            },

            div { class: "flex items-start gap-3",
                // Status icon
                div { class: "flex-shrink-0 mt-0.5",
                    if is_complete {
                        svg {
                            class: "h-5 w-5 text-green-500",
                            fill: "none",
                            stroke: "currentColor",
                            view_box: "0 0 24 24",
                            stroke_width: "2",
                            path {
                                stroke_linecap: "round",
                                stroke_linejoin: "round",
                                d: "M9 12l2 2 4-4m6 2a9 9 0 11-18 0 9 9 0 0118 0z"
                            }
                        }
                    } else if is_failed {
                        svg {
                            class: "h-5 w-5 text-red-500",
                            fill: "none",
                            stroke: "currentColor",
                            view_box: "0 0 24 24",
                            stroke_width: "2",
                            path {
                                stroke_linecap: "round",
                                stroke_linejoin: "round",
                                d: "M12 8v4m0 4h.01M21 12a9 9 0 11-18 0 9 9 0 0118 0z"
                            }
                        }
                    } else {
                        // Animated spinner
                        svg {
                            class: "h-5 w-5 text-indigo-400 animate-spin",
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
                                d: "M4 12a8 8 0 018-8V0C5.373 0 0 5.373 0 12h4zm2 5.291A7.962 7.962 0 014 12H0c0 3.042 1.135 5.824 3 7.938l3-2.647z"
                            }
                        }
                    }
                }

                // Text content
                div { class: "flex-1 min-w-0",
                    p { class: "text-sm font-medium text-white truncate",
                        "{import.album_title}"
                    }
                    if !import.artist_name.is_empty() {
                        p { class: "text-xs text-gray-400 truncate",
                            "{import.artist_name}"
                        }
                    }
                    p { class: "text-xs {status_color} mt-1",
                        "{status_text}"
                    }

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

                // Dismiss button - always visible on hover
                button {
                    class: "flex-shrink-0 p-1.5 text-gray-600 hover:text-white hover:bg-gray-700 rounded-lg transition-all opacity-0 group-hover:opacity-100",
                    onclick: move |e: Event<MouseData>| {
                        e.stop_propagation();
                        on_dismiss.call(());
                    },
                    title: "Dismiss",
                    svg {
                        class: "h-4 w-4",
                        fill: "none",
                        stroke: "currentColor",
                        view_box: "0 0 24 24",
                        stroke_width: "2",
                        path {
                            stroke_linecap: "round",
                            stroke_linejoin: "round",
                            d: "M6 18L18 6M6 6l12 12"
                        }
                    }
                }
            }
        }
    }
}
