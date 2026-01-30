//! Selected source display view

use crate::components::icons::{FolderIcon, XIcon};
use dioxus::prelude::*;
use std::path::PathBuf;

/// Displays the selected import source (folder/torrent/CD) with path and clear button
#[component]
pub fn SelectedSourceView(
    #[props(default)] title: String,
    path: String,
    on_clear: EventHandler<()>,
    /// Callback to reveal the folder in Finder.
    on_reveal: EventHandler<()>,
    children: Element,
) -> Element {
    let path_buf = PathBuf::from(&path);
    let display_name = path_buf
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or(&path)
        .to_string();

    rsx! {
        div { class: "bg-gray-800 rounded-lg shadow p-4",
            // Source name + change button in darker container
            div { class: "flex items-center justify-between gap-2 px-3 py-2 bg-gray-900/50 rounded",
                div { class: "flex items-center gap-2 min-w-0",
                    // Folder icon - clickable to reveal in Finder
                    button {
                        class: "text-gray-400 hover:text-gray-200 flex-shrink-0 transition-colors",
                        title: crate::platform::reveal_in_file_manager(),
                        onclick: move |_| on_reveal.call(()),
                        FolderIcon { class: "w-4 h-4" }
                    }
                    span { class: "text-sm text-gray-100 truncate", {display_name} }
                }
                button {
                    class: "p-1 text-gray-400 hover:text-gray-200 flex-shrink-0 rounded hover:bg-gray-700/50 transition-colors",
                    title: "Clear selection",
                    onclick: move |_| on_clear.call(()),
                    XIcon { class: "w-4 h-4" }
                }
            }

            {children}
        }
    }
}
