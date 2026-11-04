//! The candidates a person set aside, `skipped_import_candidates`: each by
//! its folder on disk, under a watched folder. Setting one aside ends its
//! identification in the same write. Its table is `skipped_candidates.sql`,
//! which every device's library holds.

#[cfg(desktop)]
use super::import_state::change_identification_on;
#[cfg(desktop)]
use super::watched_folders::require_watched;
#[cfg(desktop)]
use super::*;
#[cfg(desktop)]
use crate::CandidateKey;
#[cfg(desktop)]
use std::path::Path;

/// The skipped candidates' part of the library's schema.
pub(super) const SCHEMA: bae_db::SchemaPart =
    bae_db::SchemaPart::new(&[include_str!("skipped_candidates.sql")]);

/// Forget every candidate set aside at or below `folders`.
#[cfg(desktop)]
pub(super) fn forget_at_or_below(
    sql: &CovenWrite<'_>,
    folders: &super::watched_folders::FoldersAtOrBelow,
) -> Result<(), DbError> {
    sql.execute(
        &format!(
            "DELETE FROM skipped_import_candidates WHERE {}",
            super::watched_folders::FoldersAtOrBelow::condition("candidate_path")
        ),
        &folders.params(),
    )?;
    Ok(())
}

#[cfg(desktop)]
impl ImportTables {
    /// Every skipped candidate under one root, by its path below the root:
    /// what the tests of skipping and of removing a watched folder read back.
    /// A stored path that is not canonical fails the read.
    #[cfg(any(test, feature = "test-utils"))]
    pub async fn load_skipped_import_candidates(
        &self,
        watched_folder_path: &Path,
    ) -> Result<std::collections::HashSet<bae_db::RelativeFolder>, DbError> {
        use super::watched_folders::relative_below;
        use std::collections::HashSet;
        use std::path::PathBuf;

        let (below_first, below_past) = bae_db::watched_path::paths_below(watched_folder_path);
        let paths: Vec<PathBuf> = self
            .database
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
            bae_db::watched_path::validate_stored_folder(path)?;
            below.insert(relative_below(watched_folder_path, path)?);
        }
        Ok(below)
    }

    /// Set the candidate whose folder on disk is `candidate_path` aside, or
    /// back; `false` when it already was. Setting it aside ends its
    /// identification in the same write.
    pub async fn set_import_candidate_skipped(
        &self,
        candidate_path: &Path,
        skipped: bool,
    ) -> Result<bool, DbError> {
        bae_db::watched_path::validate_stored_folder(candidate_path)?;
        let candidate_path = candidate_path.to_path_buf();
        // The store refuses a write that changes nothing, so read first.
        let stored = {
            let candidate_path = coven::path_bytes(&candidate_path);
            self.database
                .read(move |sql| {
                    Ok(sql
                        .query_row(
                            "SELECT 1 FROM skipped_import_candidates WHERE candidate_path = ?",
                            [candidate_path],
                            |_| Ok(()),
                        )
                        .optional()?
                        .is_some())
                })
                .await?
        };
        if stored == skipped {
            return Ok(false);
        }
        self.database
            .call(move |sql| {
                let changed = if skipped {
                    require_watched(sql, &candidate_path)?;
                    // Set aside, the candidate is not identification's to answer:
                    // its identification ends with the skip.
                    change_identification_on(
                        sql,
                        &CandidateKey::folder(&candidate_path),
                        &crate::candidate_runtime::IdentificationChange::Ended,
                    )?;
                    sql.execute(
                        "INSERT INTO skipped_import_candidates (candidate_path) VALUES (?) \
                     ON CONFLICT DO NOTHING",
                        [coven::path_bytes(&candidate_path)],
                    )?
                } else {
                    sql.execute(
                        "DELETE FROM skipped_import_candidates WHERE candidate_path = ?",
                        [coven::path_bytes(&candidate_path)],
                    )?
                };
                Ok(changed == 1)
            })
            .await
    }
}

#[cfg(test)]
#[path = "skipped_candidates_tests.rs"]
mod tests;
