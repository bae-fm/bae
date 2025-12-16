use crate::library::use_library_manager;
use dioxus::prelude::*;

const VERSION: &str = env!("CARGO_PKG_VERSION");

/// About section - version info and library stats
#[component]
pub fn AboutSection() -> Element {
    let library_manager = use_library_manager();
    let mut album_count = use_signal(|| 0usize);

    let lm = library_manager.clone();
    use_effect(move || {
        let lm = lm.clone();
        spawn(async move {
            if let Ok(albums) = lm.get_albums().await {
                album_count.set(albums.len());
            }
        });
    });

    rsx! {
        div { class: "max-w-2xl",
            h2 { class: "text-xl font-semibold text-white mb-6", "About" }

            // Version info
            div { class: "bg-gray-800 rounded-lg p-6 mb-6",
                h3 { class: "text-lg font-medium text-white mb-4", "Application" }

                div { class: "space-y-3",
                    div { class: "flex justify-between",
                        span { class: "text-gray-400", "Version" }
                        span { class: "text-white font-mono", "{VERSION}" }
                    }
                    div { class: "flex justify-between",
                        span { class: "text-gray-400", "Build" }
                        span { class: "text-white font-mono", "Rust (stable)" }
                    }
                }
            }

            // Library stats
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
