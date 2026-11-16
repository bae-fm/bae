//! SQL functions that compute an artist credit's name key and catalog id with
//! the rules the write code uses, for reads that apply the same rule in SQL.

use rusqlite::types::Value;

use bae_store::SchemaFunction;

/// `bae_artist_name_key(name)`: the folded name that lookups match an artist
/// on (`bae_types::text_match::artist_name_key`); NULL stays NULL. Once an
/// index or trigger uses this function, it is part of the schema: what it
/// folds must not change.
pub(crate) static ARTIST_NAME_KEY: SchemaFunction = SchemaFunction {
    name: "bae_artist_name_key",
    arguments: 1,
    version: 1,
    compute: |arguments| {
        Ok(match arguments.get::<Option<String>>(0)? {
            Some(name) => Value::Text(bae_types::text_match::artist_name_key(&name)),
            None => Value::Null,
        })
    },
};

/// `bae_artist_identity(musicbrainz_id, discogs_id)`: the bae artist id
/// derived from the MusicBrainz id, else from the Discogs id
/// (`crate::identity::artist_id`); NULL when neither id is given.
pub(crate) static ARTIST_IDENTITY: SchemaFunction = SchemaFunction {
    name: "bae_artist_identity",
    arguments: 2,
    version: 1,
    compute: |arguments| {
        let musicbrainz = arguments.get::<Option<String>>(0)?;
        let discogs = arguments.get::<Option<String>>(1)?;
        Ok(match crate::identity::artist_id(musicbrainz.as_deref(), discogs.as_deref()) {
            Some(id) => Value::Text(id),
            None => Value::Null,
        })
    },
};
