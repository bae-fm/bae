use super::{empty_db, fixed_now};

/// The candidate as a caller reads it back after a fixture stored it: no file
/// decisions edited yet, and whatever metadata revision the caller just
/// observed. Tests that deliberately submit a stale or advanced
/// `file_edit_revision` write the record out in full instead.
fn as_read(content_hash: &str, metadata_revision: u64) -> crate::import::CandidateAsRead {
    crate::import::CandidateAsRead {
        content_hash: content_hash.to_string(),
        file_edit_revision: 0,
        metadata_revision,
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

/// Store the person's answer for how one folder reads, alone: the row a
/// fixture needs beside the entries it writes itself. The service never stores
/// a decision without the candidates it gives — that is
/// `commit_folder_reading`.
async fn store_user_folder_decision(
    db: &Database,
    key: &crate::import::folder_scanner::FolderReleaseDecisionKey,
    decision: crate::import::folder_scanner::FolderReleaseDecision,
) -> Result<(), coven::DbError> {
    let key = key.clone();
    db.call(move |sql| {
        crate::db::client::folder_scans::store_folder_release_decision(
            sql,
            &key,
            decision,
            crate::import::folder_scanner::FolderReleaseDecisionAuthor::User,
        )
    })
    .await
}

include!("import_candidate_state_tests/verdicts_and_bindings.rs");
include!("import_candidate_state_tests/folder_state.rs");
include!("import_candidate_state_tests/pane_rows.rs");
include!("import_candidate_state_tests/lookup_choices.rs");
include!("import_candidate_state_tests/metadata_author.rs");
