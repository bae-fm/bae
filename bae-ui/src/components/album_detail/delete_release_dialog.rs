//! Delete release confirmation dialog

use crate::components::{Button, ButtonVariant, Modal};
use dioxus::prelude::*;

#[component]
pub fn DeleteReleaseDialog(
    is_open: ReadSignal<bool>,
    release_id: String,
    is_last_release: bool,
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
                h2 { class: "text-xl font-bold text-white mb-4", "Delete Release?" }
                p { class: "text-gray-300 mb-6",
                    "Are you sure you want to delete this release? This will delete all tracks and associated data for this release."
                    if is_last_release {
                        " Since this is the only release, the album will also be deleted."
                    } else {
                        ""
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
                            let release_id = release_id.clone();
                            move |_| {
                                if !is_deleting() {
                                    on_confirm.call(release_id.clone());
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
