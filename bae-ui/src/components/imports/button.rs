//! Imports button view component
//!
//! Pure, props-based button that shows active imports count in the title bar.

use crate::components::icons::DownloadIcon;
use crate::display_types::{ActiveImport, ImportStatus};
use dioxus::prelude::*;

/// Button in title bar that shows active imports count and toggles dropdown
#[component]
pub fn ImportsButtonView(
    imports: Vec<ActiveImport>,
    is_open: bool,
    on_toggle: EventHandler<()>,
) -> Element {
    let count = imports.len();
    if count == 0 {
        return rsx! {};
    }

    let has_in_progress = imports
        .iter()
        .any(|i| i.status == ImportStatus::Preparing || i.status == ImportStatus::Importing);
    let has_failed = imports.iter().any(|i| i.status == ImportStatus::Failed);

    let badge_color = if has_failed {
        "bg-red-500"
    } else if has_in_progress {
        "bg-indigo-500"
    } else {
        "bg-green-500"
    };

    rsx! {
        button {
            class: "relative flex items-center gap-2 px-3 py-1.5 text-sm font-medium text-gray-300 hover:text-white hover:bg-gray-700/50 rounded-lg transition-colors",
            onclick: move |_| on_toggle.call(()),
            if has_in_progress {
                svg {
                    class: "h-4 w-4 text-indigo-400 animate-spin",
                    fill: "none",
                    view_box: "0 0 24 24",
                    circle {
                        class: "opacity-25",
                        cx: "12",
                        cy: "12",
                        r: "10",
                        stroke: "currentColor",
                        stroke_width: "4",
                    }
                    path {
                        class: "opacity-75",
                        fill: "currentColor",
                        d: "M4 12a8 8 0 018-8V0C5.373 0 0 5.373 0 12h4zm2 5.291A7.962 7.962 0 014 12H0c0 3.042 1.135 5.824 3 7.938l3-2.647z",
                    }
                }
            } else {
                DownloadIcon { class: "h-4 w-4" }
            }
            span { "Imports" }
            span { class: "{badge_color} text-white text-xs font-bold rounded-full h-5 min-w-5 flex items-center justify-center px-1.5",
                "{count}"
            }
        }
    }
}
