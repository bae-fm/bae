//! What a run publishes, in the shape a stored verdict has: while its
//! lookups are out, the ledger as it stands and what the answers so far
//! combine to; once it has answered, the verdict it reached. A run's rows
//! hold it, and every surface draws a run in flight from them, beside the
//! text it judges its rows against.

use super::{fold, run_view, IdentifyRunView, IdentifyStateView};
use crate::agreements::CandidateText;
use crate::combine::{combine_results, Findings, LookupAnswers};
use crate::signals::Signals;
use crate::state::{IdentifyState, PressingLookup};
use crate::{CatalogStepView, TerminalVerdict, VerdictSummary};

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

/// What a run published: how it stands and the signals it last read, none
/// before extraction has reported.
#[derive(Debug, Clone, PartialEq)]
pub struct PublishedRun {
    pub outcome: RunOutcome,
    pub signals: Option<Signals>,
}

impl PublishedRun {
    /// The text the run's rows are judged against, read off its signals.
    ///
    /// The numbers the person struck out are the ones the ledger leaves out:
    /// a struck-out number counts against a row only where the folder's text
    /// prints it, and the ledger lays out every struck-out number the text
    /// prints as a row left out.
    pub fn text(&self) -> CandidateText {
        let struck_out: Vec<String> = match self.outcome.ledger().map(|ledger| &ledger.catalog) {
            Some(CatalogStepView::Numbers { rows, .. }) => {
                rows.iter().filter(|row| row.excluded).map(|row| row.value.clone()).collect()
            }
            Some(CatalogStepView::NoneFound) | None => Vec::new(),
        };
        self.signals.as_ref().map_or_else(CandidateText::default, |signals| {
            CandidateText::of(&signals.text_pool, &struck_out, signals.barcode.codes())
        })
    }

    /// The sample rate that rules a CD out, read off the run's signals.
    pub fn rate_ruling_out_cd(&self) -> Option<u32> {
        self.signals.as_ref().and_then(|signals| signals.origin.not_cd_rate)
    }

    /// The run as every surface draws it.
    pub fn view(&self) -> IdentifyStateView {
        IdentifyStateView::of_outcome(self.outcome.clone(), &self.text())
    }
}

impl IdentifyStateView {
    /// `outcome` as a surface renders it, its rows judged against `text`.
    pub fn of_outcome(outcome: RunOutcome, text: &CandidateText) -> Self {
        match outcome {
            RunOutcome::Running { ledger, findings, track_count } => {
                let folded = fold(findings, Some(&ledger), text, track_count);
                IdentifyStateView::Triangulating {
                    run: ledger,
                    groups: folded.groups,
                    agreements: folded.agreements,
                    narrowed_out_count: folded.narrowed_out_count,
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
                let folded = fold(findings, ledger.as_ref(), text, track_count);
                IdentifyStateView::Found {
                    run: ledger,
                    groups: folded.groups,
                    track_count,
                    agreements: folded.agreements,
                    narrowed_out_count: folded.narrowed_out_count,
                    folder_check: summary.folder_check(),
                    unattended_pick,
                    offers_shared_album: folded.offers_shared_album,
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
                findings,
                track_count,
                ledger,
            }) => {
                let folded = fold(findings, ledger.as_ref(), text, track_count);
                let failure_lines = failures
                    .into_iter()
                    .filter(|failure| !ledger.as_ref().is_some_and(|run| run.shows(failure)))
                    .collect();
                IdentifyStateView::Failed {
                    run: ledger,
                    failure_lines,
                    groups: folded.groups,
                    agreements: folded.agreements,
                    narrowed_out_count: folded.narrowed_out_count,
                    offers_shared_album: folded.offers_shared_album,
                }
            }
        }
    }
}

#[cfg(test)]
#[path = "outcome_tests.rs"]
mod tests;
