//! The SQL functions that state facts of one stored value the import pane's
//! track table shows: how a side is lettered, whether a stored carrier is
//! played side by side, whether two lengths disagree, and a title's trim.
//! Each wraps the Rust rule, so a statement and the code that writes the
//! value share one definition.

use rusqlite::types::Value;

use bae_store::SchemaFunction;

fn integer(arguments: &bae_store::SqlArguments<'_>, at: usize) -> rusqlite::Result<i64> {
    arguments.get::<i64>(at)
}

/// `bae_side_letters(n)`: the letters of the `n`th side of a release's sided
/// mediums, counted from one (`bae_types::mediums::side_letters`).
pub(crate) static SIDE_LETTERS: SchemaFunction = SchemaFunction {
    name: "bae_side_letters",
    arguments: 1,
    version: 1,
    compute: |arguments| {
        let n = u32::try_from(integer(arguments, 0)?)
            .ok()
            .filter(|n| *n >= 1)
            .ok_or_else(|| rusqlite::Error::UserFunctionError("sides count from one".into()))?;
        Ok(Value::Text(bae_types::mediums::side_letters(n)))
    },
};

/// `bae_medium_is_sided(carrier)`: whether a stored carrier key names a medium
/// played side by side; false for no carrier.
pub(crate) static MEDIUM_IS_SIDED: SchemaFunction = SchemaFunction {
    name: "bae_medium_is_sided",
    arguments: 1,
    version: 1,
    compute: |arguments| {
        let carrier = arguments.get::<Option<String>>(0)?;
        let sided = carrier
            .as_deref()
            .map(|key| {
                bae_types::pressing::Medium::from_key(key).ok_or_else(|| {
                    rusqlite::Error::UserFunctionError(format!("unknown carrier {key}").into())
                })
            })
            .transpose()?
            .is_some_and(bae_types::pressing::Medium::is_sided);
        Ok(Value::Integer(sided.into()))
    },
};

/// `bae_lengths_disagree(a_ms, b_ms)`: whether two lengths are further apart
/// than the pane's tolerance (`medium_coverage::lengths_disagree`).
pub(crate) static LENGTHS_DISAGREE: SchemaFunction = SchemaFunction {
    name: "bae_lengths_disagree",
    arguments: 2,
    version: 1,
    compute: |arguments| {
        let length = |at| {
            u64::try_from(integer(arguments, at)?)
                .map_err(|error| rusqlite::Error::UserFunctionError(error.into()))
        };
        Ok(Value::Integer(
            bae_catalogs::medium_coverage::lengths_disagree(length(0)?, length(1)?).into(),
        ))
    },
};

/// `bae_trim(text)`: the text without the whitespace `str::trim` removes.
pub(crate) static TRIM: SchemaFunction = SchemaFunction {
    name: "bae_trim",
    arguments: 1,
    version: 1,
    compute: |arguments| Ok(Value::Text(arguments.get::<String>(0)?.trim().to_owned())),
};
