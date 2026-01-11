//! Selected source wrapper - delegates to SelectedSourceView

use bae_ui::components::import::SelectedSourceView;
use dioxus::prelude::*;

#[component]
pub fn SelectedSource(
    title: String,
    path: Signal<String>,
    on_clear: EventHandler<()>,
    children: Element,
) -> Element {
    rsx! {
        SelectedSourceView {
            title,
            path: path.read().clone(),
            on_clear: move |_| on_clear.call(()),
            {children}
        }
    }
}
