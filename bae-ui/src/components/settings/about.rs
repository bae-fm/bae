//! About section view

use crate::components::Button;
use dioxus::prelude::*;

/// About section view
#[component]
pub fn AboutSectionView(
    /// App version string
    version: String,
    /// Number of albums in library
    album_count: usize,
    /// Callback for check updates button
    on_check_updates: EventHandler<()>,
) -> Element {
    rsx! {
        div { class: "max-w-2xl",
            h2 { class: "text-xl font-semibold text-white mb-6", "About" }

            div { class: "bg-gray-800 rounded-lg p-6 mb-6",
                h3 { class: "text-lg font-medium text-white mb-4", "Application" }
                div { class: "space-y-3",
                    div { class: "flex justify-between items-center",
                        span { class: "text-gray-400", "Version" }
                        span { class: "text-white font-mono", "{version}" }
                    }
                    div { class: "flex justify-between items-center",
                        span { class: "text-gray-400", "Build" }
                        span { class: "text-white font-mono", "Rust (stable)" }
                    }
                }
                div { class: "mt-4 pt-4 border-t border-gray-700",
                    Button { onclick: move |_| on_check_updates.call(()), "Check for Updates" }
                }
            }

            div { class: "bg-gray-800 rounded-lg p-6",
                h3 { class: "text-lg font-medium text-white mb-4", "Library Statistics" }
                div { class: "bg-gray-700 rounded-lg p-4 text-center",
                    div { class: "text-3xl font-bold text-indigo-400", "{album_count}" }
                    div { class: "text-sm text-gray-400 mt-1", "Albums" }
                }
            }
        }
    }
}
