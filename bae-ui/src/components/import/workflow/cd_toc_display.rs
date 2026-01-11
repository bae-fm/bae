//! CD TOC display view component

use dioxus::prelude::*;

/// CD Table of Contents info for display
#[derive(Clone, Debug, PartialEq)]
pub struct CdTocInfo {
    pub disc_id: String,
    pub first_track: u8,
    pub last_track: u8,
}

/// Display for CD TOC info (DiscID, track count)
#[component]
pub fn CdTocDisplayView(
    /// TOC info if available
    toc: Option<CdTocInfo>,
    /// Whether we're currently reading the TOC
    is_reading: bool,
) -> Element {
    if let Some(toc) = toc {
        let track_count = toc.last_track - toc.first_track + 1;
        rsx! {
            div { class: "mt-4 p-4 bg-blue-50 border border-blue-200 rounded-lg",
                div { class: "space-y-2",
                    div { class: "flex items-center",
                        span { class: "text-sm font-medium text-gray-700 w-24", "DiscID:" }
                        span { class: "text-sm text-gray-900 font-mono", "{toc.disc_id}" }
                    }
                    div { class: "flex items-center",
                        span { class: "text-sm font-medium text-gray-700 w-24", "Tracks:" }
                        span { class: "text-sm text-gray-900",
                            "{track_count} tracks ({toc.first_track}-{toc.last_track})"
                        }
                    }
                }
            }
        }
    } else if is_reading {
        rsx! {
            div { class: "mt-4 p-4 bg-gray-50 border border-gray-200 rounded-lg text-center",
                p { class: "text-sm text-gray-600", "Reading CD table of contents..." }
            }
        }
    } else {
        rsx! {}
    }
}
