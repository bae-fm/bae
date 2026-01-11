use super::state::ImportContext;
use super::types::ImportPhase;
use crate::ui::components::import::ImportSource;
use bae_core::import::MatchCandidate;
use dioxus::prelude::*;
use std::rc::Rc;
/// Check if there is unclean state for the current import source
/// Returns true if switching tabs would lose progress
fn has_unclean_state(ctx: &ImportContext) -> bool {
    let current_source = *ctx.selected_import_source().read();
    match current_source {
        ImportSource::Folder => !ctx.folder_path().read().is_empty(),
        ImportSource::Torrent => {
            ctx.torrent_source().read().is_some() || !ctx.magnet_link().read().is_empty()
        }
        ImportSource::Cd => {
            !ctx.folder_path().read().is_empty() || ctx.cd_toc_info().read().is_some()
        }
    }
}
/// Try to switch import source, showing dialog if there's unclean state
pub fn try_switch_import_source(ctx: &Rc<ImportContext>, source: ImportSource) {
    if *ctx.selected_import_source().read() == source {
        return;
    }
    if has_unclean_state(ctx) {
        let ctx_for_callback = Rc::clone(ctx);
        ctx.dialog.show_with_callback(
            "Watch out!".to_string(),
            "You have unsaved work. Navigating away will discard your current progress."
                .to_string(),
            "Switch Tab".to_string(),
            "Cancel".to_string(),
            move || {
                ctx_for_callback.set_selected_import_source(source);
                ctx_for_callback.reset();
            },
        );
    } else {
        ctx.set_selected_import_source(source);
        ctx.reset();
    }
}
/// Select an exact match candidate by index and move to confirmation.
///
/// This transitions from ExactLookup phase to Confirmation phase.
pub fn select_exact_match(ctx: &ImportContext, index: usize) {
    ctx.set_selected_match_index(Some(index));
    if let Some(candidate) = ctx.exact_match_candidates().read().get(index).cloned() {
        ctx.set_confirmed_candidate(Some(candidate.clone()));
        ctx.set_import_phase(ImportPhase::Confirmation);
    }
}
/// Confirm a match candidate and move to confirmation phase.
///
/// This is used when confirming from manual search results.
pub fn confirm_candidate(ctx: &ImportContext, candidate: MatchCandidate) {
    ctx.set_confirmed_candidate(Some(candidate));
    ctx.set_import_phase(ImportPhase::Confirmation);
}
/// Reject the current confirmation and go back to previous phase.
///
/// This handles:
/// - Clearing confirmed candidate and selection
/// - Determining whether to go back to ExactLookup or ManualSearch
/// - Initializing search query from detected metadata if going to ManualSearch
pub fn reject_confirmation(ctx: &ImportContext) {
    ctx.set_confirmed_candidate(None);
    ctx.set_selected_match_index(None);
    if !ctx.exact_match_candidates().read().is_empty() {
        ctx.set_import_phase(ImportPhase::ExactLookup);
    } else {
        if let Some(metadata) = ctx.detected_metadata().read().as_ref() {
            ctx.init_search_query_from_metadata(metadata);
        }
        ctx.set_import_phase(ImportPhase::ManualSearch);
    }
}
