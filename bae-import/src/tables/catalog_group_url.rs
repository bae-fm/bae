//! The SQL function that names the page of an album on its catalog.

use rusqlite::types::Value;

use bae_store::SchemaFunction;

/// `bae_catalog_group_url`: the editorial page of the album a catalog files
/// under the group key in its second argument (`Catalog::group_url`). The
/// function returns NULL when the catalog has no album page or the group key
/// is NULL. The statements that show a card's albums call this function, so
/// bae defines each page address in one place.
pub(crate) static CATALOG_GROUP_URL: SchemaFunction = SchemaFunction {
    name: "bae_catalog_group_url",
    arguments: 2,
    version: 1,
    compute: |arguments| {
        let catalog: bae_catalogs::catalog::Catalog = arguments
            .get::<String>(0)?
            .parse()
            .map_err(|error: String| rusqlite::Error::UserFunctionError(error.into()))?;
        let Some(group) = arguments.get::<Option<String>>(1)? else {
            return Ok(Value::Null);
        };
        Ok(catalog.group_url(&group).map_or(Value::Null, Value::Text))
    },
};
