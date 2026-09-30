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
//! does: only the states identification can actually end on, holding only
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
//! What the lookups found is one shape, [`Findings`], on both variants that
//! hold answers. A failed run keeps what the lookups that did answer returned,
//! exactly as a found one does, so opening a failed candidate later shows the
//! results the live run showed beside the lookups that failed.
//!
//! The reducer makes partial evidence unrepresentable as a successful terminal
//! state: any active lookup failure produces `IdentifyState::Failed`, whose
//! findings the queue never imports unattended. Only `Idle` and
//! `Triangulating` have no terminal verdict.

use super::agreements::CandidateText;
use super::combine::{Findings, LibraryStatuses, LookupProvenance, NarrowedOut};
use super::state::{IdentifyState, SignalsContext};
use super::view::IdentifyRunView;
use crate::db::LibraryStatus;
use crate::import::search::{MetadataResult, SourceFailure};
use crate::signals::{AudioFacts, InternalFailure, LookupFailure};

/// Which lookup failed, and — where several providers answer it — which
/// provider. The disc-ID endpoint is MusicBrainz's alone, release details are
/// fetched from the source that named the release, and artist images are
/// Discogs's alone, so those name no provider; the barcode and catalog lookups
/// ask every configured provider independently, so one failing is a fact
/// about that provider. A cover may come from any catalog the picked pressing
/// claims, so its failure names the one that offered it.
///
/// The last three are the fetches that apply the release a run picked
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
    ReleaseDetails(LookupFailure),
    /// Discogs could not give the images of the artists the picked release
    /// credits.
    ArtistImages(LookupFailure),
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
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum TerminalVerdict {
    /// One or more results — what `combine` produced, and what the sidebar and
    /// the auto-import check both work from directly.
    Found {
        findings: Findings,
        track_count: u32,
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
    pub(crate) fn of_pick(result: MetadataResult, track_count: u32) -> Self {
        Self::Found {
            findings: Findings {
                matches: vec![result],
                provenance: vec![LookupProvenance::CHOSEN],
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
    /// fetching the details of the release it settled on, or the artist
    /// images and cover applying it needs. The lookups ran and showed what
    /// they showed, so what they found and the ledger they recorded stay; the
    /// failure joins any the run already had.
    pub(crate) fn fail(&mut self, failure: IdentifyFailure) {
        let (findings, track_count, ledger) = match self {
            Self::Failed { failures, .. } => {
                failures.push(failure);
                return;
            }
            Self::Found {
                findings,
                track_count,
                ledger,
            } => (std::mem::take(findings), *track_count, ledger.take()),
            Self::ManualOnly {
                track_count,
                ledger,
            } => (Findings::default(), *track_count, ledger.take()),
            // Nothing counted the folder's tracks on the way to finding nothing.
            Self::NotFoundAnywhere { ledger } => (Findings::default(), 0, ledger.take()),
            Self::Error { .. } => {
                unreachable!("an error settles no release to fetch the details of")
            }
        };
        *self = Self::Failed {
            failures: vec![failure],
            findings,
            track_count,
            ledger,
        };
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
                // A live per-release check at read time, not a stored copy —
                // see the module doc.
                library_statuses: _,
                // The run's, and the candidate's text it judged against, which
                // is stored on its own and read back beside these findings.
                context: _,
            } => Ok(Self::Found {
                findings,
                track_count,
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

            IdentifyState::Failed {
                failures,
                findings,
                track_count,
                ledger,
                library_statuses: _,
                context: _,
            } => Ok(Self::Failed {
                failures,
                findings,
                track_count,
                ledger,
            }),

            IdentifyState::Error {
                failure,
                context: _,
            } => Ok(Self::Error { failure }),

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
        self.findings()
            .map(|findings| findings.releases().collect())
            .unwrap_or_default()
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

    /// The identify state this stored verdict stands back up as — what opening
    /// an answered candidate shows without running anything.
    ///
    /// Nothing here is re-derived: the matches are the ones the run settled
    /// on, and the ledger beside them is the one the run recorded, so the pane
    /// after the save draws what it drew on the last live frame — every
    /// provider that was asked, every code it was asked about, and what each of them
    /// found, including results the agreement then narrowed out.
    ///
    /// The state carries no run context, because there is no run: this one
    /// ended, and a toggle or a re-run starts another from the candidate's own
    /// signals and choices, which are stored and read on their own.
    ///
    /// A failed run stands back up with what its answering lookups found,
    /// beside the lookups that failed — the pane it showed as it ended.
    ///
    /// `status_of` is the live library check for a release id the verdict
    /// names, never a stored copy (see the module doc).
    ///
    /// `text` is the candidate's own stored lines, which is what the rows are
    /// judged and ordered against, and `audio` is its audio read off its
    /// files. Both belong to the candidate rather than to the run, so they
    /// stand back up beside the matches.
    pub fn resume_state(
        self,
        status_of: &impl Fn(&MetadataResult) -> LibraryStatus,
        text: CandidateText,
        audio: AudioFacts,
    ) -> IdentifyState {
        let context = || SignalsContext {
            text: text.clone(),
            text_settled: true,
            audio: audio.clone(),
            ..SignalsContext::default()
        };
        match self {
            Self::Found {
                findings,
                track_count,
                ledger,
            } => IdentifyState::Found {
                library_statuses: LibraryStatuses::of(&findings, status_of),
                findings,
                track_count,
                ledger,
                context: context(),
            },
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
            Self::Failed {
                failures,
                findings,
                track_count,
                ledger,
            } => IdentifyState::Failed {
                failures,
                library_statuses: LibraryStatuses::of(&findings, status_of),
                findings,
                track_count,
                ledger,
                context: context(),
            },
            Self::Error { failure } => IdentifyState::Error {
                failure,
                context: context(),
            },
        }
    }
}

#[cfg(test)]
#[path = "verdict_tests.rs"]
mod tests;
