//! Back button component

use crate::components::icons::ChevronLeftIcon;
use crate::components::{Button, ButtonSize, ButtonVariant};
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
        div { class: "pt-4 mb-6 px-6",
            Button {
                variant: ButtonVariant::Ghost,
                size: ButtonSize::Medium,
                onclick: move |_| on_click.call(()),
                ChevronLeftIcon { class: "w-5 h-5" }
                "{text}"
            }
        }
    }
}
