//! The import sidebar's rows, decided once in core.
//!
//! The sidebar asks the same questions of every candidate — which tab it
//! belongs to, what it leads with, and which commands it offers — and every
//! one of them is a rule rather than a rendering. [`crate::identify::view`] is the precedent: shape the
//! state for the surfaces once, here, so both desktop UIs render the same
//! decisions instead of each re-deriving them from a
//! [`FolderCandidate`](crate::import::FolderCandidate) and a
//! [`TerminalVerdict`](crate::identify::TerminalVerdict).
//!
//! **Nothing here formats text.** Years, counts, durations and byte sizes cross
//! as numbers, and a failed check against the folder crosses as its own
//! [`FolderCheck`] variant carrying its operands, so each platform builds the
//! sentence in its own locale.
//!
//! **Nothing here re-judges a verdict.** [`crate::identify::VerdictSummary`]
//! already says which check against the folder its release failed; this module
//! decides where the row goes and what it shows. Which rows exist and in what
//! order is [`crate::import::list`]'s.

use super::folder_scanner::FolderReleaseDecisionKey;
use super::search::{ImportSearchReleaseDetail, SourceTracks};
use super::types::{Catalog, MetadataProvenance};
use super::MetadataAuthor;
use super::{CandidateRuntimeSnapshot, ImportedRelease};
use crate::identify::{FolderCheck, LeadMatch, VerdictSummary};

mod actions;
mod model;
mod selection;

pub use actions::{CandidateAction, CandidateActionBasis, CandidateLiveState, StoredLookup};
pub use model::*;
pub use selection::{keys_for, selection_offers, SelectionMember, SelectionOffer};

/// Which tab a candidate belongs to, checked in one order:
///
/// 1. **Done first**, which is an import that completed, or a folder a
///    previous session already imported, whether or not it was ever skipped.
/// 2. **Then Skipped**, which is a decision the person already made.
/// 3. **Then a failed attempt**, which is Pending work with its failure
///    stated.
/// 4. **Otherwise Pending.**
///
/// Nothing running is one of these facts: a run and an import are true of a
/// candidate wherever it is placed, and are the row's [`CandidateLiveState`].
pub fn place(
    skipped: bool,
    is_added: bool,
    import_status: Option<&TriageImportStatus>,
) -> TriagePlacement {
    // Spelled out rather than `is_some()`: each variant places the row
    // somewhere different, and a new one should have to be placed here on
    // purpose rather than inherited by an `_`.
    let failed = match import_status {
        Some(TriageImportStatus::Complete { .. }) => return TriagePlacement::Done,
        Some(TriageImportStatus::Error { .. } | TriageImportStatus::Blocked { .. }) => true,
        None => false,
    };
    if is_added {
        return TriagePlacement::Done;
    }
    if skipped {
        return TriagePlacement::Skipped;
    }
    if failed {
        return TriagePlacement::Failed;
    }
    TriagePlacement::Pending
}

/// The failed check against the folder the pane states beside Import: the
/// found release's, unless a person or the tags wrote a valid draft, which is
/// their answer and not judged by the verdict.
pub fn stated_folder_check(
    author: MetadataAuthor,
    draft_valid: bool,
    verdict: Option<&VerdictSummary>,
) -> Option<FolderCheck> {
    // Spelled out: who wrote the draft decides whether the verdict judges it.
    let answered = match author {
        MetadataAuthor::Person | MetadataAuthor::Prefill => draft_valid,
        MetadataAuthor::Identification | MetadataAuthor::Nobody => false,
    };
    if answered {
        return None;
    }
    verdict.and_then(VerdictSummary::folder_check)
}

/// What the last import of a candidate left in the tables.
///
/// The release wins: the failure row is written when an attempt fails and
/// cleared by the commit of the next one that lands, so a release for this
/// hash means an attempt already succeeded and any leftover error is behind
/// it.
///
/// The stored failure is here rather than only in the pane because a row has
/// to say it too. Without it, quitting after a failed import brings the
/// candidate back as an ordinary pending row, and the only way to find out it
/// failed is to open it. It stays in Pending either way — see
/// [`TriagePlacement::Failed`] — but as a row that says what went wrong.
pub fn import_status_of(
    imported: Option<&ImportedRelease>,
    blocked: Option<&crate::import::GroupingBlock>,
    failure: Option<&str>,
) -> Option<TriageImportStatus> {
    if let Some(release) = imported {
        return Some(TriageImportStatus::Complete {
            release: release.clone(),
        });
    }
    if let Some(reason) = blocked {
        return Some(TriageImportStatus::Blocked {
            reason: reason.clone(),
        });
    }
    failure.map(|error| TriageImportStatus::Error {
        error: error.to_string(),
    })
}

/// The runtime facts a row reads: a change to any other part of a candidate's
/// runtime — a progress tick within a running import — leaves the row as it
/// was drawn. The default is a key nothing is running for: no identification
/// work exists and no import has claimed it.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct TriageRuntimeFacts {
    pub identification: Option<IdentificationStatus>,
    /// Where the import that owns this candidate right now stands, or `None`
    /// when no import does. How far it has got is the runtime's, read by the
    /// leaf that draws the bar.
    pub import: Option<ImportStanding>,
}

/// Where an import that owns a candidate stands, as far as what can be asked
/// of it goes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ImportStanding {
    /// Waiting for the worker or running: a cancel still drops it.
    Cancellable,
    /// Writing its release, which completes whatever is asked of it.
    Writing,
}

impl TriageRuntimeFacts {
    /// One order over the fields, most recent fact first: what the last write
    /// failed with, then the answer being written, then the run in flight,
    /// then the queue it is waiting in.
    pub fn of(runtime: &CandidateRuntimeSnapshot) -> Self {
        let identification = if let Some(error) = &runtime.save_failed {
            Some(IdentificationStatus::FinalizationFailed {
                error: error.clone(),
            })
        } else if runtime.saving.is_some() {
            Some(IdentificationStatus::Finalizing)
        } else if runtime.running.is_some() {
            Some(IdentificationStatus::Running)
        } else {
            runtime.queued.map(|_| IdentificationStatus::Queued)
        };
        let import = runtime.import.as_ref().map(|import| match import.step {
            Some(crate::import::ImportStep::Running(crate::import::ImportPhase::Finalizing)) => {
                ImportStanding::Writing
            }
            Some(crate::import::ImportStep::Preparing(_) | crate::import::ImportStep::Running(_))
            | None => ImportStanding::Cancellable,
        });
        Self {
            identification,
            import,
        }
    }

    /// An import owns the candidate, whatever it has reached.
    pub fn importing(&self) -> bool {
        self.import.is_some()
    }

    /// A run for the candidate is queued, running, or having its answer
    /// written — something is still to come from it. A write that failed is
    /// over.
    pub fn identifying(&self) -> bool {
        matches!(
            self.identification,
            Some(
                IdentificationStatus::Queued
                    | IdentificationStatus::Running
                    | IdentificationStatus::Finalizing
            )
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A candidate nobody has imported or skipped is Pending whatever
    /// identification found or is doing for it: the run is the row's, not the
    /// placement's.
    #[test]
    fn an_unimported_candidate_is_pending() {
        assert_eq!(place(false, false, None), TriagePlacement::Pending);
    }

    /// The pane states the found release's failed check while identification's
    /// pick or nobody's draft stands; a person's or the tags' valid draft is
    /// their answer and states none.
    #[test]
    fn a_person_s_draft_states_no_folder_check() {
        let disagrees = VerdictSummary {
            kind: crate::identify::VerdictKind::Found,
            track_count: Some(12),
            pressing_count: 1,
            lead: Some(LeadMatch {
                release_id: "mb-1".to_string(),
                source: Catalog::MusicBrainz,
                source_group_id: None,
                title: "Album".to_string(),
                artist: None,
                year: None,
                media: Vec::new(),
                cover: None,
                source_tracks: Some(SourceTracks::Listed { count: 13 }),
                by_disc_id: true,
                by_barcode: false,
                by_search: false,
            }),
            medium_conflict: None,
        };
        let check = Some(FolderCheck::TrackCountDisagrees {
            local: 12,
            source: 13,
        });
        for (author, draft_valid, stated) in [
            (MetadataAuthor::Identification, true, check.clone()),
            (MetadataAuthor::Nobody, false, check.clone()),
            (MetadataAuthor::Person, false, check.clone()),
            (MetadataAuthor::Person, true, None),
            (MetadataAuthor::Prefill, true, None),
        ] {
            assert_eq!(
                stated_folder_check(author, draft_valid, Some(&disagrees)),
                stated,
                "{author:?}, valid: {draft_valid}"
            );
        }
    }

    fn a_draft() -> TriageMetadataSummary {
        TriageMetadataSummary {
            album_title: "Album Title".to_string(),
            album_artist_assignments: Vec::new(),
        }
    }

    /// The records a pick's documents describe a release in, as the reader
    /// of those documents hands them over.
    fn described_in() -> Vec<crate::import::ReleaseRecord> {
        vec![
            crate::import::ReleaseRecord::new(
                &crate::import::MetadataRef::new(Catalog::MusicBrainz, "mb-1".to_string()),
                None,
                true,
            ),
            crate::import::ReleaseRecord::new(
                &crate::import::MetadataRef::new(Catalog::Discogs, "discogs-1".to_string()),
                None,
                false,
            ),
        ]
    }

    #[test]
    fn a_row_with_no_draft_is_its_folder_and_nothing_else() {
        assert_eq!(
            TriageReading::of(None, None, Vec::new()),
            TriageReading::Unidentified
        );
        assert_eq!(
            TriageReading::of(None, Some(&MetadataProvenance::FileMetadata), Vec::new()),
            TriageReading::Unidentified,
            "a blank draft leads with its folder whatever once wrote it"
        );
    }

    #[test]
    fn a_draft_read_off_the_file_tags_names_no_source() {
        assert_eq!(
            TriageReading::of(
                Some(&a_draft()),
                Some(&MetadataProvenance::FileMetadata),
                Vec::new()
            ),
            TriageReading::Prefilled
        );
        assert_eq!(
            TriageReading::of(Some(&a_draft()), None, Vec::new()),
            TriageReading::Prefilled,
            "a typed-in draft came from nowhere and is still a draft"
        );
    }

    /// A pick reads as identified in exactly the records its documents were
    /// read into — the reading names them, it does not derive them.
    #[test]
    fn a_pick_reads_as_identified_in_the_records_it_is_handed() {
        let pick = MetadataProvenance::ExternalRelease {
            record: crate::import::MetadataRef::new(Catalog::MusicBrainz, "mb-1".to_string()),
            partners: Vec::new(),
        };
        assert_eq!(
            TriageReading::of(Some(&a_draft()), Some(&pick), described_in()),
            TriageReading::Identified {
                records: described_in()
            }
        );
    }
}
