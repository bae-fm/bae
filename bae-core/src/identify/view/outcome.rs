//! What a run publishes, in the shape a stored verdict has: while its
//! lookups are out, the ledger as it stands and what the answers so far
//! combine to; once it has answered, the verdict it reached. A run's rows
//! hold it, and every surface draws a run in flight from them, beside the
//! text it judges its rows against.

use super::{fold, run_view, IdentifyRunView, IdentifyStateView};
use crate::identify::agreements::CandidateText;
use crate::identify::combine::{combine_results, Findings, LookupAnswers};
use crate::identify::state::{IdentifyState, PressingLookup};
use crate::identify::toolbar::{SignalKind, SignalOption, SignalState, ToolbarSignal};
use crate::identify::{
    BarcodeStepView, CatalogStepView, DiscIdStepView, IsrcStepView, LookupView, NotAskedReason,
    SignalValueRow, TerminalVerdict, VerdictSummary,
};
use crate::signals::Signals;

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
    pub(crate) fn of(state: &IdentifyState) -> Option<Self> {
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
                let folded = fold(findings, ledger.as_ref(), text, track_count);
                IdentifyStateView::Found {
                    run: ledger,
                    groups: folded.groups,
                    track_count,
                    agreements: folded.agreements,
                    narrowed_out_count: folded.narrowed_out_count,
                    folder_check: summary.folder_check(),
                    picks_unattended: summary.picks_unattended(),
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

impl PublishedRun {
    /// The disc-ID, barcode, catalog and ISRC badges, read off the ledger
    /// the run shows; none for an outcome that lays no ledger out.
    pub fn toolbar(&self) -> Vec<ToolbarSignal> {
        self.outcome.ledger().map(IdentifyRunView::toolbar).unwrap_or_default()
    }
}

/// The badge state of `lookups` together: under way while one is, else what
/// they found, else the first failure, else why nobody was asked, else no
/// match. `None` when there are no lookups.
fn lookups_state<'a>(lookups: impl IntoIterator<Item = &'a LookupView>) -> Option<SignalState> {
    let lookups: Vec<&LookupView> = lookups.into_iter().collect();
    if lookups.is_empty() {
        return None;
    }
    if lookups.iter().any(|lookup| matches!(lookup, LookupView::Queued | LookupView::LookingUp)) {
        return Some(SignalState::LookingUp);
    }
    let found: std::collections::HashSet<(bae_catalogs::catalog::Catalog, &str)> = lookups
        .iter()
        .flat_map(|lookup| lookup.releases())
        .map(|release| (release.source, release.release_id.as_str()))
        .collect();
    if !found.is_empty() {
        return Some(SignalState::Found { count: found.len() as u32 });
    }
    if let Some(failure) = lookups.iter().find_map(|lookup| match lookup {
        LookupView::Failed { failure } => Some(failure.clone()),
        _ => None,
    }) {
        return Some(SignalState::Failed { failure });
    }
    let not_asked: Vec<NotAskedReason> = lookups
        .iter()
        .filter_map(|lookup| match lookup {
            LookupView::NotAsked { reason } => Some(*reason),
            _ => None,
        })
        .collect();
    if not_asked.len() == lookups.len() {
        return Some(SignalState::NotAsked { reason: not_asked[0] });
    }
    Some(SignalState::NoMatch)
}

impl IdentifyRunView {
    /// The disc-ID, barcode, catalog and ISRC badges this ledger shows.
    pub fn toolbar(&self) -> Vec<ToolbarSignal> {
        let option = |value: &str, chosen: bool| SignalOption { value: value.to_string(), chosen };
        let disc = match &self.disc_id {
            DiscIdStepView::Read { discs } => {
                let excluded = discs.iter().all(|disc| {
                    matches!(disc.lookup, LookupView::NotAsked { reason: NotAskedReason::LeftOut })
                });
                ToolbarSignal {
                    kind: SignalKind::DiscId,
                    shown: discs.first().map(|disc| disc.disc_id.clone()),
                    state: lookups_state(discs.iter().map(|disc| &disc.lookup))
                        .unwrap_or(SignalState::Skipped),
                    excluded,
                    options: discs.iter().map(|disc| option(&disc.disc_id, !excluded)).collect(),
                }
            }
            DiscIdStepView::Reading => badge(SignalKind::DiscId, SignalState::LookingUp),
            DiscIdStepView::Absent | DiscIdStepView::NotCdAudio => {
                badge(SignalKind::DiscId, SignalState::Skipped)
            }
        };
        fn cells(row: &SignalValueRow) -> Vec<&LookupView> {
            row.cells.iter().map(|cell| &cell.lookup).collect()
        }
        let barcode = match &self.barcode {
            BarcodeStepView::Absent => badge(SignalKind::Barcode, SignalState::Skipped),
            BarcodeStepView::NoCodes => badge(SignalKind::Barcode, SignalState::NoMatch),
            BarcodeStepView::Rows { scanning, rows } => {
                let lookups: Vec<&LookupView> = rows.iter().flat_map(cells).collect();
                let matched = rows
                    .iter()
                    .find(|row| row.cells.iter().any(|cell| !cell.lookup.releases().is_empty()));
                ToolbarSignal {
                    kind: SignalKind::Barcode,
                    shown: matched.or(rows.first()).map(|row| row.value.clone()),
                    state: if *scanning {
                        SignalState::LookingUp
                    } else {
                        lookups_state(lookups).unwrap_or(SignalState::NoMatch)
                    },
                    excluded: !rows.is_empty() && rows.iter().all(|row| row.excluded),
                    options: rows.iter().map(|row| option(&row.value, !row.excluded)).collect(),
                }
            }
        };
        let catalog = match &self.catalog {
            CatalogStepView::NoneFound => badge(SignalKind::Catalog, SignalState::Skipped),
            CatalogStepView::Numbers { scanning, rows, candidates } => {
                let in_effect: Vec<&SignalValueRow> =
                    rows.iter().filter(|row| !row.excluded).collect();
                let lookups: Vec<&LookupView> =
                    in_effect.iter().flat_map(|row| cells(row)).collect();
                ToolbarSignal {
                    kind: SignalKind::Catalog,
                    shown: in_effect.first().map(|row| row.value.clone()),
                    state: match lookups_state(lookups) {
                        Some(state) => state,
                        None if *scanning => SignalState::LookingUp,
                        None => SignalState::Skipped,
                    },
                    excluded: false,
                    options: rows
                        .iter()
                        .map(|row| option(&row.value, !row.excluded))
                        .chain(candidates.iter().map(|candidate| option(&candidate.value, false)))
                        .collect(),
                }
            }
        };
        let isrc = match &self.isrc {
            IsrcStepView::Reading => badge(SignalKind::Isrc, SignalState::LookingUp),
            IsrcStepView::Absent => badge(SignalKind::Isrc, SignalState::Skipped),
            IsrcStepView::Read { isrcs, lookup } => ToolbarSignal {
                kind: SignalKind::Isrc,
                shown: isrcs.first().cloned(),
                state: lookups_state([lookup]).unwrap_or(SignalState::Skipped),
                excluded: false,
                options: isrcs.iter().map(|code| option(code, true)).collect(),
            },
        };
        vec![disc, barcode, catalog, isrc]
    }
}

/// A badge with no value.
fn badge(kind: SignalKind, state: SignalState) -> ToolbarSignal {
    ToolbarSignal { kind, shown: None, state, excluded: false, options: Vec::new() }
}
