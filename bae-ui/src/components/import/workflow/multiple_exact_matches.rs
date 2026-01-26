//! Multiple DiscID matches view component

use super::match_list::MatchListView;
use super::DiscIdPill;
use crate::components::{Button, ButtonSize, ButtonVariant};
use crate::display_types::IdentifyMode;
use crate::stores::import::ImportState;
use dioxus::prelude::*;

/// Displays multiple DiscID matches for user to pick from
///
/// Accepts `ReadStore<ImportState>` - reads at leaf level for granular reactivity.
#[component]
pub fn MultipleExactMatchesView(
    state: ReadStore<ImportState>,
    on_select: EventHandler<usize>,
    on_confirm: EventHandler<()>,
) -> Element {
    // Read state at leaf - these are computed values
    let st = state.read();
    let candidates = st.get_exact_match_candidates();
    let selected_index = st.get_selected_match_index();
    // Extract disc_id from the mode - it's carried in MultipleExactMatches(disc_id)
    let disc_id = match st.get_identify_mode() {
        IdentifyMode::MultipleExactMatches(id) => Some(id),
        _ => None,
    };
    drop(st);

    if candidates.is_empty() {
        return rsx! {};
    }

    rsx! {
        div { class: "p-4 max-w-xl",
            // Header
            p { class: "text-white mb-1", "Multiple Disc ID matches" }

            // Disc ID pill on its own line
            if let Some(id) = disc_id {
                p { class: "text-sm text-gray-400 mb-4",
                    "Disc ID: "
                    DiscIdPill { disc_id: id }
                }
            }

            MatchListView {
                candidates,
                selected_index,
                on_select: move |index| on_select.call(index),
            }

            // Continue button - only shown when something is selected
            if selected_index.is_some() {
                div { class: "flex justify-end mt-4",
                    Button {
                        variant: ButtonVariant::Primary,
                        size: ButtonSize::Medium,
                        onclick: move |_| on_confirm.call(()),
                        "Continue"
                    }
                }
            }
        }
    }
}
