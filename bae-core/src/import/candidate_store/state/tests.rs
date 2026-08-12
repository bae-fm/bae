use super::*;
use crate::import::folder_scanner::{CategorizedFiles, InvalidReason};
use crate::import::types::{ImportPhase, ImportProgress};
use std::path::{Path, PathBuf};

const REL_1: &str = "cccb6034-5922-40d2-8d0b-d94619230882";

/// An empty file set — the reducer only reads a candidate's identity and
/// grouping fields, never its files.
fn empty_categorized() -> CategorizedFiles {
    CategorizedFiles {
        files: Vec::new(),
        format_label: "FLAC".to_string(),
    }
}

fn folder_candidate(path: &str, watched: &str) -> FolderCandidate {
    FolderCandidate {
        path: PathBuf::from(path),
        file_root: PathBuf::from(path),
        name: format!("Candidate {path}"),
        files: empty_categorized(),
        watched_folder_path: watched.to_string(),
        scope: crate::import::folder_scanner::ReleaseFileScope::Recursive,
        file_edit_revision: 0,
        display_path: path.trim_start_matches('/').to_string(),
        resolved_boundaries: Vec::new(),
        combine_ancestor_key: None,
    }
}

fn invalid_candidate(path: &str, watched: &str) -> InvalidCandidate {
    InvalidCandidate {
        path: PathBuf::from(path),
        name: format!("Invalid {path}"),
        watched_folder_path: watched.to_string(),
        display_path: path.trim_start_matches('/').to_string(),
        resolved_boundaries: Vec::new(),
        reason: InvalidReason::NoValidAudio,
    }
}

fn watched(path: &str) -> WatchedFolder {
    WatchedFolder {
        path: path.to_string(),
        name: path.rsplit('/').next().unwrap_or(path).to_string(),
    }
}

#[test]
fn upserts_populate_snapshot_with_folder_and_invalid_candidates() {
    let mut state = CandidateState::default();
    state.upsert_folder(folder_candidate("/watch/a/rel1", "/watch/a"), false, false);
    state.upsert_invalid(invalid_candidate("/watch/a/bad", "/watch/a"));

    let snapshot = state.snapshot(vec![watched("/watch/a")]);
    assert_eq!(snapshot.folder_candidates.len(), 1);
    assert_eq!(
        snapshot.folder_candidates[0].candidate.path,
        PathBuf::from("/watch/a/rel1")
    );
    assert!(!snapshot.folder_candidates[0].skipped);
    assert_eq!(snapshot.invalid_candidates.len(), 1);
    assert_eq!(
        snapshot.invalid_candidates[0].path,
        PathBuf::from("/watch/a/bad")
    );
}

#[test]
fn file_decision_revision_advances_every_path_with_the_same_identity() {
    let mut state = CandidateState::default();
    let first = folder_candidate("/watch/a/first", "/watch/a");
    let second = folder_candidate("/watch/a/second", "/watch/a");
    let content_hash = first.files.content_hash();
    state.upsert_folder(first, false, false);
    state.upsert_folder(second, false, false);
    let settled = state
        .files_for_identity(&content_hash, 0)
        .into_iter()
        .map(|(key, mut files)| {
            files.format_label = "Settled".to_string();
            (key, files)
        })
        .collect();

    let changed = state
        .set_files_for_identity(&content_hash, 0, settled, 1)
        .expect("both matching candidates advance together");

    assert_eq!(changed.len(), 2);
    assert!(changed
        .iter()
        .all(|candidate| candidate.file_edit_revision == 1));
    assert!(changed
        .iter()
        .all(|candidate| candidate.files.format_label == "Settled"));
}

#[test]
fn grouping_folder_is_a_current_release_decision_target() {
    let mut state = CandidateState::default();
    let mut candidate = folder_candidate("/watch/a/Group/Release", "/watch/a");
    candidate.display_path = "Group/Release".to_string();
    state.upsert_folder(candidate, false, false);
    let mut candidate = folder_candidate("/watch/a/Group/Release2", "/watch/a");
    candidate.display_path = "Group/Release2".to_string();
    state.upsert_folder(candidate, false, false);

    assert!(state
        .release_boundary_ancestor_keys(&FolderReleaseDecisionKey {
            watched_folder_path: "/watch/a".to_string(),
            relative_folder_path: "Group".to_string(),
        })
        .is_some());
    assert!(state
        .release_boundary_ancestor_keys(&FolderReleaseDecisionKey {
            watched_folder_path: "/watch/a".to_string(),
            relative_folder_path: "Other".to_string(),
        })
        .is_none());
}

#[test]
fn retain_root_drops_unreported_candidates_and_remove_root_clears() {
    let mut state = CandidateState::default();
    let root = Path::new("/watch/a");
    state.upsert_folder(folder_candidate("/watch/a/old", "/watch/a"), false, false);
    state.upsert_folder(folder_candidate("/watch/a/new", "/watch/a"), false, false);

    // A completed walk that reported only `new` drops `old` and names it.
    let removed = state.retain_root(
        root,
        &std::collections::HashSet::from(["/watch/a/new".to_string()]),
    );
    assert_eq!(removed, vec!["/watch/a/old".to_string()]);

    let snapshot = state.snapshot(vec![watched("/watch/a")]);
    assert_eq!(snapshot.folder_candidates.len(), 1);
    assert_eq!(
        snapshot.folder_candidates[0].candidate.path,
        PathBuf::from("/watch/a/new")
    );

    state.remove_root(root);
    let snapshot = state.snapshot(vec![watched("/watch/a")]);
    assert!(snapshot.folder_candidates.is_empty());
    assert!(snapshot.invalid_candidates.is_empty());
}

#[test]
fn stale_scan_finish_and_failure_cannot_change_a_new_generation() {
    let mut state = CandidateState::default();
    let root = Path::new("/watch/a");
    state.begin_root_scan(root, 1);
    state.upsert_folder(folder_candidate("/watch/a/old", "/watch/a"), false, false);

    // A decision or replacement scan invalidates generation 1 after its DB
    // completion write but before its in-memory completion is applied.
    state.begin_root_scan(root, 2);
    state.upsert_folder(folder_candidate("/watch/a/new", "/watch/a"), false, false);

    assert!(state
        .finish_root_scan(
            root,
            1,
            &std::collections::HashSet::from(["/watch/a/old".to_string()]),
            &std::collections::HashSet::new(),
        )
        .is_none());
    assert!(!state.fail_root_scan(root, 1, "obsolete failure".to_string()));

    let snapshot = state.snapshot(vec![watched("/watch/a")]);
    assert_eq!(snapshot.folder_candidates.len(), 2);
    assert!(matches!(
        snapshot.folder_scan_statuses.as_slice(),
        [WatchedFolderScanStatus {
            status: FolderScanStatus::Scanning,
            ..
        }]
    ));
}

/// A folder watch re-scans on every filesystem event under it, so an import or
/// identify already running on a candidate must survive a walk that re-reports
/// that candidate unchanged. Only a candidate the walk stopped reporting loses
/// its runtime.
#[test]
fn rescan_re_reporting_a_candidate_keeps_its_runtime() {
    let mut state = CandidateState::default();
    let root = Path::new("/watch/a");
    state.upsert_folder(folder_candidate("/watch/a/rel1", "/watch/a"), false, false);
    state.record_event(&ImportEvent::ImportProgress {
        candidate_key: "/watch/a/rel1".to_string(),
        progress: ImportProgress::Progress {
            id: "rel1".to_string(),
            percent: 42,
            phase: ImportPhase::MeasuringLoudness,
            import_id: "imp-1".to_string(),
        },
    });

    // A second walk reports the same candidate and retains it.
    state.upsert_folder(folder_candidate("/watch/a/rel1", "/watch/a"), false, false);
    let removed = state.retain_root(
        root,
        &std::collections::HashSet::from(["/watch/a/rel1".to_string()]),
    );

    assert!(removed.is_empty());
    assert!(
        state.snapshot(vec![watched("/watch/a")]).folder_candidates[0]
            .runtime
            .import_status
            .is_some(),
        "the in-flight import was reset by a re-scan"
    );
}

#[test]
fn set_skipped_round_trips_on_folder_candidate() {
    let mut state = CandidateState::default();
    state.upsert_folder(folder_candidate("/watch/a/rel1", "/watch/a"), false, false);

    state.set_skipped("/watch/a/rel1", true);
    assert!(state.snapshot(vec![watched("/watch/a")]).folder_candidates[0].skipped);

    state.set_skipped("/watch/a/rel1", false);
    assert!(!state.snapshot(vec![watched("/watch/a")]).folder_candidates[0].skipped);
}

#[test]
fn record_event_overlays_import_progress_onto_folder_runtime() {
    let mut state = CandidateState::default();
    state.upsert_folder(folder_candidate("/watch/a/rel1", "/watch/a"), false, false);

    state.record_event(&ImportEvent::ImportProgress {
        candidate_key: "/watch/a/rel1".to_string(),
        progress: ImportProgress::Progress {
            id: "rel1".to_string(),
            percent: 42,
            phase: ImportPhase::MeasuringLoudness,
            import_id: "imp-1".to_string(),
        },
    });

    // The snapshot joins the runtime map at read time, so it reflects the
    // recorded progress.
    let running = state.snapshot(vec![watched("/watch/a")]).folder_candidates[0]
        .runtime
        .import_status
        .clone()
        .expect("import status overlaid");
    match running {
        CandidateImportStatusSnapshot::Importing {
            progress_percent,
            step,
        } => {
            assert_eq!(progress_percent, 42);
            assert_eq!(
                step,
                Some(ImportStep::Running(ImportPhase::MeasuringLoudness))
            );
        }
        other => panic!("expected Importing, got {other:?}"),
    }

    // A later Complete event replaces the overlay with the terminal snapshot.
    state.record_event(&ImportEvent::ImportProgress {
        candidate_key: "/watch/a/rel1".to_string(),
        progress: ImportProgress::Complete {
            id: "rel1".to_string(),
            import_id: "imp-1".to_string(),
            album_id: "1250a7bb-41ed-4500-8ab4-04f5d3461e30".to_string(),
        },
    });
    let complete = state.snapshot(vec![watched("/watch/a")]).folder_candidates[0]
        .runtime
        .import_status
        .clone()
        .expect("import status overlaid");
    match complete {
        CandidateImportStatusSnapshot::Complete {
            release_id,
            album_id,
        } => {
            assert_eq!(release_id, "rel1");
            assert_eq!(album_id, "1250a7bb-41ed-4500-8ab4-04f5d3461e30");
        }
        other => panic!("expected Complete, got {other:?}"),
    }
}

#[test]
fn record_event_overlays_identify_state() {
    let mut state = CandidateState::default();
    state.upsert_folder(folder_candidate("/watch/a/rel1", "/watch/a"), false, false);

    state.record_event(&ImportEvent::IdentifyStateChanged {
        priority: crate::util::rate_limiter::CallPriority::Interactive,
        candidate_key: "/watch/a/rel1".to_string(),
        state: crate::identify::IdentifyState::Idle,
        toolbar: Vec::new(),
    });

    let runtime = &state.snapshot(vec![watched("/watch/a")]).folder_candidates[0].runtime;
    assert!(matches!(
        runtime.identify_state,
        crate::identify::IdentifyState::Idle
    ));
}

#[test]
fn snapshot_orders_by_watched_folder_then_key() {
    let mut state = CandidateState::default();
    state.upsert_folder(folder_candidate("/watch/b/z", "/watch/b"), false, false);
    state.upsert_folder(folder_candidate("/watch/b/a", "/watch/b"), false, false);
    state.upsert_folder(folder_candidate("/watch/a/m", "/watch/a"), false, false);

    // Watched-folder order (a before b) is the primary sort; the candidate key
    // breaks ties within a folder (a before z).
    let snapshot = state.snapshot(vec![watched("/watch/a"), watched("/watch/b")]);
    let paths: Vec<String> = snapshot
        .folder_candidates
        .iter()
        .map(|c| c.candidate.path.to_string_lossy().into_owned())
        .collect();
    assert_eq!(paths, vec!["/watch/a/m", "/watch/b/a", "/watch/b/z"]);
}

#[test]
fn get_resolves_reidentify_runtime_and_scanned_candidates() {
    let mut state = CandidateState::default();

    // A `reidentify:` key has no scanned candidate — only runtime, created by
    // recording an event against it.
    state.record_event(&ImportEvent::ImportProgress {
        candidate_key: "reidentify:rel-1".to_string(),
        progress: ImportProgress::Started {
            id: REL_1.to_string(),
            import_id: "imp-1".to_string(),
        },
    });
    match state.get("reidentify:rel-1") {
        Some(ImportCandidateSnapshot::Runtime { key, runtime }) => {
            assert_eq!(key, "reidentify:rel-1");
            assert!(matches!(
                runtime.import_status,
                Some(CandidateImportStatusSnapshot::Importing { .. })
            ));
        }
        other => panic!("expected runtime snapshot, got {other:?}"),
    }

    // A scanned folder key resolves to its folder candidate; an unknown key is
    // None.
    state.upsert_folder(folder_candidate("/watch/a/rel1", "/watch/a"), false, false);
    assert!(matches!(
        state.get("/watch/a/rel1"),
        Some(ImportCandidateSnapshot::Folder { .. })
    ));
    assert!(state.get("/watch/a/missing").is_none());
}

#[test]
fn runtime_recorded_before_scan_survives_the_scan_recording_its_candidate() {
    let mut state = CandidateState::default();

    // An event can arrive for a candidate key before the folder scan reports
    // the candidate. The runtime entry sits in the runtime map with no scanned
    // candidate yet.
    state.record_event(&ImportEvent::ImportProgress {
        candidate_key: "/watch/a/rel1".to_string(),
        progress: ImportProgress::Progress {
            id: "rel1".to_string(),
            percent: 42,
            phase: ImportPhase::MeasuringLoudness,
            import_id: "imp-1".to_string(),
        },
    });

    // The scan then reports the candidate. Recording it leaves the pre-existing
    // runtime entry alone; only `retain_root`/`remove_root` clear runtime, and
    // only for keys they drop.
    state.upsert_folder(folder_candidate("/watch/a/rel1", "/watch/a"), false, false);

    // The read-time join surfaces the recorded runtime on the scanned candidate.
    let status = state.snapshot(vec![watched("/watch/a")]).folder_candidates[0]
        .runtime
        .import_status
        .clone()
        .expect("recorded import status joined onto the candidate");
    match status {
        CandidateImportStatusSnapshot::Importing {
            progress_percent,
            step,
        } => {
            assert_eq!(progress_percent, 42);
            assert_eq!(
                step,
                Some(ImportStep::Running(ImportPhase::MeasuringLoudness))
            );
        }
        other => panic!("expected Importing, got {other:?}"),
    }
}

#[test]
fn remove_root_returns_the_removed_candidate_keys() {
    let mut state = CandidateState::default();
    state.upsert_folder(folder_candidate("/watch/a/rel1", "/watch/a"), false, false);
    state.upsert_invalid(invalid_candidate("/watch/a/bad", "/watch/a"));
    state.upsert_folder(folder_candidate("/watch/b/rel1", "/watch/b"), false, false);

    let mut removed = state.remove_root(Path::new("/watch/a"));
    removed.sort();
    assert_eq!(
        removed,
        vec!["/watch/a/bad".to_string(), "/watch/a/rel1".to_string()]
    );

    // A second removal of the same root finds nothing left.
    assert!(state.remove_root(Path::new("/watch/a")).is_empty());
}
