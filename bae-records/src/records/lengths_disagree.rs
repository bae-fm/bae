//! The SQL function that says whether a length a source lists for a track
//! disagrees with the length of its audio.

use rusqlite::types::Value;

/// `bae_lengths_disagree(a_ms, b_ms)`: whether two lengths are further apart
/// than the tolerance the import pane and the release editor mark a listed
/// length by (`bae_catalogs::track_lengths::lengths_disagree`).
pub(crate) fn lengths_disagree(
    arguments: &bae_store::SqlArguments<'_>,
) -> rusqlite::Result<rusqlite::types::Value> {
    let length = |at| {
        u64::try_from(arguments.get::<i64>(at)?)
            .map_err(|error| rusqlite::Error::UserFunctionError(error.into()))
    };
    Ok(Value::Integer(bae_catalogs::track_lengths::lengths_disagree(length(0)?, length(1)?).into()))
}
