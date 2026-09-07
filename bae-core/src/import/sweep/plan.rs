use super::*;

/// The candidates the sweep is responsible for: New ones only.
///
/// Added candidates are already in the library and skipped candidates reflect
/// an explicit user decision, so neither belongs in automatic identification.
///
/// Read from the tables rather than through the list: a pass is planned right
/// after the event that changed the queue — a skip, a scan item — and the
/// list's query lands after the commit it reflects, so it can still describe
/// the queue before that change.
pub(super) async fn new_candidates(
    context: &SweepContext,
) -> Result<Vec<ReleaseCandidate>, crate::library::LibraryError> {
    let candidates = context.library_manager.load_sweepable_candidates().await?;
    let runtime = context.import.candidate_runtimes();
    Ok(candidates
        .into_iter()
        .map(ReleaseCandidate::from)
        .filter(|candidate| {
            runtime.get(candidate.key().as_ref()).is_none_or(|runtime| {
                runtime.import.is_none()
                    && runtime
                        .identify
                        .as_ref()
                        .is_none_or(|identify| !identify.is_finalization_failed())
            })
        })
        .collect())
}

pub(super) fn candidate_identity(candidate: &ReleaseCandidate) -> CandidateIdentity {
    (
        candidate.files().content_hash(),
        candidate.file_edit_revision(),
    )
}

pub(super) fn usable_stored_answer<'a>(
    stored: &'a HashMap<String, DbImportCandidateState>,
    candidate: &ReleaseCandidate,
) -> Option<&'a DbImportCandidateState> {
    stored
        .get(&candidate.files().content_hash())
        .filter(|row| row.file_edits.revision == candidate.file_edit_revision())
        .filter(|row| row.metadata_provenance.is_some() || row.identify.is_some())
}

pub(super) async fn usable_current_candidate(
    context: &SweepContext,
    key: &str,
    identity: &CandidateIdentity,
) -> bool {
    sweepable_candidate(context, key)
        .await
        .is_some_and(|candidate| candidate_identity(&candidate) == *identity)
}

/// Whether this candidate, as it is on disk right now, already has metadata or
/// an identification answer for that shape.
pub(super) async fn current_stored_answer(
    context: &SweepContext,
    candidate: &ReleaseCandidate,
) -> Result<bool, String> {
    let Some(row) = context
        .library_manager
        .load_import_candidate_state(&candidate.files().content_hash())
        .await
        .map_err(|error| error.to_string())?
    else {
        return Ok(false);
    };
    if row.file_edits.revision != candidate.file_edit_revision() {
        return Ok(false);
    }
    Ok(row.metadata_provenance.is_some() || row.identify.is_some())
}
