//! Torrent input view component

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
                button {
                    class: if input_mode == TorrentInputMode::File { "px-4 py-2 bg-blue-600 text-white rounded-lg" } else { "px-4 py-2 bg-gray-700 text-gray-300 rounded-lg hover:bg-gray-600" },
                    onclick: move |_| on_mode_change.call(TorrentInputMode::File),
                    "File"
                }
                button {
                    class: if input_mode == TorrentInputMode::Magnet { "px-4 py-2 bg-blue-600 text-white rounded-lg" } else { "px-4 py-2 bg-gray-700 text-gray-300 rounded-lg hover:bg-gray-600" },
                    onclick: move |_| on_mode_change.call(TorrentInputMode::Magnet),
                    "Magnet Link"
                }
            }

            match input_mode {
                TorrentInputMode::File => rsx! {
                    div { class: "border-2 rounded-lg p-12 {drag_classes}",
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
                                        d: "M19.5 14.25v-2.625a3.375 3.375 0 00-3.375-3.375h-1.5A1.125 1.125 0 0113.5 7.125v-1.5a3.375 3.375 0 00-3.375-3.375H8.25m6.75 12l-3-3m0 0l-3 3m3-3v6m-1.5-15H5.625c-.621 0-1.125.504-1.125 1.125v17.25c0 .621.504 1.125 1.125 1.125h12.75c.621 0 1.125-.504 1.125-1.125V11.25a9 9 0 00-9-9z",
                                    }
                                }
                            }
                            div { class: "text-center space-y-2",
                                h3 { class: "text-lg font-semibold text-gray-200", "Select a torrent file" }
                                p { class: "text-sm text-gray-400", "Drop a .torrent file here or click to browse" }
                            }
                            button {
                                class: "px-6 py-3 bg-blue-600 text-white rounded-lg hover:bg-blue-700 transition-colors font-medium",
                                onclick: move |_| on_select_click.call(()),
                                "Select Torrent"
                            }
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
                        button {
                            class: "px-6 py-3 bg-blue-600 text-white rounded-lg hover:bg-blue-700 transition-colors font-medium disabled:opacity-50 disabled:cursor-not-allowed",
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
