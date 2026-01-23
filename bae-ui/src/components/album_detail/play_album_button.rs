//! Play album button component

use crate::components::icons::{ChevronDownIcon, PlayIcon, PlusIcon};
use crate::components::{Dropdown, Placement};
use dioxus::prelude::*;
use web_sys_x::js_sys;

/// Play album button with dropdown for "add to queue"
/// All callbacks are required - pass noops if actions are not needed.
#[component]
pub fn PlayAlbumButton(
    track_ids: Vec<String>,
    import_progress: Option<u8>,
    import_error: Option<String>,
    is_deleting: bool,
    // Callbacks - all required
    on_play_album: EventHandler<Vec<String>>,
    on_add_to_queue: EventHandler<Vec<String>>,
) -> Element {
    let mut show_play_menu = use_signal(|| false);
    let is_open: ReadSignal<bool> = show_play_menu.into();
    let is_disabled = import_progress.is_some() || import_error.is_some() || is_deleting;
    let button_text = if import_progress.is_some() {
        "Importing..."
    } else if import_error.is_some() {
        "Import Failed"
    } else {
        "Play Album"
    };

    // Unique anchor ID for the dropdown button
    let anchor_id = use_hook(|| format!("play-album-btn-{}", js_sys::Math::random() as u64));

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
                        id: "{anchor_id}",
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

            // Dropdown menu
            Dropdown {
                anchor_id: anchor_id.clone(),
                is_open,
                on_close: move |_| show_play_menu.set(false),
                placement: Placement::BottomEnd,
                class: "bg-gray-700 rounded-lg shadow-lg overflow-hidden border border-gray-600 min-w-[200px]",
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
}
