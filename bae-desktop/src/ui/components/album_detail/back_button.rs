//! Back button wrapper with app-specific navigation

use crate::ui::Route;
use bae_ui::BackButton as BackButtonView;
use dioxus::prelude::*;

/// Back to library navigation button
#[component]
pub fn BackButton() -> Element {
    rsx! {
        BackButtonView {
            text: "Back to Library",
            on_click: move |_| {
                navigator().push(Route::Library {});
            },
        }
    }
}
