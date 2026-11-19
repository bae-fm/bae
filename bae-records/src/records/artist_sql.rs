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

/// `bae_artist_is_various_artists(musicbrainz_id, discogs_id)`: 1 when either
/// id is its catalog's Various Artists entry (`crate::VARIOUS_ARTISTS`),
/// which the rule takes as one artist in both catalogs; else 0.
pub(crate) static ARTIST_IS_VARIOUS_ARTISTS: SchemaFunction = SchemaFunction {
    name: "bae_artist_is_various_artists",
    arguments: 2,
    version: 1,
    compute: |arguments| {
        let musicbrainz = arguments.get::<Option<String>>(0)?;
        let discogs = arguments.get::<Option<String>>(1)?;
        Ok(Value::Integer(i64::from(
            crate::VARIOUS_ARTISTS.named_by(musicbrainz.as_deref(), discogs.as_deref()),
        )))
    },
};
