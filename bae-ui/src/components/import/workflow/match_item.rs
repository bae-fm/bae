//! Match item view component

use crate::components::icons::ImageIcon;
use crate::display_types::{MatchCandidate, MatchSourceType};
use dioxus::prelude::*;

/// Displays a single match candidate
#[component]
pub fn MatchItemView(
    candidate: MatchCandidate,
    is_selected: bool,
    on_select: EventHandler<()>,
) -> Element {
    let border_class = if is_selected {
        "border-blue-500 bg-blue-900/30"
    } else {
        "border-gray-700 hover:border-gray-600"
    };

    let radio_border = if is_selected {
        "border-blue-500"
    } else {
        "border-gray-500"
    };

    let (format_text, country_text, label_text, catalog_text) = match candidate.source_type {
        MatchSourceType::MusicBrainz => (
            candidate.format.as_ref().map(|f| format!("Format: {}", f)),
            candidate
                .country
                .as_ref()
                .map(|c| format!("Country: {}", c)),
            candidate.label.as_ref().map(|l| format!("Label: {}", l)),
            candidate
                .catalog_number
                .as_ref()
                .map(|c| format!("Catalog: {}", c)),
        ),
        MatchSourceType::Discogs => (None, None, None, None),
    };

    rsx! {
        div {
            class: "border rounded-lg px-3 py-2 cursor-pointer hover:bg-gray-700 transition-colors {border_class}",
            onclick: move |_| on_select.call(()),

            div { class: "flex items-center gap-3",
                // Radio indicator
                div { class: "w-4 h-4 rounded-full border-2 flex-shrink-0 flex items-center justify-center {radio_border}",
                    if is_selected {
                        div { class: "w-2 h-2 rounded-full bg-blue-500" }
                    }
                }

                // Cover art
                div { class: "w-10 h-10 flex-shrink-0 bg-gray-700 rounded overflow-hidden",
                    if let Some(ref cover_url) = candidate.cover_url {
                        img {
                            src: "{cover_url}",
                            alt: "Album cover",
                            class: "w-full h-full object-cover",
                        }
                    } else {
                        div { class: "w-full h-full flex items-center justify-center text-gray-500",
                            ImageIcon { class: "w-5 h-5" }
                        }
                    }
                }

                // Info
                div { class: "flex-1 min-w-0",
                    h4 { class: "text-sm font-medium text-white truncate", "{candidate.title}" }
                    div { class: "text-xs text-gray-400 flex flex-wrap gap-x-3",
                        if let Some(ref year) = candidate.year {
                            span { "{year}" }
                        }
                        if let Some(ref fmt) = format_text {
                            span { "{fmt}" }
                        }
                        if let Some(ref country) = country_text {
                            span { "{country}" }
                        }
                    }
                    if label_text.is_some() || catalog_text.is_some() {
                        div { class: "text-xs text-gray-500 flex flex-wrap gap-x-3",
                            if let Some(ref label) = label_text {
                                span { "{label}" }
                            }
                            if let Some(ref catalog) = catalog_text {
                                span { "{catalog}" }
                            }
                        }
                    }
                }
            }
        }
    }
}
