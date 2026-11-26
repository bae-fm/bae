//! How the artist fields of a release editor's form stand to the library: the
//! album's artists and each explicit track's own. No table holds the form, so
//! the read takes its credits as the JSON arrays that `assignment_rows` makes,
//! asks the credit rule for the artists each credit could be, and spreads the
//! standing fragment that the import pane spreads for a draft's credits.

use super::*;
use crate::tables::import_list::artist_field_of_json;

sqlive_compose::read!(
    FormArtistFields(album_artists: String, track_artists: String, part: i64),
    r#"
    WITH form_credits(part, json) AS (
        SELECT 0, :album_artists UNION ALL SELECT 1, :track_artists
    ),
    form_raw AS (
        SELECT credits.part AS part,
               CASE WHEN credits.part = 1 THEN j.value ->> 'track_id' END AS track_id,
               j.value ->> 'position' AS position,
               CASE WHEN library_artist.artist_id IS NOT NULL THEN 'picked' ELSE 'credit' END
                   AS assignment_kind,
               library_artist.artist_id AS artist_id,
               j.value ->> 'name' AS name, j.value ->> 'sort_name' AS sort_name,
               j.value ->> 'musicbrainz_artist_id' AS musicbrainz_artist_id,
               j.value ->> 'discogs_artist_id' AS discogs_artist_id,
               CASE WHEN library_artist.artist_id IS NOT NULL THEN j.value ->> 'credit_key' END
                   AS credit_key,
               CASE WHEN library_artist.artist_id IS NULL THEN j.value ->> 'credited_as' END
                   AS credited_as,
               j.value ->> 'join_phrase' AS join_phrase
        FROM form_credits AS credits
        JOIN json_each(credits.json) AS j
        LEFT JOIN @bae_records::records::spread_fragments::ArtistRow AS library_artist
            ON library_artist.artist_id = j.value ->> 'artist_id'
    )
    SELECT standing.*
    FROM @bae_records::records::artist_standing::ArtistFieldStanding(
        raw: (SELECT * FROM form_raw),
        credit_answer: (
            SELECT answer.name_key, answer.musicbrainz_artist_id, answer.discogs_artist_id,
                   answer.artists
            FROM @bae_records::records::credit_rule::CreditAnswers(credits: (
                SELECT DISTINCT bae_artist_name_key(name) AS name_key, musicbrainz_artist_id,
                       discogs_artist_id
                FROM form_raw WHERE assignment_kind = 'credit')) AS answer)) AS standing
    WHERE standing.part = :part
"#
);

/// The part of a form's artist fields that `:part` binds in
/// [`FORM_ARTIST_FIELDS`]: the album's artists.
pub(super) const ALBUM_PART: i64 = 0;
/// The part of an explicit track's own artists.
pub(super) const TRACK_PART: i64 = 1;

/// The album's artist field of the form whose album artists and explicit
/// tracks' artists are the JSON arrays `album_artists` and `track_artists`,
/// with how it stands to the library; `None` when the album has none.
pub(super) fn album_artist_field_on(
    sql: &impl coven::SqlRead,
    album_artists: &str,
    track_artists: &str,
) -> Result<Option<crate::ArtistField>, DbError> {
    let request = FormArtistFieldsParams {
        album_artists: album_artists.to_string(),
        track_artists: track_artists.to_string(),
        part: ALBUM_PART,
    };
    let rows =
        sql.query(FormArtistFields::SQL, request.params().as_slice(), FormArtistFieldsRow::read)?;
    // The album has at most one artist field.
    rows.into_iter()
        .next()
        .map(|row| artist_field_of_json(&row.standing.into_columns().2))
        .transpose()
}

/// The standing of the artist fields of one part of a form, as a table with
/// `part`, `track_id` and `field` for a statement to join, which binds
/// `:album_artists`, `:track_artists` and `:part`.
pub(super) const FORM_ARTIST_FIELDS: &str = FormArtistFields::SQL;
