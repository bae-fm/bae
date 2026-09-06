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
            edit: preparation.metadata_draft,
            track_mappings: preparation.track_mappings,
            source_discogs_artist_ids: preparation.source_discogs_artist_ids,
            artist_images: preparation.assets.artist_images,
        },
    )
}

include!("import_candidate_state_tests/verdicts_and_bindings.rs");
include!("import_candidate_state_tests/folder_state.rs");
include!("import_candidate_state_tests/pane_rows.rs");
