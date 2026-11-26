//! What puts a candidate on the queue on its own, and the reads a run starts
//! from.

use super::*;

/// Put the found releases at `keys` on the queue as one automatic admission,
/// except those that cannot be answered and those whose files already have a
/// stored result, such as a folder renamed after it was identified.
pub(super) async fn admit_found(
    context: &Context,
    queue: &mut Queue,
    keys: Vec<crate::CandidateKey>,
) {
    let mut answerable = Vec::with_capacity(keys.len());
    for key in keys {
        match context.import.answerable_candidate(&key).await {
            Ok(Some(candidate)) => answerable.push(candidate),
            Ok(None) => {}
            Err(error) => {
                super::queue::record(
                    context,
                    &format!("read {key} to admit it once found"),
                    Err(error),
                );
            }
        }
    }
    let files: Vec<CandidateFiles> = answerable.iter().map(CandidateFiles::of).collect();
    let answered = match context.tables.answered_files(&files).await {
        Ok(answered) => answered,
        Err(error) => {
            let error = error.into();
            for candidate in &answerable {
                fail_unchecked(context, candidate, &error).await;
            }
            return;
        }
    };
    let mut admitted = Vec::with_capacity(answerable.len());
    for (candidate, files) in answerable.into_iter().zip(files) {
        if answered.contains(&files) {
            let key = candidate.key();
            info!("identification: {key} was found with its files already answered");
        } else {
            admitted.push(candidate);
        }
    }
    if !admitted.is_empty() {
        queue.place(context, admitted, Admission::Automatic).await;
    }
}

/// Record that no run of `candidate` could start because whether an answer is
/// stored already could not be read: its identification failed, which its
/// pane shows and Retry identifies again.
async fn fail_unchecked(
    context: &Context,
    candidate: &FolderCandidate,
    error: &crate::ImportError,
) {
    let key = candidate.key();
    super::queue::record(
        context,
        &format!("record that {key} could not be checked for a stored answer"),
        context
            .import
            .fail_identification_start(
                &key,
                &CandidateFiles::of(candidate),
                crate::SaveFailure::NotWritten {
                    error: format!("could not read whether an answer is stored: {error}"),
                },
            )
            .await,
    );
}

/// What one run of a candidate begins from.
pub(super) struct CandidateRunStart {
    /// What the person decided this candidate's identification asks about.
    pub(super) choices: LookupChoices,
    /// The title to search by when the identifiers name nothing: the person's
    /// words, else the draft's.
    pub(super) title_search: Option<TitleSearch>,
}

/// Read what a run begins from, with the candidate's preparation. The draft
/// comes from the stored rows, because the surface's projection fails for a
/// pick whose documents were cleared.
pub(super) async fn candidate_run_start(
    context: &Context,
    candidate: &FolderCandidate,
) -> Result<CandidateRunStart, crate::ImportError> {
    let content_hash = candidate.files.content_hash();
    let Some(crate::tables::CandidatePreparationRead::Stored(stored)) =
        context.tables.load_candidate_preparation(&content_hash).await?
    else {
        return Err(crate::tables::CandidateStateRefusal::NoDraft(content_hash).into());
    };
    let draft = &stored.preparation.metadata.draft;
    let artist = crate::credited_line(&draft.album_artist_assignments).plain_text();
    let title_search = match &stored.lookup_choices.search_words {
        Some(words) => TitleSearch::of(&words.album, &words.artist),
        None => TitleSearch::of_draft(&draft.album_title, &artist),
    };
    Ok(CandidateRunStart { choices: stored.lookup_choices, title_search })
}

#[cfg(test)]
#[path = "admission_tests.rs"]
mod tests;
