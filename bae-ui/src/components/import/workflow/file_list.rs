//! File list view component

use crate::display_types::FileInfo;
use crate::format_file_size;
use dioxus::prelude::*;

/// Displays a list of files with name, size, and format
#[component]
pub fn FileListView(files: Vec<FileInfo>) -> Element {
    if files.is_empty() {
        return rsx! {
            div { class: "text-gray-400 text-center py-8", "No files found" }
        };
    }

    rsx! {
        div { class: "space-y-2",
            for file in files.iter() {
                div { class: "flex items-center justify-between py-2 px-3 bg-gray-800 rounded hover:bg-gray-700 transition-colors border border-gray-700",
                    div { class: "flex-1",
                        div { class: "text-white text-sm font-medium", {file.name.clone()} }
                        div { class: "text-gray-400 text-xs mt-1",
                            {format!("{} • {}", format_file_size(file.size as i64), file.format)}
                        }
                    }
                }
            }
        }
    }
}
