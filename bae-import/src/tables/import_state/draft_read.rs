//! The stored draft of the candidate that holds a content hash, read from the
//! fragment the pane spreads for the candidate its key names.

use super::fragments::DraftRows;
use crate::CandidateDraft;
use bae_store::{statement_optional, DbError};

sqlive_compose::read!(
    StoredDraft(candidate: String),
    r#"
    SELECT draft.*
    FROM @crate::tables::import_state::fragments::DraftRows(
        candidate: (SELECT :candidate AS content_hash)) AS draft
"#
);

/// The complete stored draft of the candidate `content_hash`, its lists in
/// their stored order; `None` when the candidate holds no draft row.
pub(crate) fn load_draft_on(
    sql: &impl coven::SqlRead,
    content_hash: &str,
) -> Result<Option<CandidateDraft>, DbError> {
    let request = StoredDraftParams { candidate: content_hash.to_string() };
    // The candidate holds at most one draft row.
    let row: Option<DraftRows> = statement_optional(sql, &request)?.map(|row| row.draft);
    row.as_ref().map(super::pane_rows::draft_of).transpose()
}
