//! The artist fields of a release editor's form as the library holds them:
//! the album's artists and each explicit track's own. No table holds the
//! form, so the reads take its credits as the JSON arrays that
//! `assignment_rows` makes. Each field's standing asks the credit rule for the
//! artists each credit could be, and spreads the standing fragment that the
//! import pane spreads for a draft's credits.

use super::*;
use crate::tables::import_list::artist_field_of_json;

// The artist fields of a form whose album artists and explicit tracks'
// artists are the JSON arrays `album_artists` (`part` 0) and `track_artists`
// (`part` 1, each field with its track's id), each field with how it stands to
// the library (`field`). A picked artist the library no longer holds is a
// credit of the values the form holds for it.
//
// A track's field also has its assignments in their order, as the JSON array
// [`assignments_of`](crate::tables::draft_rows::assignments_of) reads: each
// picked artist with its values as the library holds them, and its picked
// credit name while the library holds that one.
sqlive_compose::fragment!(
    FormArtistFields(album_artists: String, track_artists: String) {
        part: i64,
        track_id: Option<String>,
        field: String,
        assignments: Option<String>
    },
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
    ),
    standing AS (
        SELECT stood.*
        FROM @bae_records::records::artist_standing::ArtistFieldStanding(
            raw: (SELECT * FROM form_raw),
            credit_answer: (
                SELECT answer.name_key, answer.musicbrainz_artist_id, answer.discogs_artist_id,
                       answer.artists
                FROM @bae_records::records::credit_rule::CreditAnswers(credits: (
                    SELECT DISTINCT bae_artist_name_key(name) AS name_key,
                           musicbrainz_artist_id, discogs_artist_id
                    FROM form_raw WHERE assignment_kind = 'credit')) AS answer)) AS stood),
    own AS (
        SELECT assignment.track_id,
               json_group_array(json_object(
                   'kind', assignment.assignment_kind, 'artist_id', assignment.artist_id,
                   'name', CASE assignment.assignment_kind WHEN 'picked' THEN picked.name
                           ELSE assignment.name END,
                   'sort_name', CASE assignment.assignment_kind
                        WHEN 'picked' THEN picked.sort_name ELSE assignment.sort_name END,
                   'musicbrainz_artist_id', CASE assignment.assignment_kind
                        WHEN 'picked' THEN picked.musicbrainz_artist_id
                        ELSE assignment.musicbrainz_artist_id END,
                   'discogs_artist_id', CASE assignment.assignment_kind
                        WHEN 'picked' THEN picked.discogs_artist_id
                        ELSE assignment.discogs_artist_id END,
                   'credit_key', printed.name_key, 'credit_name', printed.name,
                   'credited_as', assignment.credited_as,
                   'join_phrase', assignment.join_phrase)
                   ORDER BY assignment.position) AS assignments
        FROM form_raw AS assignment
        LEFT JOIN @bae_records::records::spread_fragments::ArtistRow AS picked
               ON picked.artist_id = assignment.artist_id
        LEFT JOIN @bae_records::records::spread_fragments::ArtistCreditName AS printed
               ON printed.artist_id = assignment.artist_id
              AND printed.name_key = assignment.credit_key
        WHERE assignment.part = 1
        GROUP BY assignment.track_id
    )
    SELECT standing.part, standing.track_id, standing.field, own.assignments
    FROM standing
    LEFT JOIN own ON standing.part = 1 AND own.track_id = standing.track_id
"#
);

// The album's artist field: no row when the album has no artist.
sqlive_compose::read!(
    FormAlbumArtistField(album_artists: String, track_artists: String),
    r#"
    SELECT album.*
    FROM @crate::tables::release_edit::artist_fields::FormArtistFields(
        album_artists: :album_artists, track_artists: :track_artists) AS album
    WHERE album.part = 0
"#
);

/// The album's artist field of the form whose album artists and explicit
/// tracks' artists are the JSON arrays `album_artists` and `track_artists`,
/// with how it stands to the library; `None` when the album has none.
pub(super) fn album_artist_field_on(
    sql: &impl coven::SqlRead,
    album_artists: &str,
    track_artists: &str,
) -> Result<Option<crate::ArtistField>, DbError> {
    let request = FormAlbumArtistFieldParams {
        album_artists: album_artists.to_string(),
        track_artists: track_artists.to_string(),
    };
    // The album has at most one artist field.
    statement_optional(sql, &request)?.map(|row| artist_field_of_json(&row.album.field)).transpose()
}
