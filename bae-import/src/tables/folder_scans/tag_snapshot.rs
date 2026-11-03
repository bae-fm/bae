//! A candidate's file-tag snapshot: the tags read from its audio files and the
//! cover embedded in them, stored against the scan generation and file edit
//! revision it was read at, and replaced only while both still stand.

use super::*;

/// Replace a candidate's file-tag snapshot, inside the caller's transaction,
/// if its scan generation and file edit revision still match the snapshot's.
/// `false` means the candidate changed first and nothing was written.
pub(crate) fn replace_candidate_file_tag_snapshot_on(
    sql: &CovenWrite<'_>,
    watched_folder_path: &Path,
    candidate_path: &CandidateKey,
    snapshot: &crate::file_tag_snapshot::FileTagSnapshot,
) -> Result<bool, DbError> {
    let expected_generation = generation_column(snapshot.scan_generation)?;
    let expected_file_edit_revision =
        to_i64(snapshot.file_edit_revision, "a file-tag snapshot's file edit revision")?;
    let matched = sql.execute(
        "UPDATE scan_candidate SET generation = generation \
         WHERE watched_folder_path = ? AND path = ? \
           AND generation = ? AND file_edit_revision = ?",
        params![
            coven::path_bytes(watched_folder_path),
            candidate_path,
            expected_generation,
            expected_file_edit_revision
        ],
    )?;
    if matched == 0 {
        return Ok(false);
    }

    let audio_files: Vec<(String, i64)> = sql.query(
        "SELECT relative_path, size FROM scan_candidate_file \
         WHERE watched_folder_path = ? AND candidate_path = ? AND role = 'audio' \
         ORDER BY position",
        params![coven::path_bytes(watched_folder_path), candidate_path],
        |row| Ok((row.get(0)?, row.get(1)?)),
    )?;
    if audio_files.len() != snapshot.files.len()
        || audio_files.iter().zip(&snapshot.files).any(|((relative_path, size), fact)| {
            relative_path != &fact.observation.relative_path
                || u64::try_from(*size).ok() != Some(fact.observation.size)
        })
    {
        return Err(DbError::Message(format!(
            "file-tag snapshot for {candidate_path} does not cover its current audio files"
        )));
    }
    if snapshot.embedded_cover.as_ref().is_some_and(|cover| {
        !snapshot
            .files
            .iter()
            .any(|fact| fact.observation.relative_path == cover.source_relative_path)
    }) {
        return Err(DbError::Message(format!(
            "file-tag snapshot for {candidate_path} names an embedded cover outside its audio files"
        )));
    }

    super::write::replace_candidate_file_tag_snapshot(
        sql,
        watched_folder_path,
        candidate_path,
        snapshot,
    )?;
    Ok(true)
}

impl ImportTables {
    /// The candidate's current scan generation and whatever file-tag snapshot
    /// is stored for it, even an outdated one, so a caller can tell a
    /// candidate never read from one whose reading is out of date.
    pub(crate) async fn load_candidate_file_tag_snapshot(
        &self,
        watched_folder_path: &Path,
        candidate_path: &CandidateKey,
    ) -> Result<Option<DbCandidateFileTagSnapshot>, DbError> {
        let watched_folder_path = watched_folder_path.to_path_buf();
        let candidate_path = candidate_path.clone();
        self.database
            .read(move |sql| {
                super::read::load_candidate_file_tag_snapshot(
                    &sql,
                    &watched_folder_path,
                    &candidate_path,
                )
            })
            .await
    }

    /// [`replace_candidate_file_tag_snapshot_on`] in a transaction of its own.
    #[cfg(test)]
    pub(crate) async fn replace_candidate_file_tag_snapshot(
        &self,
        watched_folder_path: &Path,
        candidate_path: &CandidateKey,
        snapshot: &crate::file_tag_snapshot::FileTagSnapshot,
    ) -> Result<bool, DbError> {
        let watched_folder_path = watched_folder_path.to_path_buf();
        let candidate_path = candidate_path.clone();
        let snapshot = snapshot.clone();
        self.database
            .call(move |sql| {
                replace_candidate_file_tag_snapshot_on(
                    sql,
                    &watched_folder_path,
                    &candidate_path,
                    &snapshot,
                )
            })
            .await
    }
}

#[cfg(test)]
#[path = "tag_snapshot_tests.rs"]
pub(crate) mod tests;
