//! Candidate conversion: what core reads by key, as the automation surface's
//! own shapes.
//!
//! Every candidate read goes through here, and every one of them is a read of
//! the tables: there is no accumulated index behind these functions, so what
//! they return is what the import tables say right now.

use super::*;
use bae_util::timestamp::StoredTimestamp as _;
use std::collections::HashMap;

/// A release the import list lists, as the automation surface shows it.
pub(crate) fn automation_candidate(
    listed: &bae_import::ListedCandidate,
    percents: &HashMap<CandidateKey, u32>,
) -> Result<AutomationCandidate, AutomationError> {
    match listed {
        bae_import::ListedCandidate::Folder(folder) => {
            automation_candidate_from_folder(folder, percents)
        }
        bae_import::ListedCandidate::Invalid(row) => Ok(automation_candidate_from_invalid(row)),
    }
}

/// A folder candidate with what is happening to it right now — its standing
/// in the standings table, with what the runtime holds beside it — joined onto
/// what the tables say: the identify state is the run in flight or the answer being
/// written where there is one, else — waiting on the queue included — the
/// state its stored verdict stands back up as, and the import status
/// is the detail's with the running attempt's progress filled in. Whether each
/// release the state names is in the library is what the read found beside
/// the verdict the state comes from.
fn automation_candidate_from_folder(
    listed: &bae_import::ListedFolder,
    percents: &HashMap<CandidateKey, u32>,
) -> Result<AutomationCandidate, AutomationError> {
    let folder = &listed.detail;
    let candidate = &folder.candidate;
    let draft = folder.draft();
    let live = listed.runtime.as_ref();
    let (view, statuses) = match live.and_then(CandidateRuntimeSnapshot::identification_in_flight) {
        Some(bae_import::IdentificationInFlight::Run(published)) => {
            (published.view(), &listed.run_statuses)
        }
        Some(bae_import::IdentificationInFlight::Queued) | None => {
            (folder.resumed_identify_state.clone(), &listed.stored_statuses)
        }
    };
    let library_statuses = statuses.of(&library_checks(&view))?;
    Ok(AutomationCandidate::Valid {
        common: automation_candidate_common(
            &candidate.key(),
            candidate.source_folders().iter().map(|path| coven::path_text(path)).collect(),
            candidate.name.clone(),
            &candidate.watched_folder_path,
            folder.skipped,
            folder.is_added(),
        ),
        track_count: folder.files.track_count,
        source_audio: folder
            .files
            .source_audio
            .clone()
            .map(AutomationSourceAudioSummary::from_core),
        content_hash: candidate.files.content_hash(),
        runtime: AutomationCandidateRuntime {
            identify_state: automation_identify_state(view, library_statuses),
            signals: folder.signals.clone().map(AutomationSignals::from_core),
            import_status: automation_import_status(
                folder.import_status.as_ref(),
                live.and_then(|live| live.import.as_ref()),
                percents.get(&candidate.key()).copied(),
            ),
        },
        picked_pressing: draft
            .and_then(|draft| draft.release_link.as_ref())
            .and_then(bae_import::ReleaseLink::pressing)
            .cloned()
            .map(AutomationPressingLink::from_core),
        file_evidence: folder
            .file_evidence
            .iter()
            .cloned()
            .map(AutomationFileEvidence::from_core)
            .collect(),
        // An imported candidate holds no draft: its release is the library's.
        // Any other reads as its draft's form, with the mapping table's track
        // rows.
        edit: draft.map(|draft| {
            AutomationCandidateEdit::from_core(bae_import::RawReleaseEdit {
                tracks: bae_import::mapping_tracks(&draft.mapping),
                ..draft.metadata_draft.clone()
            })
        }),
        failure: draft.and_then(|draft| draft.failure.as_ref()).map(|failure| {
            AutomationImportFailure {
                error: failure.reason.detail.clone(),
                failed_at: failure.failed_at.to_rfc3339(),
            }
        }),
    })
}

mirror_struct! {
    AutomationImportedRelease = bae_import::ImportedRelease,
    from_core: fn,
    fields: { release_id, album_id },
}

mirror_enum! {
    AutomationSourceAudioLayout = bae_records::source_audio::SourceAudioLayout,
    from_core: pub(crate) fn,
    variants: { File, Cue },
}

mirror_struct! {
    AutomationSourceAudioDescriptor = bae_records::source_audio::SourceAudioDescriptor,
    from_core: pub(crate) fn,
    fields: {
        layout: (AutomationSourceAudioLayout),
        format: (AutomationAudioFormat),
    },
}

mirror_enum! {
    AutomationSourceAudioSummary = bae_records::source_audio::SourceAudioSummary,
    from_core: pub(crate) fn,
    variants: {
        Uniform { descriptor: (AutomationSourceAudioDescriptor) },
        Mixed { differences: (each AutomationSourceAudioDifference) },
    },
}

mirror_enum! {
    AutomationSourceAudioDifference = bae_records::source_audio::SourceAudioDifference,
    from_core: pub(crate) fn,
    variants: {
        Layout { layouts: (each AutomationSourceAudioLayout) },
        Codec { codecs },
        SampleRate { sample_rates_hz },
        BitDepth { bits_per_sample },
        Channels { channels },
    },
}

/// An unimportable folder. The import service records no runtime against one —
/// nothing identifies or imports it — so the automation shape carries none.
pub(crate) fn automation_candidate_from_invalid(row: &InvalidRow) -> AutomationCandidate {
    AutomationCandidate::Invalid {
        common: automation_candidate_common(
            &row.candidate_key,
            vec![coven::path_text(&row.folder)],
            row.folder_name.clone(),
            &row.watched_folder_path,
            true,
            false,
        ),
        invalid_reason: row.reason.to_string(),
    }
}

fn automation_candidate_common(
    key: &CandidateKey,
    source_folders: Vec<String>,
    name: String,
    watched_folder_path: &std::path::Path,
    skipped: bool,
    is_added: bool,
) -> AutomationCandidateCommon {
    AutomationCandidateCommon {
        key: candidate_key_name(key),
        source_folders,
        name,
        watched_folder_path: coven::path_text(watched_folder_path),
        skipped,
        is_added,
    }
}

/// Where the candidate's import stands: the attempt running now, with how
/// far through its step it is, or else what the last one left in the tables.
pub(crate) fn automation_import_status(
    status: Option<&CandidateImportStatus>,
    in_flight: Option<&ImportInFlight>,
    percent: Option<u32>,
) -> Option<AutomationImportStatus> {
    if let Some(in_flight) = in_flight {
        return Some(AutomationImportStatus::Importing {
            progress_percent: percent,
            step: Some(AutomationImportStep::from_core(in_flight.step)),
        });
    }
    Some(match status? {
        // Running, with no progress reported yet.
        CandidateImportStatus::Importing { .. } => {
            AutomationImportStatus::Importing { progress_percent: None, step: None }
        }
        CandidateImportStatus::Complete { release } => AutomationImportStatus::Complete {
            release: AutomationImportedRelease::from_core(release.clone()),
        },
        CandidateImportStatus::Error { failure } => {
            AutomationImportStatus::Error { error: failure.detail.clone() }
        }
        // An agent reads the reason in the log's own words.
        CandidateImportStatus::Blocked { reason } => {
            AutomationImportStatus::Error { error: reason.to_string() }
        }
    })
}

mirror_enum! {
    AutomationImportStep = ImportStep,
    from_core: pub(crate) fn,
    variants: {
        Preparing(step: (AutomationPrepareStep)),
        Running(phase: (AutomationImportPhase)),
    },
}

#[cfg(test)]
#[path = "candidates_tests.rs"]
mod tests;
