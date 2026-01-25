//! Delete album confirmation dialog

use crate::components::{Button, ButtonVariant, Modal};
use dioxus::prelude::*;

#[component]
pub fn DeleteAlbumDialog(
    is_open: ReadSignal<bool>,
    album_id: String,
    release_count: usize,
    is_deleting: Signal<bool>,
    on_confirm: EventHandler<String>,
    on_cancel: EventHandler<()>,
) -> Element {
    rsx! {
        Modal {
            is_open,
            on_close: move |_| {
                if !is_deleting() {
                    on_cancel.call(());
                }
            },
            div { class: "bg-gray-800 rounded-lg p-6 max-w-md w-full mx-4",
                h2 { class: "text-xl font-bold text-white mb-4", "Delete Album?" }
                p { class: "text-gray-300 mb-4",
                    "Are you sure you want to delete this album? This will delete all tracks and associated data."
                }
                if release_count > 1 {
                    p { class: "text-red-400 font-semibold mb-4",
                        "This album has {release_count} releases. All of them will be permanently deleted."
                    }
                }
                div { class: "flex gap-3 justify-end",
                    Button {
                        variant: ButtonVariant::Secondary,
                        disabled: is_deleting(),
                        onclick: move |_| {
                            if !is_deleting() {
                                on_cancel.call(());
                            }
                        },
                        "Cancel"
                    }
                    Button {
                        variant: ButtonVariant::Danger,
                        disabled: is_deleting(),
                        loading: is_deleting(),
                        onclick: {
                            let album_id = album_id.clone();
                            move |_| {
                                if !is_deleting() {
                                    on_confirm.call(album_id.clone());
                                }
                            }
                        },
                        if is_deleting() {
                            "Deleting..."
                        } else {
                            "Delete"
                        }
                    }
                }
            }
        }
    }
}
