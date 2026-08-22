// ── Resuming stored verdicts onto the candidate list ────────────────────────

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

fn states_of(rows: Vec<DbImportCandidateState>) -> HashMap<String, DbImportCandidateState> {
    rows.into_iter()
        .map(|row| (row.content_hash.clone(), row))
        .collect()
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

/// An answered candidate carries its stored verdict's resumed state —
/// matches, statuses and all — on its row.
#[test]
fn a_stored_verdict_resumes_onto_its_candidate() {
    let mut snapshot = snapshot_of(vec![candidate("answered", false, false)]);
    let verdict = found(vec![result("rel-a"), result("rel-b")]);
    let rows = states_of(vec![stored_row(
        &snapshot.folder_candidates[0],
        &verdict,
        0,
    )]);
    let statuses = vec![status_for("rel-a", false), status_for("rel-b", true)];

    resume_stored_verdicts(&mut snapshot, &rows, &statuses).unwrap();

    let IdentifyState::Found {
        matches,
        library_statuses,
        ..
    } = &snapshot.folder_candidates[0].resumed_identify_state
    else {
        panic!(
            "expected the stored Found, got {:?}",
            snapshot.folder_candidates[0].resumed_identify_state
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

/// A verdict stored for an earlier file-edit revision describes files the
/// candidate no longer has; it does not resume.
#[test]
fn a_verdict_from_another_revision_does_not_resume() {
    let mut snapshot = snapshot_of(vec![candidate("edited", false, false)]);
    let verdict = found(vec![result("rel-a")]);
    let rows = states_of(vec![stored_row(
        &snapshot.folder_candidates[0],
        &verdict,
        3,
    )]);

    resume_stored_verdicts(&mut snapshot, &rows, &[status_for("rel-a", false)]).unwrap();

    assert!(matches!(
        snapshot.folder_candidates[0].resumed_identify_state,
        IdentifyState::Idle
    ));
}

/// A status set missing a release the verdict names is a read that failed,
/// not a release outside the library: resuming refuses it.
#[test]
fn resuming_refuses_a_missing_library_status() {
    let mut snapshot = snapshot_of(vec![candidate("short", false, false)]);
    let verdict = found(vec![result("rel-a"), result("rel-b")]);
    let rows = states_of(vec![stored_row(
        &snapshot.folder_candidates[0],
        &verdict,
        0,
    )]);

    let error = resume_stored_verdicts(&mut snapshot, &rows, &[status_for("rel-a", false)])
        .unwrap_err();

    assert!(
        error.to_string().contains("rel-b"),
        "the missing release is named: {error}"
    );
    assert!(
        matches!(
            snapshot.folder_candidates[0].resumed_identify_state,
            IdentifyState::Idle
        ),
        "a refused resume changes nothing"
    );
}
