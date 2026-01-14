//! Update available indicator
//!
//! Shows a subtle indicator when an app update is ready to install.

use dioxus::prelude::*;

#[cfg(target_os = "macos")]
use crate::updater::{update_state, UpdateState};

/// Polls for update state and shows an indicator when an update is ready.
/// Clicking it triggers the update check UI (which will prompt to restart).
#[component]
pub fn UpdateIndicator() -> Element {
    #[cfg(target_os = "macos")]
    {
        let mut state = use_signal(update_state);

        // Poll update state periodically
        use_future(move || async move {
            loop {
                tokio::time::sleep(std::time::Duration::from_secs(5)).await;
                state.set(update_state());
            }
        });

        let current_state = state();

        match current_state {
            UpdateState::Ready => rsx! {
                button {
                    class: "flex items-center gap-1.5 px-2 py-1 text-xs text-emerald-400 hover:text-emerald-300 hover:bg-emerald-900/30 rounded transition-colors",
                    title: "Update ready - click to restart",
                    onclick: move |_| {
                        crate::updater::check_for_updates();
                    },
                    // Pulsing dot
                    span { class: "relative flex h-2 w-2",
                        span { class: "animate-ping absolute inline-flex h-full w-full rounded-full bg-emerald-400 opacity-75" }
                        span { class: "relative inline-flex rounded-full h-2 w-2 bg-emerald-500" }
                    }
                    "Update"
                }
            },
            UpdateState::Downloading => rsx! {
                div {
                    class: "flex items-center gap-1.5 px-2 py-1 text-xs text-gray-400",
                    title: "Downloading update...",
                    // Spinning indicator
                    svg {
                        class: "animate-spin h-3 w-3",
                        xmlns: "http://www.w3.org/2000/svg",
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
                    "Updating..."
                }
            },
            UpdateState::Idle => rsx! {},
        }
    }

    #[cfg(not(target_os = "macos"))]
    {
        rsx! {}
    }
}
