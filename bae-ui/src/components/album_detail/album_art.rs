//! Album art component with import progress overlay

use crate::components::icons::MusicIcon;
use dioxus::prelude::*;

/// Album art with import progress spinner overlay
/// is_ephemeral: true when showing temporary cover art (e.g., from external URL during import)
#[component]
pub fn AlbumArt(
    title: String,
    cover_url: Option<String>,
    import_progress: ReadSignal<Option<u8>>,
    #[props(default = false)] is_ephemeral: bool,
) -> Element {
    let container_class = if is_ephemeral {
        "aspect-square bg-gray-700 rounded-lg flex items-center justify-center overflow-hidden relative animate-pulse-glow"
    } else {
        "aspect-square bg-gray-700 rounded-lg flex items-center justify-center overflow-hidden relative"
    };

    rsx! {
        div { class: "{container_class}",
            if let Some(ref url) = cover_url {
                img {
                    src: "{url}",
                    alt: "Album cover for {title}",
                    class: "w-full h-full object-cover",
                }
            } else {
                MusicIcon { class: "w-16 h-16 text-gray-500" }
            }
            if let Some(percent) = import_progress() {
                div { class: "absolute inset-0 bg-black/50 flex items-center justify-center",
                    div { class: "w-[30%] aspect-square",
                        svg {
                            class: "transform -rotate-90",
                            view_box: "0 0 100 100",
                            circle {
                                cx: "50",
                                cy: "50",
                                r: "45",
                                fill: "none",
                                stroke: "rgba(255, 255, 255, 0.2)",
                                stroke_width: "8",
                            }
                            circle {
                                cx: "50",
                                cy: "50",
                                r: "45",
                                fill: "none",
                                stroke: "#3b82f6",
                                stroke_width: "8",
                                stroke_dasharray: "283",
                                stroke_dashoffset: "{283.0 - (283.0 * percent as f32 / 100.0)}",
                                stroke_linecap: "round",
                                class: "transition-all duration-300",
                            }
                        }
                    }
                }
            }
        }
    }
}
