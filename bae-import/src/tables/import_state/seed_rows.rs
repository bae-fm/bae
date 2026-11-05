//! The state a scan creates for files it finds: the state row under their
//! hash, the folders on disk they were found at, and the draft and cover a
//! new state starts from.
//! Its tables are `seed_rows.sql`, which every device's library holds.

pub(super) const SQL: &str = include_str!("seed_rows.sql");

use super::*;

/// What a new candidate's draft and cover are created from.
pub(crate) enum CandidateStateSeed<'a> {
    /// As many empty track slots as the folder has audio units, and the
    /// folder's own cover: what a candidate holding no draft row reads as.
    Blank { draft: &'a crate::CandidateDraft, cover: Option<crate::cover_art::PreparedCover> },
    /// The folder read as its own files describe it, with the reading it was
    /// projected from and the cover those tags embed.
    FileMetadata(&'a crate::file_metadata_seed::FileMetadataSeed),
}

/// Make sure `files` has a state row, and record `folder` as a folder on disk
/// it was found at; the state lives while a watched folder covers one of
/// those folders. The write that creates the state seeds its draft and cover,
/// unless this device imported the files, whose candidate holds neither; a
/// state that already exists keeps what it holds, its draft row or none.
pub(crate) fn ensure_candidate_state(
    sql: &CovenWrite<'_>,
    path: &CandidateKey,
    folder: &std::path::Path,
    files: &crate::folder_scanner::CategorizedFiles,
    seed: CandidateStateSeed<'_>,
) -> Result<(), DbError> {
    let content_hash = files.content_hash();
    let created = sql.execute(
        "INSERT INTO import_candidate_state (content_hash, candidate_key) VALUES (?, ?) \
         ON CONFLICT (content_hash) DO NOTHING",
        params![content_hash, path],
    )? == 1;
    if !created {
        sql.execute(
            "UPDATE import_candidate_state SET candidate_key = ? WHERE content_hash = ?",
            params![path, content_hash],
        )?;
    }
    sql.execute(
        "INSERT INTO import_candidate_folder (content_hash, folder) \
         VALUES (?, ?) ON CONFLICT DO NOTHING",
        params![content_hash, coven::path_bytes(folder)],
    )?;
    if !created {
        return Ok(());
    }
    if crate::tables::import_origin::imported_release_on(sql, &content_hash)?.is_some() {
        return Ok(());
    }
    // The folder's own cover is stored with the candidate so readers never
    // recompute it; the picture the tags embed comes with the draft they
    // seed.
    let cover = match seed {
        CandidateStateSeed::Blank { draft, cover } => {
            super::pane_rows::insert_draft(
                sql,
                &content_hash,
                draft,
                crate::MetadataAuthor::Nobody,
            )?;
            cover
        }
        CandidateStateSeed::FileMetadata(seed) => {
            super::pane_rows::insert_file_tags_draft(sql, &content_hash, &seed.draft)?;
            crate::cover_art::folder_cover(seed.cover.clone(), files.artwork())
        }
    };
    if let Some(cover) = cover {
        super::cover_rows::save_cover(sql, &content_hash, &cover)?;
    }
    Ok(())
}

/// Start the state of the files hashed `content_hash` over, under `key`: drop
/// the state an undone grouping of the same files left, and when `inherited`
/// holds file decisions, store a state row carrying them.
pub(crate) fn restart_state_on(
    sql: &CovenWrite<'_>,
    content_hash: &str,
    key: &CandidateKey,
    inherited: &crate::folder_scanner::CandidateFileEdits,
) -> Result<(), DbError> {
    sql.execute("DELETE FROM import_candidate_state WHERE content_hash = ?", [content_hash])?;
    if !inherited.is_empty() {
        sql.execute(
            "INSERT INTO import_candidate_state (content_hash, candidate_key) VALUES (?, ?)",
            params![content_hash, key],
        )?;
        super::file_decision_rows::store_file_edits(sql, content_hash, inherited)?;
    }
    Ok(())
}

/// Forget every folder at or below `folders` that files were found at, and
/// the state of every candidate found only there.
pub(crate) fn forget_at_or_below(
    sql: &CovenWrite<'_>,
    folders: &crate::tables::watched_folders::FoldersAtOrBelow,
) -> Result<(), DbError> {
    let under = crate::tables::watched_folders::FoldersAtOrBelow::condition("folder");
    let found: Vec<String> = sql.query(
        &format!("SELECT DISTINCT content_hash FROM import_candidate_folder WHERE {under}"),
        &folders.params(),
        |row| row.get(0),
    )?;
    sql.execute(&format!("DELETE FROM import_candidate_folder WHERE {under}"), &folders.params())?;
    sql.execute(
        "DELETE FROM import_candidate_state \
         WHERE content_hash IN (SELECT value FROM json_each(?)) \
           AND NOT EXISTS (SELECT 1 FROM import_candidate_folder AS found \
                           WHERE found.content_hash = import_candidate_state.content_hash)",
        [serde_json::Value::from(found).to_string()],
    )?;
    Ok(())
}

#[cfg(test)]
#[path = "seed_rows_tests.rs"]
mod tests;
