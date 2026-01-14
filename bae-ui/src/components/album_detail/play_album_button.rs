//! Play album button component

use crate::components::icons::{ChevronDownIcon, PlayIcon, PlusIcon};
use dioxus::prelude::*;

/// Play album button with dropdown for "add to queue"
/// All callbacks are required - pass noops if actions are not needed.
#[component]
pub fn PlayAlbumButton(
    track_ids: Vec<String>,
    import_progress: ReadSignal<Option<u8>>,
    import_error: ReadSignal<Option<String>>,
    is_deleting: ReadSignal<bool>,
    // Callbacks - all required
    on_play_album: EventHandler<Vec<String>>,
    on_add_to_queue: EventHandler<Vec<String>>,
) -> Element {
    let mut show_play_menu = use_signal(|| false);
    let is_disabled = import_progress().is_some() || import_error().is_some() || is_deleting();
    let button_text = if import_progress().is_some() {
        "Importing..."
    } else if import_error().is_some() {
        "Import Failed"
    } else {
        "Play Album"
    };

    rsx! {
        div { class: "relative mt-6",
            div { class: "flex rounded-lg overflow-hidden",
                button {
                    class: "flex-1 px-6 py-3 bg-blue-600 hover:bg-blue-500 text-white font-semibold transition-colors flex items-center justify-center gap-2",
                    disabled: is_disabled,
                    class: if is_disabled { "opacity-50 cursor-not-allowed" } else { "" },
                    onclick: {
                        let track_ids = track_ids.clone();
                        move |_| on_play_album.call(track_ids.clone())
                    },
                    if !is_disabled {
                        PlayIcon { class: "w-4 h-4" }
                    }
                    "{button_text}"
                }
                div { class: "border-l border-blue-500",
                    button {
                        class: "px-3 py-3 bg-blue-600 hover:bg-blue-500 text-white transition-colors flex items-center justify-center",
                        disabled: is_disabled,
                        class: if is_disabled { "opacity-50 cursor-not-allowed" } else { "" },
                        onclick: move |evt| {
                            evt.stop_propagation();
                            if !is_disabled {
                                show_play_menu.set(!show_play_menu());
                            }
                        },
                        ChevronDownIcon { class: "w-4 h-4" }
                    }
                }
            }
            if show_play_menu() {
                div { class: "absolute top-full left-0 right-0 mt-2 bg-gray-700 rounded-lg shadow-lg overflow-hidden z-10 border border-gray-600",
                    button {
                        class: "w-full px-4 py-3 text-left text-white hover:bg-gray-600 transition-colors flex items-center gap-2",
                        disabled: is_disabled,
                        onclick: {
                            let track_ids = track_ids.clone();
                            move |evt| {
                                evt.stop_propagation();
                                show_play_menu.set(false);
                                on_add_to_queue.call(track_ids.clone());
                            }
                        },
                        PlusIcon { class: "w-4 h-4" }
                        "Add Album to Queue"
                    }
                }
            }
        }
        if show_play_menu() {
            div {
                class: "fixed inset-0 z-[5]",
                onclick: move |_| show_play_menu.set(false),
            }
        }
    }
}
