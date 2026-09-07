async fn current_candidate_as_read(
    db: &Database,
    content_hash: &str,
) -> crate::import::CandidateAsRead {
    let state = db
        .load_import_candidate_state(content_hash)
        .await
        .unwrap()
        .expect("the fixture candidate has stored state");
    crate::import::CandidateAsRead {
        content_hash: content_hash.to_string(),
        file_edit_revision: state.file_edits.revision,
        metadata_revision: state.metadata_revision,
    }
}

async fn current_mapping_preparation(
    db: &Database,
    content_hash: &str,
) -> (u64, crate::import::CandidateMappingPreparation) {
    let preparation = db
        .load_import_candidate_preparation(content_hash)
        .await
        .unwrap()
        .expect("the candidate has a stored preparation");
    (
        preparation.metadata_revision,
        crate::import::CandidateMappingPreparation {
            draft: preparation.draft,
            source_discogs_artist_ids: preparation.source_discogs_artist_ids,
            artist_images: preparation.assets.artist_images,
        },
    )
}

include!("import_candidate_state_tests/verdicts_and_bindings.rs");
include!("import_candidate_state_tests/folder_state.rs");
include!("import_candidate_state_tests/pane_rows.rs");
