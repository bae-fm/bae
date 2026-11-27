//! A credit line as a table stores it: one JSON list text, a `{name, join}`
//! per credit, the join as its source stated it.

use super::DbError;
use bae_types::credits::{CreditLine, StoredCredit};

/// The text a credit line is stored as.
pub(crate) fn line_json(line: &CreditLine) -> Result<String, DbError> {
    serde_json::to_string(&line.to_stored_credits())
        .map_err(|error| DbError::encode("a credit line", error))
}

/// The credit line in stored credits text, or `None` when there is no text.
pub(crate) fn line_of(json: Option<&str>) -> Result<Option<CreditLine>, serde_json::Error> {
    json.map(line_text).transpose()
}

/// The credit line in a stored credits text.
pub(crate) fn line_text(json: &str) -> Result<CreditLine, serde_json::Error> {
    serde_json::from_str::<Vec<StoredCredit>>(json)
        .map(|credits| CreditLine::from_stored_credits(&credits))
}
