//! Back button component

use crate::components::icons::ChevronLeftIcon;
use dioxus::prelude::*;

/// Back button with customizable text and callback
#[component]
pub fn BackButton(
    /// Text to display (default: "Back to Library")
    #[props(default = "Back to Library".to_string())]
    text: String,
    /// Callback when button is clicked
    on_click: EventHandler<()>,
) -> Element {
    rsx! {
        div { class: "mb-6",
            button {
                class: "inline-flex items-center text-gray-400 hover:text-white transition-colors",
                "data-testid": "back-button",
                onclick: move |_| on_click.call(()),
                ChevronLeftIcon { class: "w-5 h-5 mr-2" }
                "{text}"
            }
        }
    }
}
