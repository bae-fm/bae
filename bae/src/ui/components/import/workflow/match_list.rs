//! Match list wrapper - delegates to MatchListView

use crate::import::MatchCandidate;
use crate::ui::components::import::workflow::shared::confirmation::to_display_candidate;
use bae_ui::components::import::MatchListView;
use dioxus::prelude::*;

#[component]
pub fn MatchList(
    candidates: Vec<MatchCandidate>,
    selected_index: Option<usize>,
    on_select: EventHandler<usize>,
) -> Element {
    let display_candidates: Vec<bae_ui::display_types::MatchCandidate> =
        candidates.iter().map(to_display_candidate).collect();

    rsx! {
        MatchListView {
            candidates: display_candidates,
            selected_index,
            on_select: move |index| on_select.call(index),
        }
    }
}
