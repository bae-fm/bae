//! Image lightbox view component

use dioxus::prelude::*;

/// Image lightbox view for viewing images in full screen
#[component]
pub fn ImageLightboxView(
    /// List of (filename, url) pairs
    images: Vec<(String, String)>,
    /// Current image index
    current_index: usize,
    /// Called when lightbox is closed
    on_close: EventHandler<()>,
    /// Called when navigating to a different image
    on_navigate: EventHandler<usize>,
) -> Element {
    let total = images.len();

    if total == 0 {
        return rsx! {
            div {
                class: "fixed inset-0 bg-black/90 flex items-center justify-center z-50",
                onclick: move |_| on_close.call(()),
                div { class: "text-white", "No images available" }
            }
        };
    }

    let clamped_index = current_index.min(total - 1);
    let (filename, url) = &images[clamped_index];
    let can_prev = clamped_index > 0;
    let can_next = clamped_index < total - 1;

    rsx! {
        div {
            class: "fixed inset-0 bg-black/90 flex items-center justify-center z-50",
            onclick: move |_| on_close.call(()),

            // Close button
            button {
                class: "absolute top-4 right-4 text-gray-400 hover:text-white transition-colors text-2xl",
                onclick: move |e| {
                    e.stop_propagation();
                    on_close.call(());
                },
                "✕"
            }

            // Image counter
            if total > 1 {
                div { class: "absolute top-4 left-4 text-gray-400 text-sm",
                    {format!("{} / {}", clamped_index + 1, total)}
                }
            }

            // Previous button
            if can_prev {
                button {
                    class: "absolute left-4 top-1/2 -translate-y-1/2 w-12 h-12 bg-gray-800/60 hover:bg-gray-700/80 text-white rounded-full flex items-center justify-center transition-colors",
                    onclick: move |e| {
                        e.stop_propagation();
                        on_navigate.call(clamped_index - 1);
                    },
                    "‹"
                }
            }

            // Next button
            if can_next {
                button {
                    class: "absolute right-4 top-1/2 -translate-y-1/2 w-12 h-12 bg-gray-800/60 hover:bg-gray-700/80 text-white rounded-full flex items-center justify-center transition-colors",
                    onclick: move |e| {
                        e.stop_propagation();
                        on_navigate.call(clamped_index + 1);
                    },
                    "›"
                }
            }

            // Image and filename
            div {
                class: "flex flex-col items-center max-w-[90vw] max-h-[90vh]",
                onclick: move |e| e.stop_propagation(),
                img {
                    src: "{url}",
                    alt: "{filename}",
                    class: "max-w-full max-h-[80vh] object-contain rounded-lg shadow-2xl",
                }
                div { class: "mt-4 text-gray-300 text-sm", {filename.clone()} }
            }
        }
    }
}
