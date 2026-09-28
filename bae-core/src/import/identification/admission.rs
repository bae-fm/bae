//! What puts a candidate on the queue on its own, and the reads a run starts
//! from.

use super::*;

pub(super) fn candidate_identity(candidate: &FolderCandidate) -> CandidateIdentity {
    (
        candidate.files.content_hash(),
        candidate.file_edit_revision,
    )
}

/// Put the found releases at `keys` on the queue as one automatic admission,
/// except those that cannot be answered and those whose files already have a
/// stored result, such as a folder renamed after it was identified.
pub(super) async fn admit_found(context: &Context, queue: &mut Queue, keys: Vec<String>) {
    let mut admitted = Vec::with_capacity(keys.len());
    for key in keys {
        let Some(candidate) = answerable_candidate(context, &key).await else {
            continue;
        };
        match context
            .library_manager
            .load_import_candidate_state(&candidate.files.content_hash())
            .await
        {
            Ok(row) if usable_stored_answer(row.as_ref(), &candidate) => {
                info!("identification: {key} was found with its files already answered");
            }
            Ok(_) => admitted.push(candidate),
            Err(error) => warn!(
                "identification: cannot read the stored state of {key} ({error}); leaving it \
                 unidentified"
            ),
        }
    }
    if !admitted.is_empty() {
        admit(context, queue, admitted, Admission::Automatic).await;
    }
}

/// Whether a result is stored for the files this candidate has now.
fn usable_stored_answer(
    row: Option<&DbImportCandidateState>,
    candidate: &FolderCandidate,
) -> bool {
    row.is_some_and(|row| {
        row.file_edits.revision == candidate.file_edit_revision && row.identify.is_some()
    })
}

/// The candidate at `key` if an answer can still be stored for it: not set
/// aside, imported, or claimed by an import. The same read the verdict write
/// makes.
pub(super) async fn answerable_candidate(context: &Context, key: &str) -> Option<FolderCandidate> {
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
    /// What the person decided this candidate's identification asks about.
    pub(super) choices: LookupChoices,
    /// The title to search by when the identifiers name nothing: the person's
    /// words, else the draft's.
    pub(super) title_search: Option<TitleSearch>,
}

/// Read what a run begins from. The draft comes from the pane's rows, because
/// the surface's projection fails for a pick whose documents were cleared.
pub(super) async fn candidate_run_start(
    context: &Context,
    candidate: &FolderCandidate,
) -> Result<CandidateRunStart, crate::library::LibraryError> {
    let content_hash = candidate.files.content_hash();
    let Some(state) = context
        .library_manager
        .load_import_candidate_state(&content_hash)
        .await?
    else {
        return Err(crate::library::LibraryError::Internal(format!(
            "candidate {} has no persisted state row",
            candidate.key()
        )));
    };
    let draft = context
        .library_manager
        .load_import_candidate_pane_rows(&content_hash)
        .await?
        .draft;
    let artist = draft
        .album_artist_assignments
        .first()
        .map_or("", crate::import::ArtistAssignment::name);
    let title_search = match &state.lookup_choices.search_words {
        Some(words) => TitleSearch::of(&words.album, &words.artist),
        None => TitleSearch::of_draft(&draft.album_title, artist),
    };
    Ok(CandidateRunStart {
        choices: state.lookup_choices,
        title_search,
    })
}
