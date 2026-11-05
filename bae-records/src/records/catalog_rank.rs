//! The SQL function that ranks a stored catalog.

use rusqlite::types::Value;

use bae_store::SchemaFunction;

/// `bae_catalog_rank`: a stored catalog's place in the order surfaces list
/// catalogs (`Catalog::ALL`). Part of the schema once an index uses it: the
/// order must not change.
pub(crate) static CATALOG_RANK: SchemaFunction = SchemaFunction {
    name: "bae_catalog_rank",
    arguments: 1,
    version: 1,
    compute: |arguments| {
        let catalog: bae_catalogs::catalog::Catalog = arguments
            .get::<String>(0)?
            .parse()
            .map_err(|error: String| rusqlite::Error::UserFunctionError(error.into()))?;
        Ok(Value::Integer(catalog.rank() as i64))
    },
};
