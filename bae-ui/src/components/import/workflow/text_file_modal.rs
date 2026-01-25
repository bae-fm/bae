//! Text file modal view component

use crate::components::icons::XIcon;
use crate::components::{ChromelessButton, Modal};
use dioxus::prelude::*;

/// Modal for viewing text file contents (CUE files, logs, etc.)
#[component]
pub fn TextFileModalView(
    /// Controls whether the modal is open
    is_open: ReadSignal<bool>,
    /// Filename to display in header
    filename: String,
    /// File content to display
    content: String,
    /// Called when modal is closed
    on_close: EventHandler<()>,
) -> Element {
    rsx! {
        Modal { is_open, on_close: move |_| on_close.call(()),
            div { class: "bg-gray-800 rounded-lg shadow-xl max-w-4xl w-full max-h-[80vh] flex flex-col",

                // Header
                div { class: "flex items-center justify-between p-4 border-b border-gray-700",
                    h3 { class: "text-lg font-semibold text-white", {filename} }
                    ChromelessButton {
                        class: Some("text-gray-400 hover:text-white transition-colors".to_string()),
                        aria_label: Some("Close".to_string()),
                        onclick: move |_| on_close.call(()),
                        XIcon { class: "w-5 h-5" }
                    }
                }

                // Content
                div { class: "flex-1 overflow-auto p-4",
                    pre { class: "text-sm text-gray-300 font-mono whitespace-pre-wrap select-text",
                        {content}
                    }
                }
            }
        }
    }
}
