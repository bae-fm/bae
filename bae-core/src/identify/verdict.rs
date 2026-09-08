//! The identify pipeline's terminal outcome — what
//! [`crate::db::DbImportCandidateState`] persists, as one
//! `import_candidate_verdict` row and the `import_candidate_match` rows that
//! hang off it.
//!
//! [`IdentifyState`] is the reducer's own working shape: it carries a full
//! [`SignalsContext`] (raw signal inputs, the user's exclusions) through every
//! state so each landing answer re-combines without re-fetching, and it has
//! `Idle` and `Triangulating` variants that are mid-flight, not a verdict at
//! all. None of that belongs on disk. [`TerminalVerdict`] is the shape that
//! does: only the four states identification can actually end on, holding only
//! what the candidate's next launch needs back.
//!
//! `LibraryStatus` is deliberately absent from every variant. It is mutable
//! local state — another import landing can flip it — so storing it would
//! freeze a snapshot nothing invalidates. A reader re-checks it live, against
//! the release ids named here, rather than trusting a stored copy.
//!
//! The reducer makes partial evidence unrepresentable as a successful terminal
//! state: any active lookup failure produces `IdentifyState::Failed`. Only
//! `Idle` and `Triangulating` have no terminal verdict.

use super::combine::{NarrowedOut, ResultProvenance};
use super::state::{
    BarcodeEvidence, CatalogEvidence, ChosenCatalog, DiscIdEvidence, IdentifyState, RecordedWalk,
    SignalsContext, WalkEnd,
};
use crate::db::LibraryStatus;
use crate::import::search::{MetadataResult, SourceFailure};
use crate::import::{LookupChoices, MetadataSource};
use crate::signals::{ArtworkScan, BarcodeSignal, DiscIdSignal, LookupFailure, Signals};

/// Which lookup failed, and — where several providers answer it — which
/// provider. The disc-ID endpoint is MusicBrainz's alone, and release details
/// are fetched from the source that named the release, so those two name no
/// provider; the barcode and catalog lookups ask every configured provider
/// independently, so one failing is a fact about that provider.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum IdentifyFailure {
    DiscId(LookupFailure),
    /// Reading the candidate's barcodes failed, so no provider was asked. Not
    /// a provider's failure, which is why it names none.
    BarcodeScan(LookupFailure),
    Barcode(SourceFailure),
    Catalog(SourceFailure),
    ReleaseDetails(LookupFailure),
}

impl IdentifyFailure {
    /// Whether this failure is evidence the run asked `source`.
    ///
    /// A per-provider step names the provider that could not answer. A disc-ID
    /// lookup names none because it has only one to name — the source whose
    /// identifier a disc ID is — and that source was asked, or there would
    /// have been nothing to fail. Reading the candidate's barcodes asks
    /// nobody, and release details are fetched from whichever source already
    /// named the release, so neither says anything about who the run asked.
    fn asked(&self, source: MetadataSource) -> bool {
        match self {
            Self::Barcode(failure) | Self::Catalog(failure) => failure.source == source,
            Self::DiscId(_) => source == MetadataSource::DISC_ID_SOURCE,
            Self::BarcodeScan(_) | Self::ReleaseDetails(_) => false,
        }
    }
}

/// The identify pipeline's outcome once it can no longer change without new
/// input from the user or a re-run. Built from [`IdentifyState`]'s four
/// terminal variants (`Found`, `NotFoundAnywhere`, `ManualOnly`, `Failed`);
/// `Idle` and `Triangulating` have no terminal verdict, hence the fallible
/// conversion below.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum TerminalVerdict {
    /// One or more results — what `combine` produced, and what the sidebar and
    /// the Ready rule both work from directly.
    Found {
        matches: Vec<MetadataResult>,
        track_count: u32,
        /// Index-aligned with `matches`: which signal(s) produced or confirmed
        /// each one, for the sidebar's "matched on disc ID / barcode / text"
        /// evidence line.
        provenance: Vec<ResultProvenance>,
        /// Which of the candidate's barcodes the lookup that produced the
        /// barcode matches ran against. `None` when no barcode matched.
        ///
        /// The one lookup input the verdict keeps. The rest are recomputable
        /// facts a re-run re-extracts, but this one names *which* of several
        /// stored barcodes is the one that found the release — and the stored
        /// barcode rows carry the file each was read off, so this pointer is
        /// what puts a chip on that image and no other.
        matched_barcode: Option<String>,
        /// The releases the signals' agreement left out of `matches` — real
        /// answers from real lookups that the intersection discarded. Kept so
        /// a resumed candidate can still offer them; empty when the agreement
        /// narrowed nothing.
        narrowed_out: Vec<MetadataResult>,
        /// Index-aligned with `narrowed_out`, as `provenance` is with
        /// `matches`.
        narrowed_out_provenance: Vec<ResultProvenance>,
    },
    /// Both signals ran and settled on zero results. Distinct from a transport
    /// failure — nothing about this candidate's row goes unwritten; "we looked
    /// everywhere and there is nothing" is itself the answer.
    NotFoundAnywhere,
    /// Nothing to look up at all: no disc-ID artifact and no barcode source.
    /// Distinct from `NotFoundAnywhere` — there, signals ran and matched
    /// nothing; here, none ran, so a reader offers manual search rather than
    /// claiming a lookup that never happened.
    ManualOnly { track_count: u32 },
    /// At least one provider step failed, so partial evidence must not be
    /// classified as a complete answer.
    Failed {
        failures: Vec<IdentifyFailure>,
        track_count: u32,
    },
}

impl TryFrom<IdentifyState> for TerminalVerdict {
    /// The state handed back unchanged when it isn't terminal yet (`Idle` or
    /// `Triangulating`).
    type Error = IdentifyState;

    fn try_from(state: IdentifyState) -> Result<Self, Self::Error> {
        match state {
            IdentifyState::Found {
                matches,
                track_count,
                provenance,
                narrowed_out,
                // A live per-release check at read time, not a stored copy —
                // see the module doc.
                library_statuses: _,
                context,
            } => Ok(Self::Found {
                matches,
                track_count,
                provenance,
                matched_barcode: context.barcode.matched,
                narrowed_out: narrowed_out.matches,
                narrowed_out_provenance: narrowed_out.provenance,
            }),

            IdentifyState::NotFoundAnywhere { context: _ } => Ok(Self::NotFoundAnywhere),

            IdentifyState::ManualOnly {
                track_count,
                context: _,
            } => Ok(Self::ManualOnly { track_count }),

            // The partial matches a failed state carries are live evidence of
            // what the other source found, not a stored answer: the failure is
            // what the next launch has to know, and re-running is what turns
            // partial evidence into a verdict.
            IdentifyState::Failed {
                failures,
                track_count,
                matches: _,
                library_statuses: _,
                provenance: _,
                narrowed_out: _,
                context: _,
            } => Ok(Self::Failed {
                failures,
                track_count,
            }),

            other @ (IdentifyState::Idle | IdentifyState::Triangulating { .. }) => Err(other),
        }
    }
}

impl TerminalVerdict {
    /// Every release this verdict names — what a resumer checks live library
    /// status for before standing the state back up.
    pub fn named_releases(&self) -> Vec<&MetadataResult> {
        match self {
            // The narrowed-out releases are named too: a surface offers them
            // beside the matches, so their library status is checked with the
            // matches' rather than left unanswered.
            Self::Found {
                matches,
                narrowed_out,
                ..
            } => matches.iter().chain(narrowed_out).collect(),
            Self::NotFoundAnywhere | Self::ManualOnly { .. } | Self::Failed { .. } => Vec::new(),
        }
    }

    /// The identify state this stored verdict stands back up as — what opening
    /// an answered candidate shows without running anything.
    ///
    /// A settled run is two halves, stored side by side in one write:
    /// `signals` is what extraction read off the folder — the disc ID and the
    /// file it came from, every barcode sighting, every catalog number — and
    /// the verdict is what the providers answered about them. Put back
    /// together they are the run as it settled, so a resumed candidate lays
    /// the same ledger out as the live run did, and a cell that failed offers
    /// its retry.
    ///
    /// `choices` is what the person decided the candidate's identification
    /// asks about — the same value the run that wrote this verdict read at its
    /// start — so a resumed pane shows the signals it left out as left out and
    /// the numbers it chose as chosen.
    ///
    /// One thing no write keeps, so no resume has it: a failed run's partial
    /// matches. The failure is what stores, and re-running is what turns
    /// partial evidence into an answer.
    ///
    /// `signals` is `None` for a candidate whose signals are not stored: then
    /// there are no inputs, so there is no ledger — the matches, the barcode
    /// that found them, and nothing else.
    ///
    /// `status_of` is the live library check for a release id the verdict
    /// names, never a stored copy (see the module doc).
    pub fn resume_state(
        self,
        signals: Option<&Signals>,
        choices: &LookupChoices,
        status_of: &impl Fn(&MetadataResult) -> LibraryStatus,
    ) -> IdentifyState {
        let providers = self.resumed_providers();
        match self {
            Self::Found {
                matches,
                track_count,
                provenance,
                matched_barcode,
                narrowed_out,
                narrowed_out_provenance,
            } => {
                let library_statuses: Vec<LibraryStatus> = matches.iter().map(status_of).collect();
                let narrowed_out = NarrowedOut {
                    library_statuses: narrowed_out.iter().map(status_of).collect(),
                    matches: narrowed_out,
                    provenance: narrowed_out_provenance,
                };
                let context = match signals {
                    Some(signals) => found_run(
                        stored_inputs(signals, providers, choices),
                        &matches,
                        &library_statuses,
                        &provenance,
                        matched_barcode,
                    ),
                    None => SignalsContext {
                        barcode: BarcodeEvidence {
                            matched: matched_barcode,
                            ..Default::default()
                        },
                        ..no_inputs(track_count)
                    },
                };
                IdentifyState::Found {
                    matches,
                    library_statuses,
                    track_count,
                    provenance,
                    narrowed_out,
                    context,
                }
            }
            Self::NotFoundAnywhere => IdentifyState::NotFoundAnywhere {
                context: match signals {
                    Some(signals) => exhausted_run(stored_inputs(signals, providers, choices)),
                    None => no_inputs(0),
                },
            },
            Self::ManualOnly { track_count } => IdentifyState::ManualOnly {
                track_count,
                // Nothing ran, so there is nothing to record onto the inputs:
                // they are the whole run. Catalog numbers among them come back
                // as tiles to choose.
                context: match signals {
                    Some(signals) => stored_inputs(signals, providers, choices),
                    None => no_inputs(track_count),
                },
            },
            // A stored failure resumes with no matches: what one source found
            // before the other failed was never stored, so a resumed failure
            // offers the re-run rather than a partial list.
            Self::Failed {
                failures,
                track_count,
            } => IdentifyState::Failed {
                context: match signals {
                    Some(signals) => {
                        failed_run(stored_inputs(signals, providers, choices), &failures)
                    }
                    None => no_inputs(track_count),
                },
                failures,
                track_count,
                matches: Vec::new(),
                library_statuses: Vec::new(),
                provenance: Vec::new(),
                narrowed_out: NarrowedOut::default(),
            },
        }
    }

    /// The providers a resumed run lays its columns out for: the sources the
    /// verdict names, in source order.
    ///
    /// The run's own list is not stored, and no source is the one that always
    /// answers — a run asks whatever the library had switched on at the time —
    /// so the verdict is all that is left of it. A source it names was asked,
    /// as the source of a match or of a failure. A source that was asked and
    /// said nothing resumes as an absent column, and a verdict that names none
    /// resumes with no columns at all: that is as much as the stored answer
    /// says, and inventing a column would claim a source was asked when the
    /// stored row cannot say it was.
    fn resumed_providers(&self) -> Vec<MetadataSource> {
        MetadataSource::ALL
            .into_iter()
            .filter(|source| self.names_source(*source))
            .collect()
    }

    /// Whether the stored answer is evidence that this run asked `source`: it
    /// named a match, or it named a failure only that source could produce.
    fn names_source(&self, source: MetadataSource) -> bool {
        match self {
            Self::Found { matches, .. } => matches.iter().any(|result| result.source == source),
            Self::Failed { failures, .. } => failures.iter().any(|failure| failure.asked(source)),
            Self::NotFoundAnywhere | Self::ManualOnly { .. } => false,
        }
    }
}

/// What extraction read and what the person decided the run asks about, as the
/// run had them. Nothing has been asked yet — the verdict's own answers are
/// recorded onto this below.
///
/// A chosen number the stored signals no longer offer is dropped: a choice has
/// to be one of the values on the list, the same rule a live run applies to
/// each new snapshot.
fn stored_inputs(
    signals: &Signals,
    providers: Vec<MetadataSource>,
    choices: &LookupChoices,
) -> SignalsContext {
    let numbers = signals.text.catalogs().to_vec();
    SignalsContext {
        providers,
        // No artwork pass runs behind a stored verdict, so no row waits on one.
        artwork: ArtworkScan::Absent,
        disc: DiscIdEvidence {
            signal: signals.disc_id.clone(),
            excluded: choices.disc_id_excluded,
            ..Default::default()
        },
        barcode: BarcodeEvidence {
            codes: signals.barcode.codes().to_vec(),
            had_source: !matches!(signals.barcode, BarcodeSignal::Absent),
            excluded: choices.barcode_excluded,
            ..Default::default()
        },
        catalog: CatalogEvidence {
            chosen: choices
                .chosen_catalogs
                .iter()
                .filter(|value| numbers.iter().any(|number| &&number.value == value))
                .cloned()
                .map(ChosenCatalog::new)
                .collect(),
            numbers,
        },
        track_count: signals.disc_id.track_count(),
    }
}

/// The context a verdict resumes with when its candidate's signals are not
/// stored. `disc_id: Absent` here means "not retained", not "the folder had no
/// disc artifact" — the distinction never leaves core: with no inputs at all
/// there is no ledger to lay out and no toolbar to draw, so the person's
/// choices have nothing to be about either.
fn no_inputs(track_count: u32) -> SignalsContext {
    SignalsContext {
        providers: Vec::new(),
        artwork: ArtworkScan::Absent,
        disc: DiscIdEvidence {
            signal: DiscIdSignal::Absent { track_count },
            ..Default::default()
        },
        barcode: BarcodeEvidence::default(),
        catalog: CatalogEvidence::default(),
        track_count,
    }
}

/// What a `Found` verdict's providers answered: each match lands on the signal
/// its provenance names, and every provider's walk through the codes ended
/// where its match says — on the code the verdict points at, or having tried
/// them all.
fn found_run(
    mut context: SignalsContext,
    matches: &[MetadataResult],
    library_statuses: &[LibraryStatus],
    provenance: &[ResultProvenance],
    matched_barcode: Option<String>,
) -> SignalsContext {
    let found_by = |names: fn(&ResultProvenance) -> bool| {
        matches
            .iter()
            .zip(provenance)
            .zip(library_statuses)
            .filter(|((_, provenance), _)| names(provenance))
            .map(|((result, _), status)| (result.clone(), status.clone()))
            .collect::<Vec<_>>()
    };
    context.disc.results = found_by(|provenance| provenance.by_disc_id);
    context.barcode.results = found_by(|provenance| provenance.by_barcode);
    // Every match the run found by catalog, on each number it was looking up:
    // a verdict records that the catalog produced a match, not which of the
    // chosen numbers did.
    let by_catalog = found_by(|provenance| provenance.by_catalog);
    for chosen in &mut context.catalog.chosen {
        chosen.results = by_catalog.clone();
    }

    // A walk that matched names one of the codes it asked about, and the codes
    // and the verdict are written together, so the code the verdict points at
    // is one of them.
    let codes = context.barcode.code_values();
    let matched = matched_barcode.filter(|code| codes.contains(code));
    let answered = |source| {
        context
            .barcode
            .results
            .iter()
            .any(|(result, _)| result.source == source)
    };
    let walks = context
        .providers
        .iter()
        .map(|&source| RecordedWalk {
            source,
            end: match &matched {
                Some(code) if answered(source) => WalkEnd::Matched { code: code.clone() },
                _ => WalkEnd::Exhausted,
            },
        })
        .collect();
    context.barcode.walks = walks;
    context.barcode.matched = matched;
    context
}

/// A run that asked everything it had and matched nothing: every provider's
/// walk ran out of codes.
fn exhausted_run(mut context: SignalsContext) -> SignalsContext {
    let walks = context
        .providers
        .iter()
        .map(|&source| RecordedWalk {
            source,
            end: WalkEnd::Exhausted,
        })
        .collect();
    context.barcode.walks = walks;
    context
}

/// What a `Failed` verdict says of each step: the disc-ID lookup's failure,
/// the failure that stopped the barcodes being read at all, and every provider
/// that could not answer about a barcode. Which code a failed walk stopped on
/// is not stored, and every walk starts at the first code, so that is where
/// the ledger puts the warning. The providers that did answer show no matches:
/// a failed verdict stores none.
fn failed_run(mut context: SignalsContext, failures: &[IdentifyFailure]) -> SignalsContext {
    context.disc.failure = failures.iter().find_map(|failure| match failure {
        IdentifyFailure::DiscId(failure) => Some(failure.clone()),
        _ => None,
    });
    context.barcode.scan_failure = failures.iter().find_map(|failure| match failure {
        IdentifyFailure::BarcodeScan(failure) => Some(failure.clone()),
        _ => None,
    });
    context.barcode.failures = failures
        .iter()
        .filter_map(|failure| match failure {
            IdentifyFailure::Barcode(failure) => Some(failure.clone()),
            _ => None,
        })
        .collect();
    let first_code = context.barcode.code_values().into_iter().next();
    let walks = match &first_code {
        Some(first) => context
            .providers
            .iter()
            .map(|&source| RecordedWalk {
                source,
                end: if context
                    .barcode
                    .failures
                    .iter()
                    .any(|failure| failure.source == source)
                {
                    WalkEnd::Failed {
                        code: first.clone(),
                    }
                } else {
                    WalkEnd::Exhausted
                },
            })
            .collect(),
        // No code to walk, so no walk ran: the barcode pipe stands back up
        // from the codes alone.
        None => Vec::new(),
    };
    context.barcode.walks = walks;
    context
}

#[cfg(test)]
#[path = "verdict_tests.rs"]
mod tests;
