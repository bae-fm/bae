//! The SQL functions that state an artist credit's name and catalog identity
//! the way writes decide them, for reads that apply the same rule in SQL.

use rusqlite::types::Value;

use bae_store::SchemaFunction;

/// `bae_artist_name_key(name)`: the folded name an artist is found by
/// (`bae_types::text_match::artist_name_key`); NULL stays NULL. Part of the
/// schema once an index or trigger uses it: what it folds must not change.
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

/// `bae_artist_identity(musicbrainz_id, discogs_id)`: the id the artist a
/// catalog names takes (`crate::identity::artist_id`), preferring MusicBrainz;
/// NULL when neither id is given.
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
