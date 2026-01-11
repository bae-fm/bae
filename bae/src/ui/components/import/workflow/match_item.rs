//! Match item wrapper - delegates to MatchItemView

use crate::import::MatchCandidate;
use crate::ui::components::import::workflow::shared::confirmation::to_display_candidate;
use bae_ui::components::import::MatchItemView;
use dioxus::prelude::*;

#[component]
pub fn MatchItem(
    candidate: MatchCandidate,
    is_selected: bool,
    on_select: EventHandler<()>,
) -> Element {
    let display_candidate = to_display_candidate(&candidate);

    rsx! {
        MatchItemView {
            candidate: display_candidate,
            is_selected,
            on_select: move |_| on_select.call(()),
        }
    }
}
