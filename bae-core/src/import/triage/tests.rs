//! Triage projection tests — the rules, not the rendering.
//!
//! The first block is over the pure projection. The last drives the real
//! `load`, against a real `Database`, `LibraryManager` and `ImportService` in a
//! tempdir, because its failure modes — a library check that comes back short,
//! an undecodable stored row, a corrupt probed total — are all unreachable from
//! `place` and `project`.

use super::*;
use crate::identify::{GroupKey, ResultProvenance};
use crate::import::cover_art::RemoteCover;
use crate::import::folder_scanner::{
    CandidateFile, CategorizedFiles, FileRole, InvalidReason, ScannedFile,
};
use crate::import::{CandidateRuntimeSnapshot, WatchedFolder};
use std::path::PathBuf;

// ── Fixtures ────────────────────────────────────────────────────────────────

fn candidate_rows(queue: &TriageQueue) -> Vec<&TriageRow> {
    queue
        .sections
        .iter()
        .flat_map(|section| &section.entries)
        .filter_map(|entry| match entry {
            TriageEntry::Candidate(row) => Some(row),
            TriageEntry::Boundary(_) | TriageEntry::Invalid(_) => None,
        })
        .collect()
}

fn invalid_candidates(queue: &TriageQueue) -> Vec<&InvalidCandidate> {
    queue
        .sections
        .iter()
        .flat_map(|section| &section.entries)
        .filter_map(|entry| match entry {
            TriageEntry::Invalid(candidate) => Some(candidate),
            TriageEntry::Candidate(_) | TriageEntry::Boundary(_) => None,
        })
        .collect()
}

fn result(release_id: &str) -> MetadataResult {
    MetadataResult {
        source: MetadataSource::MusicBrainz,
        release_id: release_id.to_string(),
        title: "Album Title".to_string(),
        artist: Some("Artist Name".to_string()),
        year: Some(1999),
        format: Some("CD".to_string()),
        label: None,
        catalog_number: None,
        country: None,
        cover_art: Some(RemoteCover {
            url: "https://example.test/cover.jpg".to_string(),
            thumbnail_url: "https://example.test/thumb.jpg".to_string(),
            label: "Front".to_string(),
            source: MetadataSource::MusicBrainz,
        }),
        source_group_id: Some("group-1".to_string()),
        source_tracks: Some(SourceTracks::Listed {
            count: 11,
            total_duration_ms: Some(2_400_000),
        }),
    }
}

fn found(matches: Vec<MetadataResult>) -> TerminalVerdict {
    let provenance = matches
        .iter()
        .map(|_| ResultProvenance {
            by_disc_id: true,
            by_barcode: false,
            matches_catalog: false,
        })
        .collect();
    TerminalVerdict::Found {
        matches,
        track_count: 11,
        group: GroupKey {
            source: MetadataSource::MusicBrainz,
            source_group_id: "group-1".to_string(),
        },
        provenance,
    }
}

/// Every `NeedsYou` variant, so a table-driven test walks the whole enum. The
/// exhaustive `match` is what makes adding a variant a compile error here.
fn every_needs_you() -> Vec<NeedsYou> {
    let all = vec![
        NeedsYou::AlreadyInLibrary,
        NeedsYou::SeveralMatches { count: 3 },
        NeedsYou::SignalsConflict,
        NeedsYou::NoMatch,
        NeedsYou::NothingToLookUp,
        NeedsYou::TrackCountDisagrees {
            local: 11,
            source: 12,
        },
        NeedsYou::DurationsDisagree {
            probed_ms: 2_400_000,
            source_ms: 2_500_000,
            tolerance_ms: 5_500,
        },
        NeedsYou::SourceLengthsUnknown,
        NeedsYou::LocalDurationUnknown,
    ];
    for variant in &all {
        // No `_` arm: a tenth variant fails to compile until it is listed above.
        match variant {
            NeedsYou::AlreadyInLibrary
            | NeedsYou::SeveralMatches { .. }
            | NeedsYou::SignalsConflict
            | NeedsYou::NoMatch
            | NeedsYou::NothingToLookUp
            | NeedsYou::TrackCountDisagrees { .. }
            | NeedsYou::DurationsDisagree { .. }
            | NeedsYou::SourceLengthsUnknown
            | NeedsYou::LocalDurationUnknown => {}
        }
    }
    all
}

/// Every identify phase, with the same no-`_` guard.
fn every_phase() -> Vec<IdentifyPhase> {
    let all = vec![
        IdentifyPhase::Queued,
        IdentifyPhase::Running,
        IdentifyPhase::NoAnswer,
    ];
    for phase in &all {
        match phase {
            IdentifyPhase::Queued | IdentifyPhase::Running | IdentifyPhase::NoAnswer => {}
        }
    }
    all
}

/// Everything that can be known about a candidate: each classification, and
/// each phase of not knowing yet.
fn every_answer() -> Vec<CandidateAnswer> {
    let mut all = vec![CandidateAnswer::Classified(QueueClassification::Ready)];
    all.extend(
        every_needs_you()
            .into_iter()
            .map(|needs_you| CandidateAnswer::Classified(QueueClassification::NeedsYou(needs_you))),
    );
    all.extend(every_phase().into_iter().map(CandidateAnswer::Unanswered));
    all
}

/// Every import status a candidate can be in, including none.
fn every_import_status() -> Vec<Option<CandidateImportStatusSnapshot>> {
    vec![
        None,
        Some(CandidateImportStatusSnapshot::Importing {
            progress_percent: 40,
            step: None,
        }),
        Some(CandidateImportStatusSnapshot::Complete {
            release: ImportedRelease {
                release_id: "rel-1".to_string(),
                album_id: "alb-1".to_string(),
            },
        }),
        Some(CandidateImportStatusSnapshot::CloudUploadQueued {
            release: ImportedRelease {
                release_id: "rel-2".to_string(),
                album_id: "alb-2".to_string(),
            },
            outbox_revision: 7,
        }),
        Some(CandidateImportStatusSnapshot::Error {
            error: "boom".to_string(),
        }),
    ]
}

fn candidate(folder: &str, skipped: bool, is_added: bool) -> (FolderCandidate, bool, bool) {
    (
        FolderCandidate {
            path: PathBuf::from(format!("/music/{folder}")),
            file_root: PathBuf::from(format!("/music/{folder}")),
            name: folder.to_string(),
            files: CategorizedFiles {
                // One file, named after the folder, so every fixture candidate has
                // its own content hash — the key the stored verdicts are under.
                files: vec![CandidateFile {
                    proposed_audio: true,
                    file: ScannedFile::new(
                        PathBuf::from(format!("/music/{folder}/01.flac")),
                        format!("{folder}-01.flac"),
                        1_000,
                    ),
                    role: FileRole::Audio,
                }],
                format_label: "FLAC".to_string(),
            },
            watched_folder_path: "/music".to_string(),
            scope: crate::import::folder_scanner::ReleaseFileScope::Recursive,
            file_edit_revision: 0,
            display_path: folder.to_string(),
            resolved_boundaries: Vec::new(),
            combine_ancestor_key: None,
        },
        skipped,
        is_added,
    )
}

fn snapshot_of(candidates: Vec<(FolderCandidate, bool, bool)>) -> ImportCandidatesSnapshot {
    ImportCandidatesSnapshot {
        watched_folders: vec![WatchedFolder {
            path: "/music".to_string(),
            name: "music".to_string(),
        }],
        folder_candidates: candidates
            .into_iter()
            .map(
                |(candidate, skipped, is_added)| FolderImportCandidateSnapshot {
                    candidate,
                    actionable: true,
                    skipped,
                    is_added,
                    runtime: CandidateRuntimeSnapshot {
                        identify_state: IdentifyState::Idle,
                        toolbar: vec![],
                        signals: None,
                        import_status: None,
                    },
                },
            )
            .collect(),
        runtime_candidates: vec![],
        invalid_candidates: vec![],
        boundaries: vec![],
        folder_scan_statuses: vec![],
    }
}

fn answer(verdict: TerminalVerdict, classification: QueueClassification) -> Answered {
    Answered {
        verdict,
        classification,
    }
}

fn answers_for(
    snapshot: &ImportCandidatesSnapshot,
    per_candidate: Vec<Option<Answered>>,
) -> HashMap<(String, u64), Answered> {
    snapshot
        .folder_candidates
        .iter()
        .zip(per_candidate)
        .filter_map(|(candidate, answer)| {
            answer.map(|answer| {
                (
                    (
                        candidate.candidate.files.content_hash(),
                        candidate.candidate.file_edit_revision,
                    ),
                    answer,
                )
            })
        })
        .collect()
}

fn picks_for(
    snapshot: &ImportCandidatesSnapshot,
    per_candidate: Vec<Option<Picked>>,
) -> HashMap<(String, u64), Picked> {
    snapshot
        .folder_candidates
        .iter()
        .zip(per_candidate)
        .filter_map(|(candidate, picked)| {
            picked.map(|picked| {
                (
                    (
                        candidate.candidate.files.content_hash(),
                        candidate.candidate.file_edit_revision,
                    ),
                    picked,
                )
            })
        })
        .collect()
}

/// A release the user picked out of a manual search, as its archived documents
/// describe it — a different release from anything `result` produces, so a row
/// leading with the verdict's match instead is a failure rather than a
/// coincidence.
fn picked_release(release_id: &str) -> Picked {
    Picked {
        pick: crate::import::IdentityPick::Release {
            source: MetadataSource::MusicBrainz,
            release_id: release_id.to_string(),
            claim: crate::import::ClaimLevel::Exact,
        },
        release: Some(MatchedRelease::of_pick(
            MetadataSource::MusicBrainz,
            &crate::import::search::ImportSearchReleaseDetail {
                release_id: release_id.to_string(),
                source: MetadataSource::MusicBrainz,
                source_group_id: None,
                title: "Picked Album Title".to_string(),
                artist: Some("Picked Artist Name".to_string()),
                year: Some(1987),
                format: Some("LP".to_string()),
                label: None,
                catalog_number: None,
                country: None,
                barcode: None,
                track_count: 9,
                tracks: Vec::new(),
                cover_art: vec![RemoteCover {
                    url: "https://example.test/picked.jpg".to_string(),
                    thumbnail_url: "https://example.test/picked-thumb.jpg".to_string(),
                    label: "Front".to_string(),
                    source: MetadataSource::MusicBrainz,
                }],
            },
        )),
    }
}

include!("tests/rows.rs");
include!("tests/grouping.rs");
