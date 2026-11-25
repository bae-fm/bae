//! The SQL function that trims a title the way the code that writes it
//! does, so the import pane's count of untitled tracks and the writes share
//! one definition of an empty title.

use rusqlite::types::Value;

/// `bae_trim(text)`: the text without the whitespace `str::trim` removes.
pub(crate) fn trim(
    arguments: &bae_store::SqlArguments<'_>,
) -> rusqlite::Result<rusqlite::types::Value> {
    Ok(Value::Text(arguments.get::<String>(0)?.trim().to_owned()))
}
