//! Candidate conversion: the import service's published candidate snapshot as
//! the automation surface's own shapes.
//!
//! Every candidate read goes through here. The snapshot is the whole input —
//! there is no accumulated index behind these functions, so what they return is
//! whatever the import service is publishing right now.

use super::*;

/// Every candidate the import service is publishing, in path order.
///
/// The snapshot's own order is per watched folder; automation presents one flat
/// list across folders and kinds, so it sorts by path — the key callers name
/// candidates by.
pub(crate) fn automation_candidates(
    snapshot: &ImportCandidatesSnapshot,
) -> Vec<AutomationCandidate> {
    let mut candidates: Vec<AutomationCandidate> = snapshot
        .folder_candidates
        .iter()
        .map(automation_candidate_from_folder)
        .chain(
            snapshot
                .invalid_candidates
                .iter()
                .map(automation_candidate_from_invalid),
        )
        .collect();
    candidates.sort_by(|left, right| left.path().cmp(right.path()));
    candidates
}

/// The one candidate `candidate_key` names, or `None` when the snapshot holds
/// no such candidate. A key the snapshot doesn't carry names nothing this
/// surface can act on — including a path a `FolderReleaseBoundary` currently
/// hides, which is a candidate the import service has deliberately withdrawn.
pub(crate) fn automation_candidate(
    snapshot: &ImportCandidatesSnapshot,
    candidate_key: &str,
) -> Option<AutomationCandidate> {
    snapshot
        .folder_candidates
        .iter()
        .find(|folder| candidate_path(&folder.candidate.path) == candidate_key)
        .map(automation_candidate_from_folder)
        .or_else(|| {
            snapshot
                .invalid_candidates
                .iter()
                .find(|invalid| candidate_path(&invalid.path) == candidate_key)
                .map(automation_candidate_from_invalid)
        })
}

fn automation_candidate_from_folder(folder: &FolderImportCandidateSnapshot) -> AutomationCandidate {
    let candidate = &folder.candidate;
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
        runtime: automation_candidate_runtime(&folder.runtime),
    }
}

/// An unimportable folder. The import service records no runtime against one —
/// nothing identifies or imports it — so the automation shape carries none.
fn automation_candidate_from_invalid(candidate: &InvalidCandidate) -> AutomationCandidate {
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
