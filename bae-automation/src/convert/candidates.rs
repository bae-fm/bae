//! Candidate conversion: what core reads by key, as the automation surface's
//! own shapes.
//!
//! Every candidate read goes through here, and every one of them is a read of
//! the tables: there is no accumulated index behind these functions, so what
//! they return is what the import tables say right now.

use super::*;
use std::collections::HashMap;

/// A folder candidate with what is happening to it right now joined onto what
/// the tables say: the identify state is the run in flight where there is one,
/// else the state its stored verdict stands back up as, and the import status
/// is the row's with the running attempt's progress filled in.
pub(crate) fn automation_candidate_from_folder(
    folder: &ImportCandidateDetail,
    runtime: &HashMap<String, CandidateRuntimeSnapshot>,
) -> AutomationCandidate {
    let candidate = &folder.candidate;
    let live = runtime.get(&candidate_path(&candidate.path));
    let identify = live
        .and_then(|live| live.identify.clone())
        .unwrap_or_else(|| folder.resumed_identify_state.clone());
    AutomationCandidate::Valid {
        common: automation_candidate_common(
            &candidate.path,
            candidate.name.clone(),
            candidate.watched_folder_path.clone(),
            folder.skipped,
            folder.is_added,
        ),
        track_count: candidate.files.track_count(),
        format_label: candidate.files.format_label.clone(),
        content_hash: candidate.files.content_hash(),
        runtime: AutomationCandidateRuntime {
            toolbar: identify
                .toolbar()
                .into_iter()
                .map(automation_toolbar_signal)
                .collect(),
            identify_state: automation_identify_state(identify),
            signals: folder.signals.clone().map(automation_signals),
            import_status: automation_import_status(
                folder.row.import_status.as_ref(),
                live.and_then(|live| live.import.as_ref()),
            ),
        },
        picked_release: folder.release.clone().map(automation_release_detail),
        file_evidence: folder
            .file_evidence
            .iter()
            .cloned()
            .map(automation_file_evidence)
            .collect(),
        edit: Some(automation_release_user_edit(shaped_edit(
            &folder.metadata_draft,
            &folder.mapping,
        ))),
        failure: folder
            .failure
            .as_ref()
            .map(|failure| AutomationImportFailure {
                error: failure.error.clone(),
                failed_at: failure.failed_at.to_rfc3339(),
            }),
    }
}

/// The metadata a commit of this candidate would write: the form's album
/// fields with the table's own track rows, normalized the way the commit
/// normalizes them. A form that will not shape yet — an empty album title —
/// reads back as the seed it came from.
fn shaped_edit(
    edit: &bae_core::import::RawReleaseEdit,
    mapping: &bae_core::import::MappingTable,
) -> bae_core::import::ReleaseUserEdit {
    let mut raw = edit.clone();
    raw.tracks = bae_core::import::mapping_tracks(mapping);
    raw.shape()
        .unwrap_or_else(|_| bae_core::import::ReleaseUserEdit {
            album_title: raw.album_title.clone(),
            album_artist_assignments: Vec::new(),
            pressing: bae_core::import::PressingEdit::blank(),
            tracks: Vec::new(),
        })
}

/// An unimportable folder. The import service records no runtime against one —
/// nothing identifies or imports it — so the automation shape carries none.
pub(crate) fn automation_candidate_from_invalid(
    candidate: &InvalidCandidate,
) -> AutomationCandidate {
    AutomationCandidate::Invalid {
        common: automation_candidate_common(
            &candidate.path,
            candidate.name.clone(),
            candidate.watched_folder_path.clone(),
            true,
            false,
        ),
        invalid_reason: candidate.reason.to_string(),
    }
}

fn automation_candidate_common(
    path: &Path,
    name: String,
    watched_folder_path: String,
    skipped: bool,
    is_added: bool,
) -> AutomationCandidateCommon {
    let path = candidate_path(path);
    AutomationCandidateCommon {
        key: path.clone(),
        path,
        name,
        watched_folder_path,
        skipped,
        is_added,
    }
}

/// A candidate's path as both its display path and its key — the same string
/// the import service keys its candidate state by.
fn candidate_path(path: &Path) -> String {
    path.to_string_lossy().to_string()
}

/// The row's import status with the running attempt's progress joined in. Only
/// `Importing` has any: the other two are what an import left in a table on its
/// way out.
pub(crate) fn automation_import_status(
    status: Option<&TriageImportStatus>,
    in_flight: Option<&ImportInFlight>,
) -> Option<AutomationImportStatus> {
    Some(match status? {
        TriageImportStatus::Importing => AutomationImportStatus::Importing {
            progress_percent: in_flight.map_or(0, |in_flight| in_flight.progress_percent),
            step: in_flight
                .and_then(|in_flight| in_flight.step)
                .map(automation_import_step),
        },
        TriageImportStatus::Complete { release } => AutomationImportStatus::Complete {
            release_id: release.release_id.clone(),
            album_id: release.album_id.clone(),
        },
        TriageImportStatus::Error { error } => AutomationImportStatus::Error {
            error: error.clone(),
        },
    })
}

pub(crate) fn automation_import_step(step: ImportStep) -> AutomationImportStep {
    match step {
        ImportStep::Preparing(step) => AutomationImportStep::Preparing {
            step: automation_prepare_step(step),
        },
        ImportStep::Running(phase) => AutomationImportStep::Running {
            phase: automation_import_phase(phase),
        },
    }
}
