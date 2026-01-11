//! Text file modal wrapper - delegates to TextFileModalView

use bae_ui::components::import::TextFileModalView;
use dioxus::prelude::*;

#[component]
pub fn TextFileModal(filename: String, content: String, on_close: EventHandler<()>) -> Element {
    rsx! {
        TextFileModalView { filename, content, on_close: move |_| on_close.call(()) }
    }
}
