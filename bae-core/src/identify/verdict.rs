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
//! What a run showed is stored with what it concluded: the `ledger` on every
//! variant is the run's own last frame, recorded once when the run ended.
//! Reading a candidate back draws that, so the pane after the save is the pane
//! during the run — nothing is rebuilt from the matches, which can only name
//! providers that produced one.
//!
//! `LibraryStatus` is deliberately absent from every variant. It is mutable
//! local state — another import landing can flip it — so storing it would
//! freeze a snapshot nothing invalidates. A reader re-checks it live, against
//! the release ids named here, rather than trusting a stored copy.
//!
//! The reducer makes partial evidence unrepresentable as a successful terminal
//! state: any active lookup failure produces `IdentifyState::Failed`. Only
//! `Idle` and `Triangulating` have no terminal verdict.

use super::agreements::CandidateText;
use super::combine::{LookupProvenance, NarrowedOut};
use super::state::{IdentifyState, SignalsContext};
use super::view::IdentifyRunView;
use crate::db::LibraryStatus;
use crate::import::search::{MetadataResult, SourceFailure};
use crate::signals::LookupFailure;

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

/// The identify pipeline's outcome once it can no longer change without new
/// input from the user or a re-run. Built from [`IdentifyState`]'s four
/// terminal variants (`Found`, `NotFoundAnywhere`, `ManualOnly`, `Failed`);
/// `Idle` and `Triangulating` have no terminal verdict, hence the fallible
/// conversion below.
///
/// Every variant carries the `ledger` its run recorded as it ended — the last
/// frame the run showed, laid out signal by signal and provider by provider.
/// It is stored with the verdict and shown as it stands: what a person saw
/// while the run went is what they see afterwards. `None` for a run
/// extraction handed nothing to lay out, and for a verdict whose stored row
/// records none.
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
        provenance: Vec<LookupProvenance>,
        /// The releases the signals' agreement left out of `matches` — real
        /// answers from real lookups that the intersection discarded. Kept so
        /// a resumed candidate can still offer them; empty when the agreement
        /// narrowed nothing.
        narrowed_out: Vec<MetadataResult>,
        /// Index-aligned with `narrowed_out`, as `provenance` is with
        /// `matches`.
        narrowed_out_provenance: Vec<LookupProvenance>,
        ledger: Option<IdentifyRunView>,
    },
    /// Both signals ran and settled on zero results. Distinct from a transport
    /// failure — nothing about this candidate's row goes unwritten; "we looked
    /// everywhere and there is nothing" is itself the answer.
    NotFoundAnywhere { ledger: Option<IdentifyRunView> },
    /// Nothing to look up at all: no disc-ID artifact and no barcode source.
    /// Distinct from `NotFoundAnywhere` — there, signals ran and matched
    /// nothing; here, none ran, so a reader offers manual search rather than
    /// claiming a lookup that never happened.
    ManualOnly {
        track_count: u32,
        ledger: Option<IdentifyRunView>,
    },
    /// At least one provider step failed, so partial evidence must not be
    /// classified as a complete answer.
    Failed {
        failures: Vec<IdentifyFailure>,
        track_count: u32,
        ledger: Option<IdentifyRunView>,
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
                ledger,
                // A live per-release check at read time, not a stored copy —
                // see the module doc.
                library_statuses: _,
                // The run's, and the candidate's text it judged against, which
                // is stored on its own and read back beside these matches.
                context: _,
            } => Ok(Self::Found {
                matches,
                track_count,
                provenance,
                narrowed_out: narrowed_out.matches,
                narrowed_out_provenance: narrowed_out.provenance,
                ledger,
            }),

            IdentifyState::NotFoundAnywhere { ledger, context: _ } => {
                Ok(Self::NotFoundAnywhere { ledger })
            }

            IdentifyState::ManualOnly {
                track_count,
                ledger,
                context: _,
            } => Ok(Self::ManualOnly {
                track_count,
                ledger,
            }),

            // The partial matches a failed state carries are live evidence of
            // what the other source found, not a stored answer: the failure is
            // what the next launch has to know, and re-running is what turns
            // partial evidence into a verdict.
            IdentifyState::Failed {
                failures,
                track_count,
                ledger,
                matches: _,
                library_statuses: _,
                provenance: _,
                narrowed_out: _,
                context: _,
            } => Ok(Self::Failed {
                failures,
                track_count,
                ledger,
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
            Self::NotFoundAnywhere { .. } | Self::ManualOnly { .. } | Self::Failed { .. } => {
                Vec::new()
            }
        }
    }

    /// The ledger the run showed, as it recorded it. `None` for a run
    /// extraction handed nothing to lay out, and for a verdict whose stored
    /// row records none.
    pub fn ledger(&self) -> Option<&IdentifyRunView> {
        match self {
            Self::Found { ledger, .. }
            | Self::NotFoundAnywhere { ledger }
            | Self::ManualOnly { ledger, .. }
            | Self::Failed { ledger, .. } => ledger.as_ref(),
        }
    }

    /// The identify state this stored verdict stands back up as — what opening
    /// an answered candidate shows without running anything.
    ///
    /// Nothing here is re-derived: the matches are the ones the run settled
    /// on, and the ledger beside them is the one the run recorded, so the pane
    /// after the save draws what it drew on the last live frame — every
    /// provider that was asked, every code each walked, and what each of them
    /// found, including results the agreement then narrowed out.
    ///
    /// The state carries no run context, because there is no run: this one
    /// ended, and a toggle or a re-run starts another from the candidate's own
    /// signals and choices, which are stored and read on their own.
    ///
    /// One thing no write keeps, so no resume has it: a failed run's partial
    /// matches. The failure is what stores, and re-running is what turns
    /// partial evidence into an answer.
    ///
    /// `status_of` is the live library check for a release id the verdict
    /// names, never a stored copy (see the module doc).
    ///
    /// `text` is the candidate's own stored lines, which is what the rows are
    /// judged and ordered against. It belongs to the candidate rather than to
    /// the run, so it stands back up beside the matches and the rows say and
    /// order exactly what they did while the run went.
    pub fn resume_state(
        self,
        status_of: &impl Fn(&MetadataResult) -> LibraryStatus,
        text: CandidateText,
    ) -> IdentifyState {
        let context = || SignalsContext {
            text: text.clone(),
            ..SignalsContext::default()
        };
        match self {
            Self::Found {
                matches,
                track_count,
                provenance,
                narrowed_out,
                narrowed_out_provenance,
                ledger,
            } => {
                let library_statuses: Vec<LibraryStatus> = matches.iter().map(status_of).collect();
                let narrowed_out = NarrowedOut {
                    library_statuses: narrowed_out.iter().map(status_of).collect(),
                    matches: narrowed_out,
                    provenance: narrowed_out_provenance,
                };
                IdentifyState::Found {
                    matches,
                    library_statuses,
                    track_count,
                    provenance,
                    narrowed_out,
                    ledger,
                    context: context(),
                }
            }
            Self::NotFoundAnywhere { ledger } => IdentifyState::NotFoundAnywhere {
                ledger,
                context: context(),
            },
            Self::ManualOnly {
                track_count,
                ledger,
            } => IdentifyState::ManualOnly {
                track_count,
                ledger,
                context: context(),
            },
            // A stored failure resumes with no matches: what one source found
            // before the other failed was never stored, so a resumed failure
            // offers the re-run rather than a partial list.
            Self::Failed {
                failures,
                track_count,
                ledger,
            } => IdentifyState::Failed {
                failures,
                track_count,
                ledger,
                context: context(),
                matches: Vec::new(),
                library_statuses: Vec::new(),
                provenance: Vec::new(),
                narrowed_out: NarrowedOut::default(),
            },
        }
    }
}

#[cfg(test)]
#[path = "verdict_tests.rs"]
mod tests;
