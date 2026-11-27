//! Persisted terminal import failures.
//! Its table is `failure_rows.sql`, which every device's library holds.

use super::*;
use crate::{ImportFailure, ImportFailureReason};
use bae_store::WriteStatements as _;
use bae_util::timestamp::StoredTimestamp as _;

/// The failure the last import of `candidate` left.
#[cfg(any(test, feature = "test-utils", debug_assertions))]
pub(super) fn load_failure_on(
    sql: &SqlReadContext<'_>,
    candidate: OneCandidate<'_>,
) -> Result<Option<ImportFailure>, DbError> {
    let filter = candidate.filter("failure.content_hash");
    sql.query_row(
        &format!(
            "SELECT failure.error, failure.failed_at \
             FROM import_candidate_failure failure {}",
            filter.clause
        ),
        filter.params().as_slice(),
        |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?)),
    )
    .optional()?
    .map(|(error, failed_at)| failure_of(error, &failed_at))
    .transpose()
}

/// The failure that the columns `error` and `failed_at` of
/// `import_candidate_failure` hold.
pub(crate) fn failure_of(error: String, failed_at: &str) -> Result<ImportFailure, DbError> {
    Ok(ImportFailure {
        reason: ImportFailureReason { detail: error },
        failed_at: bae_util::timestamp::parse_rfc3339(failed_at).map_err(|parse_error| {
            DbError::unreadable(
                "import_candidate_failure.failed_at",
                format!("{failed_at}: {parse_error}"),
            )
        })?,
    })
}

sqlive_compose::write!(
    DeleteImportFailure(content_hash: &'p str),
    r#"
    DELETE FROM import_candidate_failure WHERE content_hash = :content_hash
"#
);

sqlive_compose::write!(
    UpsertImportFailure(content_hash: &'p str, error: &'p str, failed_at: String),
    r#"
    INSERT INTO import_candidate_failure (content_hash, error, failed_at)
    VALUES (:content_hash, :error, :failed_at)
    ON CONFLICT (content_hash) DO UPDATE SET
        error = excluded.error, failed_at = excluded.failed_at
"#
);

/// Drop the failure the last import of `content_hash` left.
pub(crate) fn delete_failure_on(sql: &CovenWrite<'_>, content_hash: &str) -> Result<(), DbError> {
    sql.run(&DeleteImportFailureParams { content_hash })?;
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
    sql.run(&UpsertImportFailureParams { content_hash, error: &failure.reason.detail, failed_at })?;
    Ok(())
}

impl ImportTables {
    /// The failure the last import of the candidate `content_hash` left,
    /// read apart from the candidate's other rows, which a test may damage.
    #[cfg(test)]
    pub(crate) async fn load_import_failure(
        &self,
        content_hash: &str,
    ) -> Result<Option<ImportFailure>, DbError> {
        let content_hash = content_hash.to_string();
        self.store.read(move |sql| load_failure_on(&sql, OneCandidate::Hash(&content_hash))).await
    }

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
