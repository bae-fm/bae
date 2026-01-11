//! Release selector view component

use crate::display_types::DetectedRelease;
use dioxus::prelude::*;

/// View for selecting multiple releases from a folder
#[component]
pub fn ReleaseSelectorView(
    /// List of detected releases
    releases: Vec<DetectedRelease>,
    /// Currently selected indices
    selected_indices: Vec<usize>,
    /// Called when selection changes
    on_selection_change: EventHandler<Vec<usize>>,
    /// Called when import is clicked
    on_import: EventHandler<Vec<usize>>,
) -> Element {
    let selected_count = selected_indices.len();
    let total_count = releases.len();

    rsx! {
        div { class: "space-y-6",
            div { class: "text-center",
                h2 { class: "text-2xl font-semibold text-gray-100 mb-2", "Multiple Releases Detected" }
                p { class: "text-gray-400", "Select the releases you want to import" }
            }

            // Selection controls
            div { class: "flex justify-between items-center",
                div { class: "text-sm text-gray-400",
                    {format!("{} of {} selected", selected_count, total_count)}
                }
                div { class: "flex gap-2",
                    button {
                        class: "px-3 py-1 text-sm bg-gray-700 hover:bg-gray-600 text-gray-200 rounded transition-colors",
                        onclick: {
                            let all_indices: Vec<usize> = (0..total_count).collect();
                            move |_| on_selection_change.call(all_indices.clone())
                        },
                        "Select All"
                    }
                    button {
                        class: "px-3 py-1 text-sm bg-gray-700 hover:bg-gray-600 text-gray-200 rounded transition-colors",
                        onclick: move |_| on_selection_change.call(Vec::new()),
                        "Deselect All"
                    }
                }
            }

            // Release list
            div { class: "space-y-2 max-h-96 overflow-y-auto",
                for (index , release) in releases.iter().enumerate() {
                    {
                        let is_selected = selected_indices.contains(&index);
                        let checkbox_class = if is_selected {
                            "w-5 h-5 text-blue-500 bg-blue-500 border-blue-500 rounded focus:ring-2 focus:ring-blue-500"
                        } else {
                            "w-5 h-5 text-gray-400 bg-gray-700 border-gray-600 rounded focus:ring-2 focus:ring-blue-500"
                        };
                        let current_selection = selected_indices.clone();
                        rsx! {
                            div {
                                key: "{index}",
                                class: "flex items-start gap-3 p-4 bg-gray-800 rounded-lg hover:bg-gray-750 transition-colors cursor-pointer",
                                onclick: {
                                    let current = current_selection.clone();
                                    move |_| {
                                        let mut new_selection = current.clone();
                                        if let Some(pos) = new_selection.iter().position(|&i| i == index) {
                                            new_selection.remove(pos);
                                        } else {
                                            new_selection.push(index);
                                            new_selection.sort_unstable();
                                        }
                                        on_selection_change.call(new_selection);
                                    }
                                },
                                input {
                                    r#type: "checkbox",
                                    class: "{checkbox_class}",
                                    checked: is_selected,
                                    onclick: |e| e.stop_propagation(),
                                }
                                div { class: "flex-1 min-w-0",
                                    div { class: "font-medium text-gray-100 mb-1", {release.name.clone()} }
                                    div { class: "text-sm text-gray-400 truncate", {release.path.clone()} }
                                }
                            }
                        }
                    }
                }
            }

            // Import button
            div { class: "flex justify-center pt-4",
                button {
                    class: if selected_indices.is_empty() { "px-6 py-3 bg-gray-700 text-gray-500 rounded-lg cursor-not-allowed" } else { "px-6 py-3 bg-blue-600 hover:bg-blue-700 text-white rounded-lg transition-colors" },
                    disabled: selected_indices.is_empty(),
                    onclick: {
                        let indices = selected_indices.clone();
                        move |_| on_import.call(indices.clone())
                    },
                    {
                        if selected_count == 0 {
                            "Select releases to import".to_string()
                        } else if selected_count == 1 {
                            "Import 1 Release".to_string()
                        } else {
                            format!("Import {} Releases", selected_count)
                        }
                    }
                }
            }
        }
    }
}
