//! Detecting metadata wrapper - delegates to DetectingMetadataView

use bae_ui::components::import::DetectingMetadataView;
use dioxus::prelude::*;

#[component]
pub fn DetectingMetadata(message: String, on_skip: EventHandler<()>) -> Element {
    rsx! {
        DetectingMetadataView { message, on_skip: move |_| on_skip.call(()) }
    }
}
