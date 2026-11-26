//! SQL functions that compute an artist credit's name key and catalog id with
//! the rules the write code uses, for reads that apply the same rule in SQL.

use rusqlite::types::Value;

/// `bae_artist_name_key(name)`: the folded name that lookups match an artist
/// on (`bae_types::text_match::artist_name_key`); NULL stays NULL. Once an
/// index or trigger uses this function, it is part of the schema: what it
/// folds must not change.
pub(crate) fn artist_name_key(
    arguments: &bae_store::SqlArguments<'_>,
) -> rusqlite::Result<rusqlite::types::Value> {
    Ok(match arguments.get::<Option<String>>(0)? {
        Some(name) => Value::Text(bae_types::text_match::artist_name_key(&name)),
        None => Value::Null,
    })
}

/// `bae_artist_identity(musicbrainz_id, discogs_id)`: the bae artist id
/// derived from the MusicBrainz id, else from the Discogs id
/// (`crate::identity::artist_id`); NULL when neither id is given.
pub(crate) fn artist_identity(
    arguments: &bae_store::SqlArguments<'_>,
) -> rusqlite::Result<rusqlite::types::Value> {
    let musicbrainz = arguments.get::<Option<String>>(0)?;
    let discogs = arguments.get::<Option<String>>(1)?;
    Ok(match crate::identity::artist_id(musicbrainz.as_deref(), discogs.as_deref()) {
        Some(id) => Value::Text(id),
        None => Value::Null,
    })
}

/// `bae_artist_is_various_artists(musicbrainz_id, discogs_id)`: 1 when either
/// id is its catalog's Various Artists entry (`crate::VARIOUS_ARTISTS`),
/// which the rule takes as one artist in both catalogs; else 0.
pub(crate) fn artist_is_various_artists(
    arguments: &bae_store::SqlArguments<'_>,
) -> rusqlite::Result<rusqlite::types::Value> {
    let musicbrainz = arguments.get::<Option<String>>(0)?;
    let discogs = arguments.get::<Option<String>>(1)?;
    Ok(Value::Integer(i64::from(
        crate::VARIOUS_ARTISTS.named_by(musicbrainz.as_deref(), discogs.as_deref()),
    )))
}

/// `bae_various_artists_id(catalog)`: the id of the Various Artists entry of
/// the catalog `'musicbrainz'` or `'discogs'` (`crate::VARIOUS_ARTISTS`), the
/// one source that SQL reads those ids from.
pub(crate) fn various_artists_id(
    arguments: &bae_store::SqlArguments<'_>,
) -> rusqlite::Result<rusqlite::types::Value> {
    let catalog = arguments.get::<String>(0)?;
    match catalog.as_str() {
        "musicbrainz" => Ok(Value::Text(crate::VARIOUS_ARTISTS.musicbrainz.to_string())),
        "discogs" => Ok(Value::Text(crate::VARIOUS_ARTISTS.discogs.to_string())),
        other => Err(rusqlite::Error::UserFunctionError(
            format!("bae_various_artists_id knows no catalog {other}").into(),
        )),
    }
}
