//! The imports verdicts owe: what an automatic run settled on as needing
//! nothing, while "Import automatically when identified" was on, until an
//! import attempt for it ends or the decision not to import it is made.
//!
//! A row is written only by the verdict's own save (see
//! [`super::CandidateSaveExtras::owes_import`]), so nothing settled before the
//! setting was on, and nothing a person's run settled, ever owes one.

use super::*;

/// Record that `content_hash`'s verdict owes an import, for the draft at
/// `metadata_revision`. Called inside the save that stores the verdict.
pub(super) fn owe_import_on(
    sql: &SqlContext<'_, '_>,
    content_hash: &str,
    metadata_revision: i64,
) -> Result<(), DbError> {
    sql.execute(
        "INSERT INTO import_candidate_owed_import (content_hash, metadata_revision) \
         VALUES (?, ?)",
        params![content_hash, metadata_revision],
    )?;
    Ok(())
}

/// End what `content_hash` owed, inside the transaction that ends an import
/// attempt for it: the release's commit or the failure it records. An attempt
/// is the answer to what was owed whichever way it went, so a failed one is
/// not retried on its own.
pub(super) fn end_owed_import_on(
    sql: &SqlContext<'_, '_>,
    content_hash: &str,
) -> Result<(), DbError> {
    sql.execute(
        "DELETE FROM import_candidate_owed_import WHERE content_hash = ?",
        [content_hash],
    )?;
    Ok(())
}

impl Database {
    /// The draft revision `content_hash`'s verdict owes an import for, or
    /// `None` when it owes none.
    pub(crate) async fn load_owed_import(&self, content_hash: &str) -> Result<Option<u64>, DbError> {
        let content_hash = content_hash.to_string();
        self.read(move |sql| {
            let revision: Option<i64> = sql
                .query_row(
                    "SELECT metadata_revision FROM import_candidate_owed_import \
                     WHERE content_hash = ?",
                    [&content_hash],
                    |row| row.get(0),
                )
                .optional()?;
            revision
                .map(|revision| {
                    u64::try_from(revision).map_err(|_| {
                        DbError::Message(format!(
                            "owed import revision {revision} of {content_hash} is negative"
                        ))
                    })
                })
                .transpose()
        })
        .await
    }

    /// Every content hash whose verdict owes an import.
    pub(crate) async fn load_owed_imports(&self) -> Result<HashSet<String>, DbError> {
        self.read(move |sql| {
            Ok(sql
                .query(
                    "SELECT content_hash FROM import_candidate_owed_import",
                    [],
                    |row| row.get::<_, String>(0),
                )?
                .into_iter()
                .collect())
        })
        .await
    }

    /// Decide not to import what `content_hash` owed.
    pub(crate) async fn withdraw_owed_import(&self, content_hash: &str) -> Result<(), DbError> {
        let content_hash = content_hash.to_string();
        self.call(move |sql| end_owed_import_on(sql, &content_hash))
            .await
    }

    /// Record that `content_hash`'s verdict owes an import for the draft at
    /// `metadata_revision`, outside the verdict's own save — the state the app
    /// leaves when it closes between storing a verdict and starting its
    /// import, for a test to start from.
    #[cfg(test)]
    pub(crate) async fn owe_import_for_test(
        &self,
        content_hash: &str,
        metadata_revision: u64,
    ) -> Result<(), DbError> {
        let content_hash = content_hash.to_string();
        let revision = i64::try_from(metadata_revision)
            .map_err(|_| DbError::Message("owed revision exceeds SQLite's range".into()))?;
        self.call(move |sql| owe_import_on(sql, &content_hash, revision))
            .await
    }
}
