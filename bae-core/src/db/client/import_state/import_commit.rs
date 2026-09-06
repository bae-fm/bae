//! The candidate a library import commits, read inside the import's own
//! transaction so the commit and the check that it still describes the
//! prepared candidate are one.

use super::*;
use crate::import::preparation::CommittingCandidate;

/// Refuse the import unless the candidate is still exactly what the import
/// was prepared from. The rows are read here; what must match is
/// [`ImportExpectation::verify`](crate::import::service::ImportExpectation::verify)'s
/// to say.
pub(crate) fn require_import_commit_guard(
    sql: &SqlContext<'_, '_>,
    guard: &ImportCommitGuard,
) -> Result<(), DbError> {
    match guard {
        ImportCommitGuard::Candidate {
            candidate_key,
            source,
            expectation,
        } => {
            let current = load_committing_candidate(sql, candidate_key, expectation)?;
            expectation
                .verify(candidate_key, source, current.as_ref())
                .map_err(DbError::Message)
        }
        #[cfg(test)]
        ImportCommitGuard::UncheckedTestSetup => Ok(()),
    }
}

/// The stored candidate at `candidate_key` as the commit sees it, or `None`
/// when the scan no longer lists one there.
fn load_committing_candidate(
    sql: &SqlContext<'_, '_>,
    candidate_key: &str,
    expectation: &crate::import::service::ImportExpectation,
) -> Result<Option<CommittingCandidate>, DbError> {
    let Some(stored) = super::import_combinations::load_candidate_on(sql, candidate_key)? else {
        return Ok(None);
    };
    let candidate = stored.candidate;
    let revisions: Option<(i64, i64)> = sql
        .query_row(
            "SELECT edit_revision, metadata_revision FROM import_candidate_state \
             WHERE content_hash = ?",
            [expectation.content_hash()],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .optional()?;
    let prepared_revisions = revisions
        .map(|(edit, metadata)| {
            Ok::<_, DbError>((
                u64::try_from(edit)
                    .map_err(|_| DbError::Message("candidate file revision is negative".into()))?,
                u64::try_from(metadata).map_err(|_| {
                    DbError::Message("candidate metadata revision is negative".into())
                })?,
            ))
        })
        .transpose()?;
    let file_tag_snapshot = super::folder_scans::load_candidate_file_tag_snapshot(
        sql,
        candidate.watched_folder_path(),
        candidate_key,
    )?
    .and_then(|stored| stored.snapshot);
    Ok(Some(CommittingCandidate {
        actionable: stored.actionable,
        source: candidate.source(),
        content_hash: candidate.files().content_hash(),
        file_edit_revision: candidate.file_edit_revision(),
        prepared_revisions,
        file_tag_snapshot,
    }))
}
