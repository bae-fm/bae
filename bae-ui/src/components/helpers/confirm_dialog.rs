//! Confirm dialog view component

use dioxus::prelude::*;

/// A generic confirmation dialog view
#[component]
pub fn ConfirmDialogView(
    title: String,
    message: String,
    #[props(default = "Confirm".to_string())] confirm_label: String,
    #[props(default = "Cancel".to_string())] cancel_label: String,
    #[props(default = true)] is_destructive: bool,
    on_confirm: EventHandler<()>,
    on_cancel: EventHandler<()>,
) -> Element {
    let confirm_class = if is_destructive {
        "px-4 py-2 bg-red-600 hover:bg-red-700 text-white rounded-lg"
    } else {
        "px-4 py-2 bg-indigo-600 hover:bg-indigo-700 text-white rounded-lg"
    };

    rsx! {
        div {
            class: "fixed inset-0 bg-black/50 flex items-center justify-center z-[3000]",
            onclick: move |_| on_cancel.call(()),
            div {
                class: "bg-gray-800 rounded-lg p-6 max-w-md w-full mx-4",
                onclick: move |evt| evt.stop_propagation(),
                h2 { class: "text-xl font-bold text-white mb-4", "{title}" }
                p { class: "text-gray-300 mb-6", "{message}" }
                div { class: "flex gap-3 justify-end",
                    button {
                        class: "px-4 py-2 bg-gray-700 hover:bg-gray-600 text-white rounded-lg",
                        onclick: move |_| on_cancel.call(()),
                        "{cancel_label}"
                    }
                    button {
                        class: "{confirm_class}",
                        onclick: move |_| on_confirm.call(()),
                        "{confirm_label}"
                    }
                }
            }
        }
    }
}
