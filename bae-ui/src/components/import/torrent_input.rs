//! Torrent input view component

use crate::components::icons::UploadIcon;
use crate::components::{Button, ButtonVariant};
use dioxus::prelude::*;

/// Torrent input mode
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum TorrentInputMode {
    #[default]
    File,
    Magnet,
}

/// Torrent input view - file/magnet selector with drop zone
#[component]
pub fn TorrentInputView(
    /// Current input mode
    input_mode: TorrentInputMode,
    /// Whether drag is currently active
    #[props(default = false)]
    is_dragging: bool,
    /// Called when mode tab is clicked
    on_mode_change: EventHandler<TorrentInputMode>,
    /// Called when select button is clicked
    on_select_click: EventHandler<()>,
    /// Called when magnet link is submitted
    on_magnet_submit: EventHandler<String>,
) -> Element {
    let mut magnet_input = use_signal(String::new);

    let drag_classes = if is_dragging {
        "border-blue-500 bg-blue-900/20 border-solid"
    } else {
        "border-gray-600 border-dashed"
    };

    rsx! {
        div { class: "space-y-4",
            // Mode tabs
            div { class: "flex space-x-4 mb-4",
                Button {
                    variant: if input_mode == TorrentInputMode::File { ButtonVariant::Primary } else { ButtonVariant::Secondary },
                    onclick: move |_| on_mode_change.call(TorrentInputMode::File),
                    "File"
                }
                Button {
                    variant: if input_mode == TorrentInputMode::Magnet { ButtonVariant::Primary } else { ButtonVariant::Secondary },
                    onclick: move |_| on_mode_change.call(TorrentInputMode::Magnet),
                    "Magnet Link"
                }
            }

            match input_mode {
                TorrentInputMode::File => rsx! {
                    div { class: "border-2 rounded-lg p-12 {drag_classes}",
                        div { class: "flex flex-col items-center justify-center space-y-6",
                            div { class: "w-16 h-16 text-gray-400",
                                UploadIcon { class: "w-full h-full" }
                            }
                            div { class: "text-center space-y-2",
                                h3 { class: "text-lg font-semibold text-gray-200", "Select a torrent file" }
                                p { class: "text-sm text-gray-400", "Drop a .torrent file here or click to browse" }
                            }
                            Button { onclick: move |_| on_select_click.call(()), "Select Torrent" }
                        }
                    }
                },
                TorrentInputMode::Magnet => rsx! {
                    div { class: "space-y-4",
                        div {
                            label { class: "block text-sm font-medium text-gray-400 mb-2", "Magnet Link" }
                            input {
                                r#type: "text",
                                class: "w-full px-4 py-3 bg-gray-700 border border-gray-600 rounded-lg text-white placeholder-gray-500 focus:outline-none focus:ring-2 focus:ring-blue-500 font-mono text-sm",
                                placeholder: "magnet:?xt=urn:btih:...",
                                value: "{magnet_input}",
                                oninput: move |e| magnet_input.set(e.value()),
                            }
                        }
                        Button {
                            disabled: magnet_input.read().is_empty(),
                            onclick: move |_| {
                                let value = magnet_input.read().clone();
                                if !value.is_empty() {
                                    on_magnet_submit.call(value);
                                }
                            },
                            "Add Magnet Link"
                        }
                    }
                },
            }
        }
    }
}
