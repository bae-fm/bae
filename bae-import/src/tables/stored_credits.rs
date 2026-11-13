//! A credit line as a table stores it: one JSON list text, a `{name, join}`
//! per credit, the join as its source stated it.

use super::DbError;
use bae_types::credits::{CreditLine, StoredCredit};

/// The text a credit line is stored as.
pub(crate) fn line_json(line: &CreditLine) -> Result<String, DbError> {
    serde_json::to_string(&line.to_stored_credits())
        .map_err(|error| DbError::encode("a credit line", error))
}

/// The line a column of stored credits holds, `None` for NULL.
pub(crate) fn line_column(
    row: &rusqlite::Row<'_>,
    column: &str,
) -> rusqlite::Result<Option<CreditLine>> {
    let Some(json) = row.get::<_, Option<String>>(column)? else {
        return Ok(None);
    };
    serde_json::from_str::<Vec<StoredCredit>>(&json)
        .map(|credits| Some(CreditLine::from_stored_credits(&credits)))
        .map_err(|error| bae_store::read::column_conversion_error(row, column, error))
}
