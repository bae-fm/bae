//! Candidate conversion: what core reads by key, as the automation surface's
//! own shapes.
//!
//! Every candidate read goes through here, and every one of them is a read of
//! the tables: there is no accumulated index behind these functions, so what
//! they return is what the import tables say right now.

use super::*;
use std::collections::HashMap;

/// A folder candidate with its runtime joined. The identify state is the run
/// in flight when there is one, else the state its stored verdict stands back
/// up as — the same answer the import tab shows.
pub(crate) fn automation_candidate_from_folder(
    folder: &ImportCandidateDetail,
    runtime: &HashMap<String, CandidateRuntimeSnapshot>,
) -> AutomationCandidate {
    let candidate = &folder.candidate;
    let mut joined = runtime
        .get(&candidate_path(&candidate.path))
        .cloned()
        .unwrap_or_else(CandidateRuntimeSnapshot::idle);
    if matches!(
        joined.identify_state,
        bae_core::identify::IdentifyState::Idle
    ) {
        joined.identify_state = folder.resumed_identify_state.clone();
    }
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
        runtime: automation_candidate_runtime(&joined),
    }
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

pub(crate) fn automation_candidate_runtime(
    runtime: &CandidateRuntimeSnapshot,
) -> AutomationCandidateRuntime {
    AutomationCandidateRuntime {
        identify_state: automation_identify_state(runtime.identify_state.clone()),
        toolbar: runtime
            .toolbar
            .iter()
            .cloned()
            .map(automation_toolbar_signal)
            .collect(),
        signals: runtime.signals.clone().map(automation_signals),
        import_status: runtime.import_status.clone().map(automation_import_status),
    }
}

pub(crate) fn automation_import_status(
    status: CandidateImportStatusSnapshot,
) -> AutomationImportStatus {
    match status {
        CandidateImportStatusSnapshot::Importing {
            progress_percent,
            step,
        } => AutomationImportStatus::Importing {
            progress_percent,
            step: step.map(automation_import_step),
        },
        CandidateImportStatusSnapshot::Complete { release } => AutomationImportStatus::Complete {
            release_id: release.release_id,
            album_id: release.album_id,
        },
        CandidateImportStatusSnapshot::CloudUploadQueued {
            release,
            outbox_revision,
        } => AutomationImportStatus::CloudUploadQueued {
            release_id: release.release_id,
            album_id: release.album_id,
            outbox_revision,
        },
        CandidateImportStatusSnapshot::Error { error } => AutomationImportStatus::Error { error },
    }
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
