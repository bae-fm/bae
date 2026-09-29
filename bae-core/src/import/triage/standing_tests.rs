use super::*;
use crate::identify::LeadMatch;
use crate::import::search::SourceTracks;
use crate::import::{Catalog, ImportStanding, MetadataRef, SaveFailure};
use crate::signals::InternalFailure;

fn lead(source_tracks: Option<SourceTracks>) -> LeadMatch {
    LeadMatch {
        release_id: "mb-1".to_string(),
        source: Catalog::MusicBrainz,
        source_group_id: None,
        title: "Album".to_string(),
        artist: None,
        year: None,
        media: Vec::new(),
        cover: None,
        source_tracks,
        by_disc_id: true,
        by_barcode: false,
        by_isrc: false,
        by_search: false,
    }
}

/// A found verdict over a folder of 11 tracks, its lead listing `listed`.
fn found(pressing_count: u32, listed: Option<SourceTracks>) -> VerdictSummary {
    VerdictSummary {
        kind: VerdictKind::Found,
        track_count: Some(11),
        pressing_count,
        lead: Some(lead(listed)),
        medium_conflict: None,
        unread_document: false,
        kept_own_draft: false,
    }
}

fn fits() -> VerdictSummary {
    found(1, Some(SourceTracks::Listed { count: 11 }))
}

fn of_kind(kind: VerdictKind) -> VerdictSummary {
    VerdictSummary {
        track_count: match kind {
            VerdictKind::NotFound | VerdictKind::Error { .. } => None,
            VerdictKind::Found | VerdictKind::ManualOnly | VerdictKind::Failed => Some(11),
        },
        kind,
        pressing_count: 0,
        lead: None,
        medium_conflict: None,
        unread_document: false,
        kept_own_draft: false,
    }
}

fn picked() -> ReleaseLink {
    ReleaseLink {
        record: MetadataRef::new(Catalog::MusicBrainz, "mb-1"),
        partners: Vec::new(),
    }
}

fn pending(verdict: &VerdictSummary) -> Option<PendingStanding> {
    PendingStanding::stored(TriagePlacement::Pending, None, Some(verdict))
}

fn needs_you(reason: NeedsYouReason) -> Option<PendingStanding> {
    Some(PendingStanding::NeedsYou { reason })
}

/// Only Found's rows have a state; Done and Skipped are past it.
#[test]
fn rows_off_found_have_no_state() {
    for placement in [TriagePlacement::Done, TriagePlacement::Skipped] {
        assert_eq!(
            PendingStanding::stored(placement, Some(&picked()), Some(&fits())),
            None
        );
    }
}

/// A failed import is named over the release the candidate is linked to.
#[test]
fn a_failed_import_is_an_import_error_whatever_the_draft() {
    assert_eq!(
        PendingStanding::stored(TriagePlacement::Failed, Some(&picked()), Some(&fits())),
        Some(PendingStanding::ImportError)
    );
}

/// A candidate linked to a release is identified whatever the lookup came
/// to: the person's pick answers several matches and a failed lookup alike.
#[test]
fn a_linked_candidate_is_identified() {
    for verdict in [
        None,
        Some(fits()),
        Some(found(3, None)),
        Some(of_kind(VerdictKind::Failed)),
        Some(of_kind(VerdictKind::NotFound)),
    ] {
        assert_eq!(
            PendingStanding::stored(TriagePlacement::Pending, Some(&picked()), verdict.as_ref()),
            Some(PendingStanding::Identified),
            "{verdict:?}"
        );
    }
}

#[test]
fn a_row_with_no_stored_lookup_is_not_looked_up() {
    assert_eq!(
        PendingStanding::stored(TriagePlacement::Pending, None, None),
        Some(PendingStanding::NotLookedUp)
    );
}

/// A lookup that failed, and one that could not read a release it found in
/// full, are both retried: neither is the person's to answer.
#[test]
fn a_failed_lookup_and_an_unread_release_are_lookup_errors() {
    let mut unread = found(1, None);
    unread.unread_document = true;
    for verdict in [of_kind(VerdictKind::Failed), unread] {
        assert_eq!(
            pending(&verdict),
            Some(PendingStanding::LookupError),
            "{verdict:?}"
        );
    }
}

#[test]
fn each_answer_left_to_the_person_names_why() {
    let mut medium = found(2, None);
    medium.medium_conflict = Some(MediumConflict::CdRip);
    for (verdict, reason) in [
        (found(3, None), NeedsYouReason::Matches { count: 3 }),
        (
            found(1, Some(SourceTracks::Listed { count: 12 })),
            NeedsYouReason::TrackCountMismatch {
                local: 11,
                source: 12,
            },
        ),
        (
            found(1, Some(SourceTracks::Nothing)),
            NeedsYouReason::NoTracklist,
        ),
        (
            medium,
            NeedsYouReason::MediumMismatch {
                folder: MediumConflict::CdRip,
                releases: 2,
            },
        ),
        (of_kind(VerdictKind::NotFound), NeedsYouReason::NotFound),
        (
            of_kind(VerdictKind::ManualOnly),
            NeedsYouReason::NothingToLookUp,
        ),
    ] {
        assert_eq!(pending(&verdict), needs_you(reason), "{verdict:?}");
    }
}

/// A lookup that took its release for the folder, whose draft the person
/// then read from elsewhere, is no longer the lookup's answer.
#[test]
fn a_release_the_person_set_aside_is_unmatched() {
    assert_eq!(pending(&fits()), Some(PendingStanding::Unmatched));
}

/// A run that ended because bae broke is an error, with its text, whatever
/// the draft that is not read from a catalog says.
#[test]
fn a_run_bae_broke_is_an_error_with_its_text() {
    let failure = InternalFailure {
        detail: "reading the store: the disk is full".to_string(),
    };
    assert_eq!(
        pending(&of_kind(VerdictKind::Error {
            failure: failure.clone()
        })),
        Some(PendingStanding::Error { failure })
    );
}

/// What is running for a candidate decides its state over the tables: an
/// import over a run, and a run over any stored state.
#[test]
fn running_work_decides_over_the_tables() {
    let identifying = TriageRuntimeFacts {
        identification: Some(IdentificationStatus::Running),
        import: None,
    };
    let importing = TriageRuntimeFacts {
        identification: Some(IdentificationStatus::Queued),
        import: Some(ImportStanding::Queued),
    };
    let stored = PendingStanding::NeedsYou {
        reason: NeedsYouReason::NotFound,
    };
    assert_eq!(
        stored.clone().with_live(&identifying),
        PendingStanding::Identifying
    );
    assert_eq!(
        stored.clone().with_live(&importing),
        PendingStanding::Importing
    );
    assert_eq!(stored.clone().with_live(&TriageRuntimeFacts::default()), stored);
}

/// An answer bae could not store is bae's own error, with why, whether or not
/// another run could store it.
#[test]
fn an_answer_that_did_not_save_is_an_error() {
    let failed = |failure| TriageRuntimeFacts {
        identification: Some(IdentificationStatus::FinalizationFailed { failure }),
        import: None,
    };
    for failure in [
        SaveFailure::NotWritten {
            error: "disk full".to_string(),
        },
        SaveFailure::Inapplicable {
            error: "no audio to lay the release onto".to_string(),
        },
    ] {
        assert_eq!(
            PendingStanding::NotLookedUp.with_live(&failed(failure.clone())),
            PendingStanding::Error {
                failure: InternalFailure {
                    detail: failure.error().to_string()
                }
            }
        );
    }
}

/// The person keeping their own draft over what the lookup offered is the
/// answer, whatever the lookup found; a pick after it is identified.
#[test]
fn a_kept_draft_is_unmatched_until_a_catalog_is_read() {
    for mut verdict in [
        found(3, None),
        of_kind(VerdictKind::NotFound),
        of_kind(VerdictKind::ManualOnly),
    ] {
        verdict.kept_own_draft = true;
        assert_eq!(
            pending(&verdict),
            Some(PendingStanding::Unmatched),
            "{verdict:?}"
        );
        assert_eq!(
            PendingStanding::stored(TriagePlacement::Pending, Some(&picked()), Some(&verdict)),
            Some(PendingStanding::Identified)
        );
    }
}
