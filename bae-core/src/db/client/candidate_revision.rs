//! Nonreused versions of candidate facts, allocated inside their write transaction.

use super::*;

pub(super) fn allocate(sql: &SqlContext<'_, '_>) -> Result<i64, DbError> {
    sql.query_row(
        "UPDATE import_candidate_revision SET last_revision = last_revision + 1 \
         WHERE singleton = 1 AND last_revision < 9223372036854775807 \
         RETURNING last_revision",
        [],
        |row| row.get(0),
    )
    .optional()?
    .ok_or_else(|| DbError::Message("candidate revision allocator is missing or exhausted".into()))
}

/// Failure persistence belongs to the exact preparation an attempt accepted.
pub(super) fn require_current(
    sql: &SqlContext<'_, '_>,
    read: &crate::import::CandidateAsRead,
) -> Result<(), DbError> {
    let edit = i64::try_from(read.file_edit_revision).map_err(|_| {
        DbError::Message("candidate file revision exceeds SQLite's integer range".into())
    })?;
    let metadata = i64::try_from(read.metadata_revision).map_err(|_| {
        DbError::Message("candidate metadata revision exceeds SQLite's integer range".into())
    })?;
    let current = sql.query_row(
        "SELECT 1 FROM import_candidate_state WHERE content_hash = ? AND edit_revision = ? AND metadata_revision = ?",
        params![read.content_hash, edit, metadata],
        |_| Ok(()),
    ).optional()?;
    current.ok_or_else(|| {
        DbError::Message(
            "candidate file decisions or metadata changed before its import state was stored"
                .into(),
        )
    })
}
