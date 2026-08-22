// ── Overlaying stored verdicts onto the candidates snapshot ─────────────────

fn stored_row(
    candidate: &FolderImportCandidateSnapshot,
    verdict: &TerminalVerdict,
    revision: u64,
) -> DbImportCandidateState {
    DbImportCandidateState {
        content_hash: candidate.candidate.files.content_hash(),
        folder_path: candidate.candidate.path.to_string_lossy().into_owned(),
        identify: Some(crate::db::DbCandidateIdentifyResult {
            verdict: serde_json::to_string(verdict).unwrap(),
            probed_total_duration_ms: 2_400_000,
            identified_at: chrono::Utc::now(),
        }),
        file_edits: crate::import::folder_scanner::CandidateFileEdits {
            revision,
            ..Default::default()
        },
        identity_pick: None,
    }
}

fn projection_of(
    rows: Vec<DbImportCandidateState>,
    library_statuses: Vec<LibraryStatus>,
) -> crate::db::ImportTriageDbProjection {
    crate::db::ImportTriageDbProjection {
        candidate_states: rows
            .into_iter()
            .map(|row| (row.content_hash.clone(), row))
            .collect(),
        library_statuses,
        source_payloads: HashMap::new(),
        imported_releases: HashMap::new(),
    }
}

fn status_for(release_id: &str, in_library: bool) -> LibraryStatus {
    LibraryStatus {
        release_id: release_id.to_string(),
        release_in_library: in_library,
        album_in_library: in_library,
        album_title: None,
        album_id: None,
    }
}

/// An answered candidate with no run in flight reads its stored verdict's
/// resumed state — matches, statuses and all — straight from the projection.
#[test]
fn overlay_resumes_a_stored_verdict_for_an_idle_candidate() {
    let mut snapshot = snapshot_of(vec![candidate("answered", false, false)]);
    let verdict = found(vec![result("rel-a"), result("rel-b")]);
    let rows = vec![stored_row(&snapshot.folder_candidates[0], &verdict, 0)];
    let projection = projection_of(
        rows,
        vec![status_for("rel-a", false), status_for("rel-b", true)],
    );

    overlay_stored_verdicts(&mut snapshot, &projection).unwrap();

    let IdentifyState::Found {
        matches,
        library_statuses,
        ..
    } = &snapshot.folder_candidates[0].runtime.identify_state
    else {
        panic!(
            "expected the stored Found, got {:?}",
            snapshot.folder_candidates[0].runtime.identify_state
        );
    };
    assert_eq!(
        matches
            .iter()
            .map(|result| result.release_id.as_str())
            .collect::<Vec<_>>(),
        vec!["rel-a", "rel-b"]
    );
    assert_eq!(
        library_statuses
            .iter()
            .map(|status| (status.release_id.as_str(), status.release_in_library))
            .collect::<Vec<_>>(),
        vec![("rel-a", false), ("rel-b", true)],
        "statuses ride the resumed state, aligned with its matches"
    );
}

/// A run in flight owns the candidate's state; the stored verdict it is about
/// to replace must not paint over it.
#[test]
fn overlay_leaves_a_live_run_alone() {
    let mut snapshot = snapshot_of(vec![candidate("running", false, false)]);
    snapshot.folder_candidates[0].runtime.identify_state = IdentifyState::NotFoundAnywhere {
        context: crate::identify::state::SignalsContext {
            disc_id: crate::signals::DiscIdSignal::Absent { track_count: 4 },
            barcode_codes: Vec::new(),
            had_barcode_source: false,
            catalogs: Vec::new(),
            excluded: Default::default(),
            discid_results: Vec::new(),
            barcode_results: Vec::new(),
            discid_failure: None,
            barcode_failure: None,
            matched_barcode: None,
            track_count: 4,
        },
    };
    let verdict = found(vec![result("rel-a")]);
    let rows = vec![stored_row(&snapshot.folder_candidates[0], &verdict, 0)];
    let projection = projection_of(rows, vec![status_for("rel-a", false)]);

    overlay_stored_verdicts(&mut snapshot, &projection).unwrap();

    assert!(
        matches!(
            snapshot.folder_candidates[0].runtime.identify_state,
            IdentifyState::NotFoundAnywhere { .. }
        ),
        "a non-idle runtime state is the live truth and stays"
    );
}

/// A verdict stored for an earlier file-edit revision describes files the
/// candidate no longer has; it does not resume.
#[test]
fn overlay_skips_a_verdict_from_another_revision() {
    let mut snapshot = snapshot_of(vec![candidate("edited", false, false)]);
    let verdict = found(vec![result("rel-a")]);
    let rows = vec![stored_row(&snapshot.folder_candidates[0], &verdict, 3)];
    let projection = projection_of(rows, vec![status_for("rel-a", false)]);

    overlay_stored_verdicts(&mut snapshot, &projection).unwrap();

    assert!(matches!(
        snapshot.folder_candidates[0].runtime.identify_state,
        IdentifyState::Idle
    ));
}

/// A projection missing a status for a release the verdict names is a read
/// that failed, not a release outside the library: the overlay refuses it.
#[test]
fn overlay_refuses_a_projection_missing_a_status() {
    let mut snapshot = snapshot_of(vec![candidate("short", false, false)]);
    let verdict = found(vec![result("rel-a"), result("rel-b")]);
    let rows = vec![stored_row(&snapshot.folder_candidates[0], &verdict, 0)];
    let projection = projection_of(rows, vec![status_for("rel-a", false)]);

    let error = overlay_stored_verdicts(&mut snapshot, &projection).unwrap_err();

    assert!(
        error.to_string().contains("rel-b"),
        "the missing release is named: {error}"
    );
    assert!(
        matches!(
            snapshot.folder_candidates[0].runtime.identify_state,
            IdentifyState::Idle
        ),
        "a refused overlay changes nothing"
    );
}
