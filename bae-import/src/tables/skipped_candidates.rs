//! The candidates a person set aside, `skipped_import_candidates`: each by
//! its folder on disk, under a watched folder. Setting one aside ends its
//! identification in the same write. Its table is `skipped_candidates.sql`.

pub mod row_fragments;

use super::import_state::change_identification_on;
use super::watched_folders::require_watched;
use super::*;
use crate::CandidateKey;
use std::path::Path;

coven::write!(
    ForgetSkippedAtOrBelow(folder: &'p [u8], below_first: &'p [u8], below_past: &'p [u8]),
    r#"
    DELETE FROM skipped_import_candidates
    WHERE candidate_path = :folder
        OR (candidate_path >= :below_first AND candidate_path < :below_past)
"#
);

coven::write!(
    UnskipFolder(candidate_path: Vec<u8>),
    r#"DELETE FROM skipped_import_candidates WHERE candidate_path = :candidate_path"#
);

coven::write!(
    SkipFolder(candidate_path: Vec<u8>),
    r#"
    INSERT INTO skipped_import_candidates (candidate_path) VALUES (:candidate_path)
    ON CONFLICT DO NOTHING
"#
);

/// Forget every candidate set aside at or below `folders`.
pub(super) fn forget_at_or_below(
    sql: &CovenWrite<'_>,
    folders: &super::watched_folders::FoldersAtOrBelow,
) -> Result<(), DbError> {
    sql.run(&ForgetSkippedAtOrBelowParams {
        folder: &folders.folder,
        below_first: &folders.below_first,
        below_past: &folders.below_past,
    })?;
    Ok(())
}

impl ImportTables {
    /// Every skipped candidate under one root, by its path below the root:
    /// what the tests of skipping and of removing a watched folder read back.
    /// A stored path that is not canonical fails the read.
    #[cfg(any(test, feature = "test-utils"))]
    pub async fn load_skipped_import_candidates(
        &self,
        watched_folder_path: &Path,
    ) -> Result<std::collections::HashSet<crate::RelativeFolder>, DbError> {
        use super::watched_folders::relative_below;
        use std::collections::HashSet;
        use std::path::PathBuf;

        let (below_first, below_past) =
            crate::tables::watched_path::paths_below(watched_folder_path);
        let paths: Vec<PathBuf> = self
            .store
            .read(move |sql| {
                Ok(sql.query(
                    "SELECT candidate_path FROM skipped_import_candidates \
                     WHERE candidate_path >= ? AND candidate_path < ?",
                    params![below_first, below_past],
                    |row| path_column::get(row, 0),
                )?)
            })
            .await?;
        let mut below = HashSet::new();
        for path in &paths {
            crate::tables::watched_path::validate_stored_folder(path).map_err(|_refused| {
                DbError::unreadable("skipped_import_candidates.candidate_path", path)
            })?;
            below.insert(relative_below(watched_folder_path, path)?);
        }
        Ok(below)
    }

    /// [`set_folder_skipped_on`] in a transaction of its own, for a test:
    /// whatever stands at the folder.
    #[cfg(any(test, feature = "test-utils"))]
    pub async fn set_import_candidate_skipped(
        &self,
        candidate_path: &Path,
        skipped: bool,
    ) -> Result<bool, crate::ImportError> {
        let candidate_path = candidate_path.to_path_buf();
        self.store.write(move |sql| set_folder_skipped_on(&sql, &candidate_path, skipped)).await
    }
}

/// Set the candidate whose folder on disk is `candidate_path` aside, or back;
/// `false` when it already was. Setting it aside ends its identification in
/// the same write.
pub(super) fn set_folder_skipped_on(
    sql: &CovenWrite<'_>,
    candidate_path: &Path,
    skipped: bool,
) -> Result<bool, crate::ImportError> {
    crate::tables::watched_path::validate_stored_folder(candidate_path)?;
    if !skipped {
        let changed =
            sql.run(&UnskipFolderParams { candidate_path: coven::path_bytes(candidate_path) })?;
        return Ok(changed == 1);
    }
    require_watched(sql, candidate_path)?;
    // Set aside, the candidate is not identification's to answer: its
    // identification ends with the skip.
    change_identification_on(
        sql,
        &CandidateKey::folder(candidate_path),
        &crate::candidate_runtime::IdentificationChange::Ended,
    )?;
    // Set aside, the candidate is out of the import list, and its pictures'
    // copies go.
    super::picture_copies::forget_for_folder_on(sql, candidate_path)?;
    let changed =
        sql.run(&SkipFolderParams { candidate_path: coven::path_bytes(candidate_path) })?;
    Ok(changed == 1)
}

#[cfg(test)]
#[path = "skipped_candidates_tests.rs"]
mod tests;
