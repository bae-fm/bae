//! The cards of a verdict as the identify rules fold them in memory: the
//! reference the stored layout's statement is held to, and what tests of the
//! rules read the view's cards from. Nothing reads cards this way.

use super::{IdentifyRunView, IdentifyStateView, RowAgreements, RunOutcome, VerdictCards};
use crate::agreements::{judged_results, CandidateText};
use crate::combine::Findings;
use crate::release_group::{group_formed_rows, Judgements, Pressing};
use crate::row_facts::FolderFacts;
use crate::state::IdentifyState;
use crate::SharedAlbum;

/// Judge `findings` against `text` and fold them into cards, as the rules
/// do, with every row's badges. `ledger` is the run's, whose disc-ID lookups
/// say which releases agree on a disc ID.
pub fn reference_cards(
    findings: Findings,
    ledger: Option<&IdentifyRunView>,
    text: &CandidateText,
    track_count: u32,
) -> VerdictCards {
    let facts = FolderFacts::of(text, findings.releases());
    let by_disc_id = ledger.map(IdentifyRunView::disc_id_releases).unwrap_or_default();
    let Findings { matches, pressings, narrowed_out, medium_conflict: _, named_notes } = findings;
    let offered = judged_results(matches, &by_disc_id, text, &facts);
    let set_aside = judged_results(narrowed_out.matches, &by_disc_id, text, &facts);
    let judgements = Judgements::of(
        &offered.iter().chain(&set_aside).cloned().collect::<Vec<_>>(),
        Some(track_count),
    );
    let cards = group_formed_rows(
        offered,
        &pressings,
        set_aside,
        &narrowed_out.pressings,
        Some(track_count),
    );
    let named_notes = |pressing: &Pressing| {
        pressing.noted(|release| {
            named_notes
                .iter()
                .find(|named| named.release == release.release())
                .map(|named| named.note.clone())
        })
    };
    let agreements = cards
        .iter()
        .flat_map(|group| group.pressings().chain(group.narrowed_out()))
        .flat_map(|pressing| {
            let agreements = RowAgreements {
                fields: pressing.agreements(&judgements),
                notes: named_notes(pressing),
            };
            pressing
                .releases
                .iter()
                .map(move |release| (release.release_id.clone(), agreements.clone()))
        })
        .collect();
    let narrowed_out_count = cards.iter().map(|group| group.narrowed_out().count() as u32).sum();
    VerdictCards {
        offers_shared_album: SharedAlbum::of(&cards).is_some(),
        groups: cards,
        agreements,
        narrowed_out_count,
    }
}

/// The cards of `outcome` as the rules fold them against `text`.
pub fn reference_cards_of(outcome: &RunOutcome, text: &CandidateText) -> VerdictCards {
    use crate::TerminalVerdict;
    let (findings, ledger, track_count) = match outcome {
        RunOutcome::Running { ledger, findings, track_count } => {
            (findings, Some(ledger), *track_count)
        }
        RunOutcome::Answered(
            TerminalVerdict::Found { findings, track_count, ledger }
            | TerminalVerdict::Failed { findings, track_count, ledger, .. },
        ) => (findings, ledger.as_ref(), *track_count),
        RunOutcome::Answered(_) => return VerdictCards::default(),
    };
    reference_cards(findings.clone(), ledger, text, track_count)
}

impl From<IdentifyState> for IdentifyStateView {
    fn from(state: IdentifyState) -> Self {
        let text = state.candidate_text();
        match RunOutcome::of(&state) {
            Some(outcome) => {
                let cards = reference_cards_of(&outcome, &text);
                IdentifyStateView::of(outcome, cards)
            }
            None => IdentifyStateView::Idle,
        }
    }
}

/// A stored verdict as its candidate's pane draws it, its rows judged by the
/// rules against the text its `signals` and `choices` give.
pub fn reference_resume(
    verdict: crate::TerminalVerdict,
    signals: Option<&crate::signals::Signals>,
    choices: &crate::LookupChoices,
) -> IdentifyStateView {
    let outcome = RunOutcome::Answered(verdict);
    let cards = reference_cards_of(&outcome, &CandidateText::of_stored(signals, choices));
    IdentifyStateView::of(outcome, cards)
}
