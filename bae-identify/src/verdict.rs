//! The identify pipeline's terminal outcome — what an import candidate's
//! stored state persists, as one `import_candidate_verdict` row and the `import_candidate_match` rows that
//! hang off it.
//!
//! [`IdentifyState`] is the reducer's own working shape: it carries a full
//! [`SignalsContext`] (raw signal inputs, the user's exclusions) through every
//! state so each landing answer re-combines without re-fetching, and it has
//! `Idle` and `Triangulating` variants that are mid-flight, not a verdict at
//! all. None of that belongs on disk. [`TerminalVerdict`] is the shape that
//! does: only the states identification can actually end on, holding only
//! what the candidate's next launch needs back.
//!
//! What a run showed is stored with what it concluded: the `ledger` on every
//! variant is the run's own last frame, recorded once when the run ended.
//! Reading a candidate back draws that, so the pane after the save is the pane
//! during the run. Its cells are stored as they stood, every provider asked
//! and every answer, not rebuilt from the matches, which can only name
//! providers that produced one; the releases a cell found are the match rows,
//! which hold each release once, as the run last read it.
//!
//! `LibraryStatus` is deliberately absent from every variant, and from the
//! run's own state too. It is mutable local state — another import landing can
//! flip it — so carrying it would freeze a snapshot nothing invalidates. The
//! pane reads it live, against the release ids named here, through one
//! library-status read.
//!
//! What the lookups found is one shape, [`Findings`], on both variants that
//! hold answers. A failed run keeps what the lookups that did answer returned,
//! exactly as a found one does, so opening a failed candidate later shows the
//! results the live run showed beside the lookups that failed.
//!
//! The reducer makes partial evidence unrepresentable as a successful terminal
//! state: any active lookup failure produces `IdentifyState::Failed`, whose
//! findings the queue never imports unattended. Only `Idle` and
//! `Triangulating` have no terminal verdict.

use super::combine::{Findings, NarrowedOut};
use super::state::IdentifyState;
use super::view::IdentifyRunView;
use bae_catalogs::search::{MetadataResult, SourceFailure};
use bae_catalogs::{InternalFailure, LookupFailure};

/// Which lookup failed, and — where several providers answer it — which
/// provider. The disc-ID endpoint is MusicBrainz's alone, and release details
/// are fetched from the source that named the release, so those name no
/// provider; the barcode and catalog lookups ask every configured provider
/// independently, so one failing is a fact about that provider. A cover may
/// come from any catalog the picked pressing claims, so its failure names the
/// one that offered it.
///
/// The last two are the fetches that apply the release a run picked
/// unattended: one failing leaves the pick unapplied, and the verdict stores
/// the failure in its place.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum IdentifyFailure {
    DiscId(LookupFailure),
    Barcode(SourceFailure),
    Catalog(SourceFailure),
    /// One provider could not answer the title search the run fell back on.
    Search(SourceFailure),
    /// MusicBrainz could not answer the search by the audio's ISRCs.
    Isrc(LookupFailure),
    /// One catalog could not answer the lookup of an offered row's pressing
    /// on it, so that row may be missing its record there.
    Pressing(SourceFailure),
    /// The catalog offering the picked release's cover could not give it.
    Cover(SourceFailure),
}

/// The identify pipeline's outcome once it can no longer change without new
/// input from the user or a re-run. Built from [`IdentifyState`]'s terminal
/// variants (`Found`, `NotFoundAnywhere`, `ManualOnly`, `Failed`, `Error`);
/// `Idle` and `Triangulating` have no terminal verdict, hence the fallible
/// conversion below.
///
/// Every variant but `Error` carries the `ledger` its run recorded as it ended — the last
/// frame the run showed, laid out signal by signal and provider by provider.
/// It is stored with the verdict and shown as it stands: what a person saw
/// while the run went is what they see afterwards. `None` for a run
/// extraction handed nothing to lay out, and for a verdict whose stored row
/// records none.
#[derive(Debug, Clone, PartialEq)]
pub enum TerminalVerdict {
    /// One or more results — what `combine` produced, and what the sidebar and
    /// the auto-import check both work from directly.
    Found { findings: Findings, track_count: u32, ledger: Option<IdentifyRunView> },
    /// Both signals ran and settled on zero results. Distinct from a transport
    /// failure — nothing about this candidate's row goes unwritten; "we looked
    /// everywhere and there is nothing" is itself the answer.
    NotFoundAnywhere { ledger: Option<IdentifyRunView> },
    /// Nothing to look up at all: no disc-ID artifact and no barcode source.
    /// Distinct from `NotFoundAnywhere` — there, signals ran and matched
    /// nothing; here, none ran, so a reader offers manual search rather than
    /// claiming a lookup that never happened.
    ManualOnly { track_count: u32, ledger: Option<IdentifyRunView> },
    /// At least one lookup failed, so what the others found must not be
    /// classified as a complete answer. `findings` is what they found — empty
    /// when nothing that answered returned anything.
    Failed {
        failures: Vec<IdentifyFailure>,
        findings: Findings,
        track_count: u32,
        ledger: Option<IdentifyRunView>,
    },
    /// bae broke on its own side and the run ended there: nothing it found
    /// stands, and no ledger was recorded.
    Error { failure: InternalFailure },
}

impl TerminalVerdict {
    /// The result a release a person chose stores: that release, matched by no
    /// signal and no run.
    pub fn of_pick(result: MetadataResult, track_count: u32) -> Self {
        Self::Found {
            findings: Findings {
                matches: vec![result],
                pressings: vec![0],
                narrowed_out: NarrowedOut::default(),
                // A person's pick is theirs to make, whatever the folder's
                // medium; nothing is left to hold back.
                medium_conflict: None,
                // Nothing ranked it, so nothing named it.
                named_notes: Vec::new(),
            },
            track_count,
            ledger: None,
        }
    }

    /// What the lookups found. `None` for the verdicts that hold no answers.
    pub fn findings(&self) -> Option<&Findings> {
        match self {
            Self::Found { findings, .. } | Self::Failed { findings, .. } => Some(findings),
            Self::NotFoundAnywhere { .. } | Self::ManualOnly { .. } | Self::Error { .. } => None,
        }
    }

    /// The verdict this one becomes when a lookup after the run's fails —
    /// fetching the details of the release it settled on, or the cover
    /// applying it needs. The lookups ran and showed what
    /// they showed, so what they found and the ledger they recorded stay; the
    /// failure joins any the run already had.
    pub fn fail(&mut self, failure: IdentifyFailure) {
        let (findings, track_count, ledger) = match self {
            Self::Failed { failures, .. } => {
                failures.push(failure);
                return;
            }
            Self::Found { findings, track_count, ledger } => {
                (std::mem::take(findings), *track_count, ledger.take())
            }
            Self::ManualOnly { track_count, ledger } => {
                (Findings::default(), *track_count, ledger.take())
            }
            // Nothing counted the folder's tracks on the way to finding nothing.
            Self::NotFoundAnywhere { ledger } => (Findings::default(), 0, ledger.take()),
            Self::Error { .. } => {
                unreachable!("an error settles no release to fetch the details of")
            }
        };
        *self = Self::Failed { failures: vec![failure], findings, track_count, ledger };
    }
}

impl TryFrom<IdentifyState> for TerminalVerdict {
    /// The state handed back unchanged when it isn't terminal yet (`Idle` or
    /// `Triangulating`).
    type Error = IdentifyState;

    fn try_from(state: IdentifyState) -> Result<Self, IdentifyState> {
        match state {
            IdentifyState::Found {
                findings,
                track_count,
                ledger,
                // The run's, and the candidate's text it judged against, which
                // is stored on its own and read back beside these findings.
                context: _,
            } => Ok(Self::Found { findings, track_count, ledger }),

            IdentifyState::NotFoundAnywhere { ledger, context: _ } => {
                Ok(Self::NotFoundAnywhere { ledger })
            }

            IdentifyState::ManualOnly { track_count, ledger, context: _ } => {
                Ok(Self::ManualOnly { track_count, ledger })
            }

            IdentifyState::Failed { failures, findings, track_count, ledger, context: _ } => {
                Ok(Self::Failed { failures, findings, track_count, ledger })
            }

            IdentifyState::Error { failure, context: _ } => Ok(Self::Error { failure }),

            other @ (IdentifyState::Idle | IdentifyState::Triangulating { .. }) => Err(other),
        }
    }
}

impl TerminalVerdict {
    /// Every release this verdict names — what a resumer checks live library
    /// status for before standing the state back up. The narrowed-out
    /// releases are named too: a surface offers them beside the matches, so
    /// their library status is checked with the matches' rather than left
    /// unanswered.
    pub fn named_releases(&self) -> Vec<&MetadataResult> {
        self.findings().map(|findings| findings.releases().collect()).unwrap_or_default()
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
            Self::Error { .. } => None,
        }
    }
}

#[cfg(any(test, feature = "test-utils"))]
impl TerminalVerdict {
    /// The identify state this verdict stands as in a test of what consumes a
    /// run's final state, with no signals' context behind it.
    pub fn standing_for_tests(self) -> IdentifyState {
        let context = super::state::SignalsContext::default;
        match self {
            Self::Found { findings, track_count, ledger } => {
                IdentifyState::Found { findings, track_count, ledger, context: context() }
            }
            Self::NotFoundAnywhere { ledger } => {
                IdentifyState::NotFoundAnywhere { ledger, context: context() }
            }
            Self::ManualOnly { track_count, ledger } => {
                IdentifyState::ManualOnly { track_count, ledger, context: context() }
            }
            Self::Failed { failures, findings, track_count, ledger } => IdentifyState::Failed {
                failures,
                findings,
                track_count,
                ledger,
                context: context(),
            },
            Self::Error { failure } => IdentifyState::Error { failure, context: context() },
        }
    }
}

#[cfg(test)]
#[path = "verdict_tests.rs"]
mod tests;
