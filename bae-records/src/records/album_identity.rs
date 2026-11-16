//! The SQL function that names the library album of a catalog's album.

use rusqlite::types::Value;

use bae_store::SchemaFunction;

/// `bae_album_id` takes a catalog's name, as stored, and the key of the group
/// that the catalog files releases under. It returns the library album id
/// that [`crate::identity::album_id`] computes for them.
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
