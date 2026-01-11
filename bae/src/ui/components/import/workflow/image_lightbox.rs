//! Image lightbox wrapper - delegates to ImageLightboxView

use bae_ui::components::import::ImageLightboxView;
use dioxus::prelude::*;

#[component]
pub fn ImageLightbox(
    images: Vec<(String, String)>,
    current_index: usize,
    on_close: EventHandler<()>,
    on_navigate: EventHandler<usize>,
) -> Element {
    rsx! {
        ImageLightboxView {
            images,
            current_index,
            on_close: move |_| on_close.call(()),
            on_navigate: move |idx| on_navigate.call(idx),
        }
    }
}
