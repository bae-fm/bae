//! Folder selector view component

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
                    svg {
                        xmlns: "http://www.w3.org/2000/svg",
                        fill: "none",
                        view_box: "0 0 24 24",
                        stroke_width: "1.5",
                        stroke: "currentColor",
                        class: "w-full h-full",
                        path {
                            stroke_linecap: "round",
                            stroke_linejoin: "round",
                            d: "M3 16.5v2.25A2.25 2.25 0 005.25 21h13.5A2.25 2.25 0 0021 18.75V16.5m-13.5-9L12 3m0 0l4.5 4.5M12 3v13.5m-7.5 0h15",
                        }
                    }
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
