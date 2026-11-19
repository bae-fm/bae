//! The SQL function that says whether a length a source lists for a track
//! disagrees with the length of its audio.

use rusqlite::types::Value;

use bae_store::SchemaFunction;

/// `bae_lengths_disagree(a_ms, b_ms)`: whether two lengths are further apart
/// than the tolerance the import pane and the release editor mark a listed
/// length by (`medium_coverage::lengths_disagree`).
pub(crate) static LENGTHS_DISAGREE: SchemaFunction = SchemaFunction {
    name: "bae_lengths_disagree",
    arguments: 2,
    version: 1,
    compute: |arguments| {
        let length = |at| {
            u64::try_from(arguments.get::<i64>(at)?)
                .map_err(|error| rusqlite::Error::UserFunctionError(error.into()))
        };
        Ok(Value::Integer(
            bae_catalogs::medium_coverage::lengths_disagree(length(0)?, length(1)?).into(),
        ))
    },
};
