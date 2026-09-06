use super::*;

#[cfg(not(any(target_os = "ios", target_os = "android")))]
pub(super) fn require_import_commit_guard(
    sql: &SqlContext<'_, '_>,
    guard: &ImportCommitGuard,
) -> Result<(), DbError> {
    match guard {
        ImportCommitGuard::Candidate {
            candidate_key,
            source,
            expectation,
        } => {
            let Some(stored) = super::import_combinations::load_candidate_on(sql, candidate_key)?
            else {
                return Err(DbError::Message(format!(
                    "{candidate_key} is no longer a valid import candidate"
                )));
            };
            let candidate = stored.candidate;
            if !stored.actionable
                || candidate.source() != *source
                || candidate.files().content_hash() != expectation.content_hash()
                || candidate.file_edit_revision() != expectation.edit_revision()
            {
                return Err(DbError::Message(format!(
                    "{candidate_key} changed before its import committed"
                )));
            }

            let revisions: Option<(i64, i64)> = sql
                .query_row(
                    "SELECT edit_revision, metadata_revision FROM import_candidate_state \
                     WHERE content_hash = ?",
                    [expectation.content_hash()],
                    |row| Ok((row.get(0)?, row.get(1)?)),
                )
                .optional()?;
            let expected_edit_revision =
                i64::try_from(expectation.edit_revision()).map_err(|_| {
                    DbError::Message(
                        "candidate file revision exceeds SQLite's integer range".into(),
                    )
                })?;
            let expected_metadata_revision = i64::try_from(expectation.metadata_revision())
                .map_err(|_| {
                    DbError::Message(
                        "candidate metadata revision exceeds SQLite's integer range".into(),
                    )
                })?;
            if revisions != Some((expected_edit_revision, expected_metadata_revision)) {
                return Err(DbError::Message(format!(
                    "{candidate_key}'s prepared metadata changed before its import committed"
                )));
            }

            if let Some(snapshot) = &expectation.file_tag_snapshot {
                let stored = super::folder_scans::load_candidate_file_tag_snapshot(
                    sql,
                    candidate.watched_folder_path(),
                    candidate_key,
                )?
                .and_then(|stored| stored.snapshot);
                if stored.as_ref() != Some(snapshot) {
                    return Err(DbError::Message(format!(
                        "{candidate_key}'s file-tag reading changed before its import committed"
                    )));
                }
            }
            Ok(())
        }
        #[cfg(test)]
        ImportCommitGuard::UncheckedTestSetup => Ok(()),
    }
}
