//! Folder selector view component

use crate::components::icons::UploadIcon;
use dioxus::prelude::*;

/// Folder selector view - drop zone and button for selecting a folder
#[component]
pub fn FolderSelectorView(
    /// Whether drag is currently active
    #[props(default = false)]
    is_dragging: bool,
    /// Called when select button is clicked
    on_select_click: EventHandler<()>,
) -> Element {
    let drag_classes = if is_dragging {
        "border-blue-500 bg-blue-900/20 border-solid"
    } else {
        "border-gray-600 border-dashed"
    };

    rsx! {
        div { class: "border-2 rounded-lg p-12 transition-all duration-200 {drag_classes}",
            div { class: "flex flex-col items-center justify-center space-y-6",
                div { class: "w-16 h-16 text-gray-400",
                    UploadIcon { class: "w-full h-full" }
                }
                div { class: "text-center space-y-2",
                    h3 { class: "text-lg font-semibold text-gray-200", "Select your music folder" }
                    p { class: "text-sm text-gray-400",
                        "Click the button below to choose a folder containing your music files"
                    }
                }
                button {
                    class: "px-6 py-3 bg-blue-600 text-white rounded-lg hover:bg-blue-700 transition-colors font-medium",
                    onclick: move |_| on_select_click.call(()),
                    "Select Folder"
                }
            }
        }
    }
}
