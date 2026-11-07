//! Persisted terminal import failures.
//! Its table is `failure_rows.sql`, which every device's library holds.

pub(super) const SQL: &str = include_str!("failure_rows.sql");

use super::*;
use crate::{ImportFailure, ImportFailureReason};

/// The diagnostic column read by both the list and the candidate pane.
pub(crate) const FAILURE_REASON_COLUMNS: &str = "failure.error AS failure_error";

pub(crate) fn failure_reason_from_row(row: &Row<'_>) -> rusqlite::Result<ImportFailureReason> {
    Ok(ImportFailureReason { detail: row.get("failure_error")? })
}

/// The failure the last import of `content_hash` left.
pub(super) fn load_failure_on(
    sql: &SqlReadContext<'_>,
    content_hash: &str,
) -> Result<Option<ImportFailure>, DbError> {
    sql.query_row(
        &format!(
            "SELECT {FAILURE_REASON_COLUMNS}, failure.failed_at \
             FROM import_candidate_failure failure WHERE failure.content_hash = ?"
        ),
        [content_hash],
        |row| {
            Ok(ImportFailure {
                reason: failure_reason_from_row(row)?,
                failed_at: rfc3339_column(row, "failed_at")?,
            })
        },
    )
    .optional()
    .map_err(DbError::from)
}

/// Drop the failure the last import of `content_hash` left.
pub(crate) fn delete_failure_on(sql: &CovenWrite<'_>, content_hash: &str) -> Result<(), DbError> {
    sql.execute("DELETE FROM import_candidate_failure WHERE content_hash = ?", [content_hash])?;
    Ok(())
}

/// Record that an import of `content_hash` at `edit_revision` failed, inside
/// the caller's write.
fn save_failure_on(
    sql: &CovenWrite<'_>,
    content_hash: &str,
    edit_revision: u64,
    failure: &ImportFailure,
) -> Result<(), crate::ImportError> {
    let current_revision = sql
        .query_row(
            "SELECT edit_revision FROM import_candidate_state WHERE content_hash = ?",
            [content_hash],
            |row| row.get::<_, i64>(0),
        )
        .optional()?
        .ok_or_else(|| CandidateStateRefusal::NoState(content_hash.to_string()))?;
    if current_revision != to_i64(edit_revision, "candidate edit revision")? {
        return Err(CandidateStateRefusal::FilesChanged {
            content_hash: content_hash.to_string(),
            revision: edit_revision,
        }
        .into());
    }
    let failed_at = failure.failed_at.to_rfc3339();
    sql.execute(
        "INSERT INTO import_candidate_failure \
             (content_hash, error, failed_at) \
         VALUES (?, ?, ?) \
         ON CONFLICT (content_hash) DO UPDATE SET \
             error = excluded.error, failed_at = excluded.failed_at",
        params![content_hash, failure.reason.detail, failed_at],
    )?;
    Ok(())
}

impl ImportTables {
    /// Record that an import of this candidate failed, so the pane still
    /// offers Retry after a relaunch.
    pub async fn save_import_candidate_failure(
        &self,
        content_hash: &str,
        edit_revision: u64,
        failure: &ImportFailure,
    ) -> Result<(), crate::ImportError> {
        let content_hash = content_hash.to_string();
        let failure = failure.clone();
        self.store
            .write(move |sql| save_failure_on(&sql, &content_hash, edit_revision, &failure))
            .await
    }

    /// Record that the import `import_id` of the candidate at
    /// `candidate_key` failed, and end its claim on the candidate, in one
    /// write: no read sees the import ended without its failure, or the
    /// failure beside the import still going.
    pub(crate) async fn fail_candidate_import(
        &self,
        content_hash: &str,
        edit_revision: u64,
        failure: &ImportFailure,
        candidate_key: &crate::CandidateKey,
        import_id: &str,
    ) -> Result<(), crate::ImportError> {
        let content_hash = content_hash.to_string();
        let failure = failure.clone();
        let candidate_key = candidate_key.clone();
        let import_id = import_id.to_string();
        self.store
            .write(move |sql| {
                let sql = &sql;
                save_failure_on(sql, &content_hash, edit_revision, &failure)?;
                // The failing import is the worker's, which holds its claim
                // until this write ends it.
                let ended = super::move_import_on(sql, &candidate_key, &import_id, None)?;
                assert!(ended, "the failing import {import_id} does not own {candidate_key}");
                Ok(())
            })
            .await
    }
}

#[cfg(test)]
#[path = "failure_rows_tests.rs"]
mod tests;
