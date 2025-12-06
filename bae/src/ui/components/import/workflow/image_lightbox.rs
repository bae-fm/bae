use dioxus::prelude::*;

#[component]
pub fn ImageLightbox(
    images: Vec<(String, String)>, // Vec of (filename, url)
    current_index: usize,
    on_close: EventHandler<()>,
    on_navigate: EventHandler<usize>,
) -> Element {
    let total = images.len();
    let (filename, url) = &images[current_index];

    let can_prev = current_index > 0;
    let can_next = current_index < total - 1;

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
                div {
                    class: "absolute top-4 left-4 text-gray-400 text-sm",
                    {format!("{} / {}", current_index + 1, total)}
                }
            }

            // Previous button
            if can_prev {
                button {
                    class: "absolute left-4 top-1/2 -translate-y-1/2 w-12 h-12 bg-gray-800/60 hover:bg-gray-700/80 text-white rounded-full flex items-center justify-center transition-colors",
                    onclick: move |e| {
                        e.stop_propagation();
                        on_navigate.call(current_index - 1);
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
                        on_navigate.call(current_index + 1);
                    },
                    "›"
                }
            }

            // Main content
            div {
                class: "flex flex-col items-center max-w-[90vw] max-h-[90vh]",
                onclick: move |e| e.stop_propagation(),

                img {
                    src: "{url}",
                    alt: "{filename}",
                    class: "max-w-full max-h-[80vh] object-contain rounded-lg shadow-2xl",
                }

                // Filename
                div {
                    class: "mt-4 text-gray-300 text-sm",
                    {filename.clone()}
                }
            }
        }
    }
}
