//! What puts a candidate on the queue, and the reads that decide it.

use super::*;

/// What one candidate's identification is judged by: the bytes it holds and
/// the revision of the file decisions taken over them.
pub(super) fn candidate_identity(candidate: &ReleaseCandidate) -> CandidateIdentity {
    (
        candidate.files().content_hash(),
        candidate.file_edit_revision(),
    )
}

/// Whether a run has already finished for the files this candidate has right
/// now. What its draft holds is not an answer — only a stored result is.
///
/// The one predicate, over the one row: the automatic admission reads every
/// candidate's row in a map, and a re-evaluation of a single candidate reads
/// its own.
pub(super) fn usable_stored_answer(
    row: Option<&DbImportCandidateState>,
    candidate: &ReleaseCandidate,
) -> bool {
    row.is_some_and(|row| {
        row.file_edits.revision == candidate.file_edit_revision() && row.identify.is_some()
    })
}

/// The candidate at `key` an identification can still answer: a stored,
/// actionable candidate that is not set aside, already in the library, or
/// claimed by a running import.
///
/// A read that fails answers no candidate, and says so. This is the same read
/// the verdict write makes, so what the queue admits and what the write accepts
/// cannot disagree.
pub(super) async fn answerable_candidate(context: &Context, key: &str) -> Option<ReleaseCandidate> {
    match context.import.answerable_candidate(key).await {
        Ok(candidate) => candidate,
        Err(error) => {
            warn!("identification: cannot read candidate {key} ({error}); leaving it alone");
            None
        }
    }
}

/// What one run of a candidate begins from.
pub(super) struct CandidateRunStart {
    /// The editable metadata revision the run answers. A later edit makes its
    /// terminal result stale even when the candidate's files did not change.
    pub(super) metadata_revision: u64,
    /// What the person decided this candidate's identification asks about.
    pub(super) choices: LookupChoices,
}

/// Read both in one go, off the one stored row that states them.
pub(super) async fn candidate_run_start(
    context: &Context,
    candidate: &ReleaseCandidate,
) -> Result<CandidateRunStart, crate::library::LibraryError> {
    context
        .library_manager
        .load_import_candidate_state(&candidate.files().content_hash())
        .await?
        .map(|state| CandidateRunStart {
            metadata_revision: state.metadata_revision,
            choices: state.lookup_choices,
        })
        .ok_or_else(|| {
            crate::library::LibraryError::Internal(format!(
                "candidate {} has no persisted state row",
                candidate.key()
            ))
        })
}

/// Whether the automatic admission still wants this candidate answered: its
/// row states no result for the files it has right now.
///
/// A read that fails answers "leave it alone" and says so: without the row
/// there is no telling an answered candidate from an unanswered one, and
/// identifying it again would spend the rate limit re-learning what it may
/// already know.
pub(super) async fn wants_an_answer(context: &Context, candidate: &ReleaseCandidate) -> bool {
    match context
        .library_manager
        .load_import_candidate_state(&candidate.files().content_hash())
        .await
    {
        Ok(row) => !usable_stored_answer(row.as_ref(), candidate),
        Err(error) => {
            warn!(
                "identification: cannot read the stored state of {} ({error}); leaving it as it \
                 is until the next event about it",
                candidate.key()
            );
            false
        }
    }
}

/// Admit every candidate the automatic policy is responsible for that holds no
/// usable stored answer. Admitting is idempotent: a candidate the queue already
/// holds keeps the place — and the admission — it has.
///
/// Read from the tables rather than through the list: an admission runs right
/// after the event that changed the queue — a skip, a scan item — and the
/// list's query lands after the commit it reflects, so it can still describe
/// the queue before that change.
pub(super) async fn admit_automatically(context: &Context, queue: &mut Queue) {
    let candidates = match context.library_manager.load_sweepable_candidates().await {
        Ok(candidates) => candidates,
        Err(error) => {
            // Without the list there is nothing to admit from. Skip it; the
            // next scan admits again.
            warn!(
                "identification: could not read the candidate list ({error}); \
                 admitting nothing this time"
            );
            return;
        }
    };
    let stored = match context.library_manager.load_import_candidate_states().await {
        Ok(stored) => stored,
        Err(error) => {
            // Without the stored set there is no telling answered from
            // unanswered, and identifying the whole queue again would spend the
            // rate limit re-learning what it already knows.
            warn!(
                "identification: could not read stored candidate states ({error}); \
                 admitting nothing this time"
            );
            return;
        }
    };
    let runtime = context.import.candidate_runtimes();
    let admitted: Vec<ReleaseCandidate> = candidates
        .into_iter()
        .map(ReleaseCandidate::from)
        .filter(|candidate| {
            // An import owns the candidate, or the last write of its answer
            // failed: neither is the automatic admission's to take back.
            runtime
                .get(candidate.key().as_ref())
                .is_none_or(|runtime| runtime.import.is_none() && runtime.save_failed.is_none())
        })
        .filter(|candidate| {
            !usable_stored_answer(stored.get(&candidate.files().content_hash()), candidate)
        })
        .collect();
    let planned = admitted.len();
    let opened = queue.admit_all(context, admitted, Admission::Automatic);
    info!(
        "identification: the automatic admission wants {planned} candidate(s) answered, \
         {opened} of them new to the queue"
    );
}
