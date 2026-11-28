//! The state a scan creates for files it finds: the state row under their
//! hash, the folders on disk they were found at, and the draft and cover a
//! new state starts from.
//! Its tables are `seed_rows.sql`, which every device's library holds.

use super::*;
use bae_store::WriteStatements as _;

coven::write!(
    InsertCandidateStateIfNew(content_hash: &'p str, candidate_key: &'p crate::CandidateKey),
    r#"
    INSERT INTO import_candidate_state (content_hash, candidate_key)
    VALUES (:content_hash, :candidate_key)
    ON CONFLICT (content_hash) DO NOTHING
"#
);

coven::write!(
    SetCandidateStateKey(content_hash: &'p str, candidate_key: &'p crate::CandidateKey),
    r#"
    UPDATE import_candidate_state SET candidate_key = :candidate_key
    WHERE content_hash = :content_hash
"#
);

coven::write!(
    InsertCandidateFolder(content_hash: &'p str, folder: Vec<u8>),
    r#"
    INSERT INTO import_candidate_folder (content_hash, folder)
    VALUES (:content_hash, :folder)
    ON CONFLICT DO NOTHING
"#
);

/// Make sure `files` has a state row, and record `folder` as a folder on disk
/// it was found at; the state lives while a watched folder covers one of
/// those folders. When a state holds no draft row and this device did not
/// import its files, the function writes one: the draft that the files' own
/// tags describe when `file_metadata` holds those tags, and otherwise the
/// blank draft that the folder's stored audio units make. A state that holds
/// a draft keeps it.
pub(crate) fn ensure_candidate_state(
    sql: &CovenWrite<'_>,
    path: &CandidateKey,
    folder: &std::path::Path,
    files: &crate::folder_scanner::CategorizedFiles,
    file_metadata: Option<&crate::file_metadata_seed::FileMetadataSeed>,
) -> Result<(), DbError> {
    let content_hash = files.content_hash();
    let created = sql.run(&InsertCandidateStateIfNewParams {
        content_hash: &content_hash,
        candidate_key: path,
    })? == 1;
    if !created {
        sql.run(&SetCandidateStateKeyParams { content_hash: &content_hash, candidate_key: path })?;
    }
    sql.run(&InsertCandidateFolderParams {
        content_hash: &content_hash,
        folder: coven::path_bytes(folder),
    })?;
    if crate::tables::import_origin::imported_release_on(sql, &content_hash)?.is_some()
        || super::pane_rows::has_draft_row_on(sql, &content_hash)?
    {
        return Ok(());
    }
    match file_metadata {
        Some(seed) => {
            super::pane_rows::insert_file_tags_draft(sql, &content_hash, &seed.draft)?;
            // The picture the tags embed comes with the draft they seed.
            if let Some(cover) = crate::cover_art::folder_cover(seed.cover.clone(), files.artwork())
            {
                super::cover_rows::save_cover(sql, &content_hash, &cover)?;
            }
        }
        None => super::blank_draft_rows::seed_blank_draft_on(sql, &content_hash)?,
    }
    Ok(())
}

coven::write!(
    DeleteCandidateState(content_hash: &'p str),
    r#"
    DELETE FROM import_candidate_state WHERE content_hash = :content_hash
"#
);

coven::write!(
    InsertCandidateState(content_hash: &'p str, candidate_key: &'p crate::CandidateKey),
    r#"
    INSERT INTO import_candidate_state (content_hash, candidate_key)
    VALUES (:content_hash, :candidate_key)
"#
);

/// Start the state of the files hashed `content_hash` over, under `key`: drop
/// the state an undone grouping of the same files left, and when `inherited`
/// holds file decisions, store a state row carrying them.
pub(crate) fn restart_state_on(
    sql: &CovenWrite<'_>,
    content_hash: &str,
    key: &CandidateKey,
    inherited: &crate::folder_scanner::CandidateFileEdits,
) -> Result<(), DbError> {
    sql.run(&DeleteCandidateStateParams { content_hash })?;
    if !inherited.is_empty() {
        sql.run(&InsertCandidateStateParams { content_hash, candidate_key: key })?;
        super::file_decision_rows::store_file_edits(sql, content_hash, inherited)?;
    }
    Ok(())
}

coven::write!(
    ForgetCandidateFolders(folder: &'p [u8], below_first: &'p [u8], below_past: &'p [u8]),
    r#"
    DELETE FROM import_candidate_folder
    WHERE folder = :folder OR (folder >= :below_first AND folder < :below_past)
"#
);

coven::write!(
    DeleteStatesFoundNowhere(content_hashes: String),
    r#"
    DELETE FROM import_candidate_state
    WHERE content_hash IN (SELECT wanted.value FROM json_each(:content_hashes) AS wanted)
        AND NOT EXISTS (SELECT 1 FROM import_candidate_folder AS found
                        WHERE found.content_hash = import_candidate_state.content_hash)
"#
);

// The candidates whose files were found at the folder `root` or below it.
coven::read!(
    writer CandidatesFoundAtOrBelow(root: Vec<u8>, below_first: Vec<u8>, below_past: Vec<u8>),
    r#"
    SELECT DISTINCT found.content_hash
    FROM import_candidate_folder AS found
    WHERE found.folder = :root
       OR (found.folder >= :below_first AND found.folder < :below_past)
"#
);

/// Forget every folder at or below `folders` that files were found at, and
/// the state of every candidate found only there.
pub(crate) fn forget_at_or_below(
    sql: &CovenWrite<'_>,
    folders: &crate::tables::watched_folders::FoldersAtOrBelow,
) -> Result<(), DbError> {
    let found: Vec<String> = bae_store::statement_rows(
        sql,
        &CandidatesFoundAtOrBelowParams {
            root: folders.folder.clone(),
            below_first: folders.below_first.clone(),
            below_past: folders.below_past.clone(),
        },
    )?
    .into_iter()
    .map(|found| found.content_hash)
    .collect();
    sql.run(&ForgetCandidateFoldersParams {
        folder: &folders.folder,
        below_first: &folders.below_first,
        below_past: &folders.below_past,
    })?;
    sql.run(&DeleteStatesFoundNowhereParams {
        content_hashes: serde_json::Value::from(found).to_string(),
    })?;
    Ok(())
}

#[cfg(test)]
#[path = "seed_rows_tests.rs"]
mod tests;
