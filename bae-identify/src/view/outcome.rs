//! What a run publishes, in the shape a stored verdict has: while its
//! lookups are out, the ledger as it stands and what the answers so far
//! combine to; once it has answered, the verdict it reached. A run's rows
//! hold it, and every surface draws a run in flight from them, beside the
//! text it judges its rows against.

use super::{run_view, IdentifyRunView, IdentifyStateView, VerdictCards};
use crate::combine::{combine_results, Findings, LookupAnswers};
use crate::signals::Signals;
use crate::state::{IdentifyState, PressingLookup};
use crate::{TerminalVerdict, VerdictSummary};

/// How a run stands, as its rows hold it.
#[derive(Debug, Clone, PartialEq)]
pub enum RunOutcome {
    /// Lookups in flight.
    Running {
        /// The ledger as it stands, each release in its cells as the
        /// answers so far return it, which is the copy `findings` holds.
        ledger: IdentifyRunView,
        /// What the answers so far combine to, the way the settle combines
        /// them.
        findings: Findings,
        /// How many tracks the folder plays.
        track_count: u32,
    },
    /// The answer the run reached.
    Answered(TerminalVerdict),
}

impl RunOutcome {
    /// How `state` stands; `None` for `Idle`, which is no run.
    pub fn of(state: &IdentifyState) -> Option<Self> {
        match state {
            IdentifyState::Idle => None,
            IdentifyState::Triangulating { discid, barcode, catalog, isrc, search, context } => {
                let answers = LookupAnswers {
                    disc_id: discid.answers(),
                    barcode: barcode.results(),
                    catalog: catalog.results(),
                    isrc: isrc.results(),
                    search: search.results(),
                    pressing: context.pressings.iter().flat_map(PressingLookup::results).collect(),
                };
                let ledger =
                    run_view(discid, barcode, catalog, isrc, search, context).read_as(&answers);
                Some(Self::Running {
                    ledger,
                    findings: combine_results(answers, &context.text, context.folder_audio()),
                    track_count: context.audio.track_count,
                })
            }
            settled => Some(Self::Answered(
                TerminalVerdict::try_from(settled.clone())
                    .expect("every state but idle and triangulating is a verdict"),
            )),
        }
    }

    /// Whether the run has reached its answer.
    pub fn is_answered(&self) -> bool {
        matches!(self, Self::Answered(_))
    }

    /// The ledger the run shows: as it stands, or as it recorded it.
    pub fn ledger(&self) -> Option<&IdentifyRunView> {
        match self {
            Self::Running { ledger, .. } => Some(ledger),
            Self::Answered(verdict) => verdict.ledger(),
        }
    }
}

/// What a run published: how it stands, the signals it last read (none
/// before extraction has reported), and the cards its rows stored.
#[derive(Debug, Clone, PartialEq)]
pub struct PublishedRun {
    pub outcome: RunOutcome,
    pub signals: Option<Signals>,
    pub cards: VerdictCards,
}

impl PublishedRun {
    /// The sample rate that rules a CD out, read off the run's signals.
    pub fn rate_ruling_out_cd(&self) -> Option<u32> {
        self.signals.as_ref().and_then(|signals| signals.origin.not_cd_rate)
    }

    /// The run as every surface draws it.
    pub fn view(&self) -> IdentifyStateView {
        IdentifyStateView::of(self.outcome.clone(), self.cards.clone())
    }
}

impl IdentifyStateView {
    /// `outcome` as a surface renders it, its cards as its rows stored them.
    pub fn of(outcome: RunOutcome, cards: VerdictCards) -> Self {
        match outcome {
            RunOutcome::Running { ledger, findings: _, track_count: _ } => {
                IdentifyStateView::Triangulating {
                    run: ledger,
                    groups: cards.groups,
                    agreements: cards.agreements,
                    narrowed_out_count: cards.narrowed_out_count,
                }
            }
            RunOutcome::Answered(TerminalVerdict::Found { findings, track_count, ledger }) => {
                let summary = VerdictSummary::of_found(&findings, track_count);
                // A verdict that picks unattended has one pressing, whose lead
                // is the findings' first match: the release the settle reads.
                let unattended_pick = summary
                    .picks_unattended()
                    .then(|| findings.matches.first().map(|lead| lead.release_id.clone()))
                    .flatten();
                IdentifyStateView::Found {
                    run: ledger,
                    groups: cards.groups,
                    track_count,
                    agreements: cards.agreements,
                    narrowed_out_count: cards.narrowed_out_count,
                    folder_check: summary.folder_check(),
                    unattended_pick,
                    offers_shared_album: cards.offers_shared_album,
                }
            }
            RunOutcome::Answered(TerminalVerdict::NotFoundAnywhere { ledger }) => {
                IdentifyStateView::NotFoundAnywhere { run: ledger }
            }
            RunOutcome::Answered(TerminalVerdict::ManualOnly { track_count, ledger }) => {
                IdentifyStateView::ManualOnly { track_count, run: ledger }
            }
            RunOutcome::Answered(TerminalVerdict::Error { failure }) => {
                IdentifyStateView::Error { failure }
            }
            RunOutcome::Answered(TerminalVerdict::Failed {
                failures,
                findings: _,
                track_count: _,
                ledger,
            }) => {
                let failure_lines = failures
                    .into_iter()
                    .filter(|failure| !ledger.as_ref().is_some_and(|run| run.shows(failure)))
                    .collect();
                IdentifyStateView::Failed {
                    run: ledger,
                    failure_lines,
                    groups: cards.groups,
                    agreements: cards.agreements,
                    narrowed_out_count: cards.narrowed_out_count,
                    offers_shared_album: cards.offers_shared_album,
                }
            }
        }
    }
}

#[cfg(test)]
#[path = "outcome_tests.rs"]
mod tests;
