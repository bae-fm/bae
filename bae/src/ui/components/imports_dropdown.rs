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

    // Check if there are any finished imports to dismiss
    let has_finished = imports.iter().any(|i| {
        i.status == ImportOperationStatus::Complete || i.status == ImportOperationStatus::Failed
    });

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
            class: "absolute top-full right-0 mt-2 w-80 bg-gray-800 border border-gray-700 rounded-lg shadow-xl z-[1700] overflow-hidden",

            // Header with dismiss all button
            div { class: "px-4 py-3 border-b border-gray-700 flex items-center justify-between",
                h3 { class: "text-sm font-semibold text-white", "Imports" }
                if has_finished {
                    button {
                        class: "text-xs text-gray-400 hover:text-white transition-colors",
                        onclick: move |e: Event<MouseData>| {
                            e.stop_propagation();
                            active_imports.dismiss_all_finished();
                        },
                        "Clear finished"
                    }
                }
            }

            // Import list
            if imports.is_empty() {
                div { class: "px-4 py-6 text-center text-gray-400 text-sm",
                    "No imports"
                }
            } else {
                div { class: "max-h-80 overflow-y-auto",
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
    let status_text = match import.status {
        ImportOperationStatus::Preparing => {
            if let Some(step) = import.current_step {
                step.display_text().to_string()
            } else {
                "Preparing...".to_string()
            }
        }
        ImportOperationStatus::Importing => {
            if let Some(percent) = import.progress_percent {
                format!("Importing... {}%", percent)
            } else {
                "Importing...".to_string()
            }
        }
        ImportOperationStatus::Complete => "Complete - click to view".to_string(),
        ImportOperationStatus::Failed => "Failed".to_string(),
    };

    let progress_percent = import.progress_percent.unwrap_or(0);
    let is_complete = import.status == ImportOperationStatus::Complete;
    let is_failed = import.status == ImportOperationStatus::Failed;
    let is_finished = is_complete || is_failed;

    rsx! {
        div {
            class: if is_complete { "px-4 py-3 hover:bg-gray-700/50 cursor-pointer border-b border-gray-700/50 last:border-b-0 transition-colors" } else { "px-4 py-3 hover:bg-gray-700/50 border-b border-gray-700/50 last:border-b-0 transition-colors" },
            onclick: move |_| {
                if is_complete {
                    on_click.call(());
                }
            },

            // Album info
            div { class: "flex items-start gap-3",
                // Status indicator
                div { class: "flex-shrink-0 mt-1",
                    if is_complete {
                        // Checkmark
                        svg {
                            class: "h-4 w-4 text-green-500",
                            fill: "none",
                            stroke: "currentColor",
                            view_box: "0 0 24 24",
                            stroke_width: "2",
                            path {
                                stroke_linecap: "round",
                                stroke_linejoin: "round",
                                d: "M5 13l4 4L19 7"
                            }
                        }
                    } else if is_failed {
                        // X mark
                        svg {
                            class: "h-4 w-4 text-red-500",
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
                    } else {
                        // Spinner
                        div { class: "animate-spin h-4 w-4 border-2 border-indigo-500 border-t-transparent rounded-full" }
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
                    p { class: "text-xs text-gray-500 mt-1",
                        "{status_text}"
                    }
                }

                // Dismiss button for finished imports
                if is_finished {
                    button {
                        class: "flex-shrink-0 p-1 text-gray-500 hover:text-white transition-colors",
                        onclick: move |e: Event<MouseData>| {
                            e.stop_propagation();
                            on_dismiss.call(());
                        },
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

            // Progress bar (only when importing)
            if import.status == ImportOperationStatus::Importing && progress_percent > 0 {
                div { class: "mt-2 h-1 bg-gray-700 rounded-full overflow-hidden",
                    div {
                        class: "h-full bg-indigo-500 transition-all duration-300",
                        style: "width: {progress_percent}%",
                    }
                }
            }
        }
    }
}
