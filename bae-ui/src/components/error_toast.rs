//! Generic error toast notification

use crate::components::icons::XIcon;
use dioxus::prelude::*;

/// A dismissible error toast notification
#[component]
pub fn ErrorToast(
    /// Optional title (e.g., "Export Failed")
    title: Option<String>,
    /// The error message to display
    message: String,
    /// Called when the user dismisses the toast
    on_dismiss: EventHandler<()>,
) -> Element {
    rsx! {
        div { class: "fixed bottom-20 right-4 bg-red-600 text-white px-6 py-4 rounded-lg shadow-lg z-50 max-w-md",
            div { class: "flex items-center justify-between gap-4",
                div { class: "flex-1",
                    if let Some(ref title) = title {
                        p { class: "font-medium", "{title}" }
                    }
                    span { class: if title.is_some() { "text-sm text-red-100" } else { "" }, "{message}" }
                }
                button {
                    class: "text-white hover:text-gray-200",
                    onclick: move |_| on_dismiss.call(()),
                    XIcon { class: "w-4 h-4" }
                }
            }
        }
    }
}
