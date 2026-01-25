//! Image lightbox view component

use crate::components::icons::{ChevronLeftIcon, ChevronRightIcon, XIcon};
use crate::components::Modal;
use crate::display_types::FileInfo;
use dioxus::prelude::*;

/// Image lightbox view for viewing images in full screen
#[component]
pub fn ImageLightboxView(
    /// Artwork files with display_url
    images: Vec<FileInfo>,
    /// Current image index
    current_index: usize,
    /// Called when lightbox is closed
    on_close: EventHandler<()>,
    /// Called when navigating to a different image
    on_navigate: EventHandler<usize>,
) -> Element {
    // Component is only rendered when open, so is_open is always true
    let is_open = use_memo(|| true);

    let total = images.len();

    if total == 0 {
        return rsx! {
            Modal { is_open, on_close,
                div { class: "text-white", "No images available" }
            }
        };
    }

    let clamped_index = current_index.min(total - 1);
    let file = &images[clamped_index];
    let filename = &file.name;
    let url = &file.display_url;
    let can_prev = clamped_index > 0;
    let can_next = clamped_index < total - 1;

    // Keyboard navigation handler
    let on_keydown = move |evt: KeyboardEvent| match evt.key() {
        Key::ArrowLeft if can_prev => on_navigate.call(clamped_index - 1),
        Key::ArrowRight if can_next => on_navigate.call(clamped_index + 1),
        _ => {}
    };

    rsx! {
        Modal { is_open, on_close,
            div { tabindex: 0, autofocus: true, onkeydown: on_keydown,

                // Close button - fixed to viewport
                button {
                    class: "fixed top-4 right-4 text-gray-400 hover:text-white transition-colors z-10",
                    onclick: move |e| {
                        e.stop_propagation();
                        on_close.call(());
                    },
                    XIcon { class: "w-6 h-6" }
                }

                // Image counter - fixed to viewport
                if total > 1 {
                    div { class: "fixed top-4 left-4 text-gray-400 text-sm z-10",
                        {format!("{} / {}", clamped_index + 1, total)}
                    }
                }

                // Previous button - fixed to viewport
                if can_prev {
                    button {
                        class: "fixed left-4 top-1/2 -translate-y-1/2 w-14 h-14 bg-gray-800/60 hover:bg-gray-700/80 rounded-full flex items-center justify-center transition-colors z-10",
                        onclick: move |e| {
                            e.stop_propagation();
                            on_navigate.call(clamped_index - 1);
                        },
                        ChevronLeftIcon {
                            class: "w-8 h-8 text-gray-300 -translate-x-0.5",
                            stroke_width: "1.5",
                        }
                    }
                }

                // Next button - fixed to viewport
                if can_next {
                    button {
                        class: "fixed right-4 top-1/2 -translate-y-1/2 w-14 h-14 bg-gray-800/60 hover:bg-gray-700/80 rounded-full flex items-center justify-center transition-colors z-10",
                        onclick: move |e| {
                            e.stop_propagation();
                            on_navigate.call(clamped_index + 1);
                        },
                        ChevronRightIcon {
                            class: "w-8 h-8 text-gray-300 translate-x-0.5",
                            stroke_width: "1.5",
                        }
                    }
                }

                // Image and filename - centered by Modal
                div { class: "flex flex-col items-center",
                    img {
                        src: "{url}",
                        alt: "{filename}",
                        class: "max-w-[90vw] max-h-[80vh] object-contain rounded-lg shadow-2xl",
                    }
                    div { class: "mt-4 text-gray-300 text-sm", {filename.clone()} }
                }
            }
        }
    }
}
