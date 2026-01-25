//! Confirm dialog view component

use crate::components::{Button, ButtonVariant, Modal};
use dioxus::prelude::*;

/// A generic confirmation dialog view
#[component]
pub fn ConfirmDialogView(
    is_open: ReadSignal<bool>,
    title: String,
    message: String,
    #[props(default = "Confirm".to_string())] confirm_label: String,
    #[props(default = "Cancel".to_string())] cancel_label: String,
    #[props(default = true)] is_destructive: bool,
    on_confirm: EventHandler<()>,
    on_cancel: EventHandler<()>,
) -> Element {
    let confirm_variant = if is_destructive {
        ButtonVariant::Danger
    } else {
        ButtonVariant::Primary
    };

    rsx! {
        Modal { is_open, on_close: move |_| on_cancel.call(()),
            div { class: "bg-gray-800 rounded-lg p-6 max-w-md w-full mx-4",
                h2 { class: "text-xl font-bold text-white mb-4", "{title}" }
                p { class: "text-gray-300 mb-6", "{message}" }
                div { class: "flex gap-3 justify-end",
                    Button {
                        variant: ButtonVariant::Secondary,
                        onclick: move |_| on_cancel.call(()),
                        "{cancel_label}"
                    }
                    Button {
                        variant: confirm_variant,
                        onclick: move |_| on_confirm.call(()),
                        "{confirm_label}"
                    }
                }
            }
        }
    }
}
