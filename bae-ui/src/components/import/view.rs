//! Import view - main import page layout

use super::source_selector::{ImportSource, ImportSourceSelectorView};
use dioxus::prelude::*;

/// Import page view
#[component]
pub fn ImportView(
    selected_source: ImportSource,
    on_source_select: EventHandler<ImportSource>,
    children: Element,
) -> Element {
    rsx! {
        div { class: "max-w-4xl mx-auto p-6",
            div { class: "mb-6",
                h1 { class: "text-2xl font-bold text-white", "Import" }
            }
            div { class: "bg-gray-900 rounded-lg shadow p-4",
                ImportSourceSelectorView { selected_source, on_source_select }
                {children}
            }
        }
    }
}
