//! Exact lookup wrapper - reads signals and delegates to ExactLookupView

use crate::import::MatchCandidate;
use crate::ui::components::import::workflow::shared::confirmation::to_display_candidate;
use bae_ui::components::import::ExactLookupView;
use dioxus::prelude::*;

#[component]
pub fn ExactLookup(
    is_looking_up: ReadSignal<bool>,
    exact_match_candidates: ReadSignal<Vec<MatchCandidate>>,
    selected_match_index: ReadSignal<Option<usize>>,
    on_select: EventHandler<usize>,
) -> Element {
    let display_candidates: Vec<bae_ui::display_types::MatchCandidate> = exact_match_candidates
        .read()
        .iter()
        .map(to_display_candidate)
        .collect();

    rsx! {
        ExactLookupView {
            is_looking_up: *is_looking_up.read(),
            exact_match_candidates: display_candidates,
            selected_match_index: *selected_match_index.read(),
            on_select: move |index| on_select.call(index),
        }
    }
}
