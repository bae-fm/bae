//! The import sidebar's rows, decided once in core.
//!
//! The sidebar asks the same questions of every candidate — which tab it
//! belongs to, which Needs-you group it joins, what it leads with, and which
//! commands it offers — and every one of them is a rule rather
//! than a rendering. [`crate::identify::view`] is the precedent: shape the
//! state for the surfaces once, here, so both desktop UIs render the same
//! decisions instead of each re-deriving them from a
//! [`FolderCandidate`](crate::import::FolderCandidate) and a
//! [`TerminalVerdict`](crate::identify::TerminalVerdict).
//!
//! **Nothing here formats text.** Years, counts, durations and byte sizes cross
//! as numbers, and a disagreement crosses as its own [`NeedsYou`] variant
//! carrying its operands, so each platform builds the sentence in its own
//! locale.
//!
//! **Nothing here re-classifies.** [`crate::identify::ready::classify`] already
//! answers what the queue needs from the user; this module decides where that
//! answer puts the row and what the row shows. Which rows exist and in what
//! order is [`crate::import::list`]'s.

use super::folder_scanner::{FolderReleaseDecisionKey, ResolvedFolderReleaseBoundary};
use super::search::{ImportSearchReleaseDetail, SourceTracks};
use super::types::{Catalog, MetadataProvenance};
use super::MetadataAuthor;
use super::{CandidateRuntimeSnapshot, ImportedRelease};
use crate::identify::{LeadMatch, NeedsYou, QueueClassification, VerdictSummary};

mod actions;
mod model;

pub(crate) use actions::candidate_actions;
pub use actions::CandidateAction;
pub use model::*;

/// Which tab a candidate belongs to, and why a Pending row still needs input.
///
/// A total function of the facts core already holds, checked in one order:
///
/// 1. **An import in flight outranks everything**, including the library
///    check: the release row lands partway through an import, so `is_added`
///    flips before the import is finished, and a row that reads Done then says
///    the folder is in the library while its files are still being copied.
/// 2. **Then Done**, which is an import that completed, or a folder a previous
///    session already imported. Not awaiting triage, whatever its verdict says
///    and whether or not it was ever skipped.
/// 3. **Then Skipped**, which is a decision the user already made.
/// 4. **Then a failed attempt**, which is Pending work: the folder is not in
///    the library and the only thing standing between it and being there is
///    another attempt. It comes before the draft because a failed candidate
///    always has one — read the draft first and the row would say Ready, and
///    join the set a bulk import sweeps up, on the strength of the attempt
///    that just failed.
/// 5. **Then a valid draft a person or the tags wrote**, which is Ready. A
///    person's draft is their answer to whatever the verdict was going to
///    ask; a draft the folder's tags seeded is what the person chose to start
///    from. Either way nothing is left to ask.
/// 6. **Then what its stored verdict classified to.** This is where a draft
///    identification wrote lands: a run applying its own pick is not an
///    answer, so the Ready rule's checks — the track count, the library —
///    decide whether it is Ready or which question it asks.
///
/// An invalid draft is never Ready, whoever wrote it and whatever the verdict
/// says: Ready means a bulk import can commit it. With no verdict, or with one
/// classified Ready over a draft that would not import, the row is Pending.
///
/// Live identification is not one of these facts. A run is true of a candidate
/// wherever that candidate is placed — a Ready row somebody asked to identify
/// again is still Ready, and still running — so it rides on the row as
/// [`TriageRow::identification`] rather than displacing the placement.
pub fn place(
    skipped: bool,
    is_added: bool,
    import_status: Option<&TriageImportStatus>,
    author: MetadataAuthor,
    draft_valid: bool,
    answer: Option<&QueueClassification>,
) -> TriagePlacement {
    // Spelled out rather than `is_some()`: each variant places the row
    // somewhere different, and a new one should have to be placed here on
    // purpose rather than inherited by an `_`.
    let failed = match import_status {
        Some(TriageImportStatus::Importing) => return TriagePlacement::Importing,
        Some(TriageImportStatus::Complete { .. }) => return TriagePlacement::Done,
        Some(TriageImportStatus::Error { .. }) => true,
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
    // Spelled out for the same reason: who wrote the draft decides whether it
    // answers the verdict or is judged by it.
    let answered = match author {
        MetadataAuthor::Person | MetadataAuthor::Prefill => draft_valid,
        MetadataAuthor::Identification | MetadataAuthor::Nobody => false,
    };
    if answered {
        return TriagePlacement::Ready;
    }
    match answer {
        Some(QueueClassification::Ready) if draft_valid => TriagePlacement::Ready,
        Some(QueueClassification::Ready) | None => TriagePlacement::Pending,
        Some(QueueClassification::NeedsYou(reason)) => TriagePlacement::NeedsYou {
            reason: reason.clone(),
        },
    }
}

/// The Ready check a row waits on the person for: its release's tracklist
/// disagrees with the folder, or there is no tracklist to compare. Stated
/// beside the Import it bears on; the other questions a row can ask — which
/// release, whether to retry a lookup — are answered in Find online.
pub fn ready_check(placement: &TriagePlacement) -> Option<NeedsYou> {
    let TriagePlacement::NeedsYou { reason } = placement else {
        return None;
    };
    match reason {
        NeedsYou::TrackCountDisagrees { .. } | NeedsYou::SourceTracksUnknown => {
            Some(reason.clone())
        }
        NeedsYou::AlreadyInLibrary
        | NeedsYou::SeveralMatches { .. }
        | NeedsYou::NoMatch
        | NeedsYou::NothingToLookUp
        | NeedsYou::LookupFailed => None,
    }
}

/// Where a candidate's import stands, from the three places that can say so.
///
/// A running import is the only live fact, so it outranks both stored ones. Of
/// those, the release wins: the failure row is written when an attempt fails
/// and cleared when the next one is queued, so a release for this hash means
/// an attempt already succeeded and any leftover error is behind it.
///
/// The stored failure is here rather than only in the pane because a row has
/// to say it too. Without it, quitting after a failed import brings the
/// candidate back as an ordinary pending row, and the only way to find out it
/// failed is to open it. It stays in Pending either way — see
/// [`TriagePlacement::Failed`] — but as a row that says what went wrong.
pub fn import_status_of(
    importing: bool,
    imported: Option<&ImportedRelease>,
    failure: Option<&str>,
) -> Option<TriageImportStatus> {
    if importing {
        return Some(TriageImportStatus::Importing);
    }
    if let Some(release) = imported {
        return Some(TriageImportStatus::Complete {
            release: release.clone(),
        });
    }
    failure.map(|error| TriageImportStatus::Error {
        error: error.to_string(),
    })
}

/// The runtime facts a row reads: a change to any other part of a candidate's
/// runtime — a progress tick within a running import — leaves the queue as
/// projected.
/// The default is a key nothing is running for: no identification work exists
/// and no import has claimed it.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct TriageRuntimeFacts {
    pub identification: Option<IdentificationStatus>,
    /// Whether an import owns this candidate right now. How far it has got is
    /// the runtime's, read by the leaf that draws the bar.
    pub importing: bool,
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
        Self {
            identification,
            importing: runtime.import.is_some(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A candidate nobody has answered is Pending whatever identification is
    /// doing for it: the run is the row's, not the placement's.
    #[test]
    fn an_unanswered_candidate_is_pending() {
        assert_eq!(
            place(false, false, None, MetadataAuthor::Nobody, false, None),
            TriagePlacement::Pending
        );
    }

    /// Only a tracklist that disagrees with the folder, or is missing, is a
    /// Ready check stated beside Import; which release, or retrying a lookup,
    /// is Find online's question.
    #[test]
    fn a_ready_check_is_a_tracklist_question() {
        let waiting = |reason| TriagePlacement::NeedsYou { reason };
        let disagrees = NeedsYou::TrackCountDisagrees {
            local: 13,
            source: 12,
        };
        assert_eq!(ready_check(&waiting(disagrees.clone())), Some(disagrees));
        assert_eq!(
            ready_check(&waiting(NeedsYou::SourceTracksUnknown)),
            Some(NeedsYou::SourceTracksUnknown)
        );
        assert_eq!(
            ready_check(&waiting(NeedsYou::SeveralMatches { count: 2 })),
            None
        );
        assert_eq!(ready_check(&waiting(NeedsYou::LookupFailed)), None);
        assert_eq!(ready_check(&TriagePlacement::Ready), None);
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
