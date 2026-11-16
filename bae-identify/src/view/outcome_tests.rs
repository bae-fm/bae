//! What the view of a settled run states from the cards its rows stored.

use crate::view::*;
use crate::{Findings, TerminalVerdict};

const DG: LookupCatalog = LookupCatalog::Discogs;

/// The view states the decision its cards stored; it does not judge the
/// findings again. A found verdict whose one release fits the folder shows
/// the check its cards state, and a pick of another release when they state
/// that.
#[test]
fn a_found_view_states_what_its_cards_stored_without_judging_its_findings() {
    let verdict = TerminalVerdict::Found {
        findings: Findings {
            matches: vec![MetadataResult {
                source_tracks: Some(bae_catalogs::search::SourceTracks::Listed { count: 13 }),
                ..MetadataResult::for_test(DG, "dg-1", Some("g"))
            }],
            pressings: vec![0],
            ..Findings::default()
        },
        track_count: 13,
        ledger: None,
    };
    for stored in [
        crate::FoundDecision::Declined(Some(crate::FolderCheck::SourceTracksUnknown)),
        crate::FoundDecision::Picks("stored-pick".to_string()),
    ] {
        let cards = VerdictCards { decision: Some(stored.clone()), ..VerdictCards::default() };
        let IdentifyStateView::Found { decision, .. } =
            IdentifyStateView::of(RunOutcome::Answered(verdict.clone()), cards)
        else {
            panic!("a found verdict");
        };
        assert_eq!(decision, stored);
    }
}
