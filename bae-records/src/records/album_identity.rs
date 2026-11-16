//! The SQL function that names the library album of a catalog's album.

use rusqlite::types::Value;

use bae_store::SchemaFunction;

/// `bae_album_id`: the library album of the group a catalog (its first
/// argument, as stored) files releases under by a key (its second): the id
/// [`crate::identity::album_id`] gives it.
pub(crate) static ALBUM_IDENTITY: SchemaFunction = SchemaFunction {
    name: "bae_album_id",
    arguments: 2,
    version: 1,
    compute: |arguments| {
        let catalog: bae_catalogs::catalog::Catalog = arguments
            .get::<String>(0)?
            .parse()
            .map_err(|error: String| rusqlite::Error::UserFunctionError(error.into()))?;
        let key = arguments.get::<String>(1)?;
        Ok(Value::Text(crate::identity::album_id(catalog, &key)))
    },
};
