//! Exact lookup view component

use super::match_list::MatchListView;
use crate::display_types::MatchCandidate;
use dioxus::prelude::*;

/// Displays exact lookup results (e.g., from DiscID lookup)
#[component]
pub fn ExactLookupView(
    is_loading: bool,
    exact_match_candidates: Vec<MatchCandidate>,
    selected_match_index: Option<usize>,
    on_select: EventHandler<usize>,
) -> Element {
    if is_loading {
        return rsx! {
            div { class: "bg-gray-800 rounded-lg shadow p-6 text-center",
                p { class: "text-gray-400", "Looking up release by DiscID..." }
            }
        };
    }

    if exact_match_candidates.is_empty() {
        return rsx! {};
    }

    rsx! {
        div { class: "bg-gray-900 rounded-lg shadow p-6",
            h3 { class: "text-lg font-semibold text-white mb-4", "Multiple Exact Matches Found" }
            p { class: "text-sm text-gray-400 mb-4", "Select the correct release:" }
            div { class: "mt-4",
                MatchListView {
                    candidates: exact_match_candidates,
                    selected_index: selected_match_index,
                    on_select: move |index| on_select.call(index),
                }
            }
        }
    }
}
