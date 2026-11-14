//! The credits a release's detail lists: the release's own, each track's own,
//! and the credits of the works each track performs in a role other than a
//! writing one (a work's writing credits are its composers, named with the
//! work), by role: the artists who hold a role over the same tracks are one
//! row.

use super::query::shown_artist_id;
use super::read::credit_name_join;
use super::*;

/// One part of the read: the credit rows of `table` as the columns
/// [`release_credits_on`] orders and reads.
struct CreditPart<'a> {
    table: &'a str,
    /// 0 for the release's credits, 1 for a track's.
    part: u8,
    /// The track the credit sits on, an SQL expression.
    track_id: &'a str,
    /// Its place in playing order, an SQL expression.
    track_position: &'a str,
    /// 0 for a track's own credits, 1 for those of the works it performs.
    kind: u8,
    first_position: &'a str,
    second_position: &'a str,
    joins: &'a str,
    filter: String,
}

impl CreditPart<'_> {
    fn select(&self) -> String {
        format!(
            "SELECT {part} AS part, {track_id} AS track_id, {track_position} AS track_position, \
                    {kind} AS kind, {first} AS first_position, {second} AS second_position, \
                    credit.id AS credit_id, \
                    shown.id AS artist_id, COALESCE(printed.name, shown.name) AS printed, \
                    credit.role AS role, credit.role_words AS role_words, \
                    credit.qualifiers AS qualifiers \
             FROM {table} credit {joins} \
             JOIN artists shown ON shown.id = {shown} \
             {printed_name} \
             WHERE {filter}",
            part = self.part,
            track_id = self.track_id,
            track_position = self.track_position,
            kind = self.kind,
            first = self.first_position,
            second = self.second_position,
            table = self.table,
            joins = self.joins,
            shown = shown_artist_id("credit.artist_id"),
            printed_name = credit_name_join("credit", "printed"),
            filter = self.filter,
        )
    }
}

/// The credits the detail of the release `release_expr` (an SQL expression naming
/// its id) lists, as a JSON array of rows for [`credit_rows_column`]. The credits
/// are ordered as the release lists them: the release's, in the order the
/// catalog stated them; then for each track in playing order, its own
/// credits, then the credits in a role other than a writing one of the works
/// it performs, each work in the order the track performs it. An artist
/// holds a role (with its qualifiers) over the tracks its credits sit on, all
/// of them for a credit of the release; artists holding a role over the same
/// tracks are one row, in the order of their first credit, and rows go in the
/// order of their first credit. A row covering every track has no tracks.
pub(super) fn release_credits_json_sql(release_expr: &str) -> String {
    let release = CreditPart {
        table: "release_artist_roles",
        part: 0,
        track_id: "NULL",
        track_position: "0",
        kind: 0,
        first_position: "credit.position",
        second_position: "0",
        joins: "",
        filter: format!("credit.release_id = {release_expr}"),
    };
    let track = CreditPart {
        table: "track_artist_roles",
        part: 1,
        track_id: "credit.track_id",
        track_position: "track.position",
        kind: 0,
        first_position: "credit.position",
        second_position: "0",
        joins: "JOIN tracks track ON track.id = credit.track_id",
        filter: format!("track.release_id = {release_expr}"),
    };
    let work = CreditPart {
        table: "work_artists",
        part: 1,
        track_id: "performed.track_id",
        track_position: "track.position",
        kind: 1,
        first_position: "performed.position",
        second_position: "credit.position",
        joins: "JOIN track_works performed ON performed.work_id = credit.work_id \
                JOIN tracks track ON track.id = performed.track_id",
        filter: format!(
            "track.release_id = {release_expr} \
             AND credit.role NOT IN (SELECT role FROM writing_credit_roles)"
        ),
    };
    format!(
        "(WITH {placed}, \
         listed AS ({release} UNION ALL {track} UNION ALL {work}), \
         ordered AS ( \
            SELECT listed.*, \
                   ROW_NUMBER() OVER (ORDER BY part, track_position, kind, first_position, \
                                               second_position, credit_id) AS ord \
            FROM listed), \
         named AS ( \
            SELECT ordered.*, \
                   FIRST_VALUE(printed) OVER (PARTITION BY role, role_words, qualifiers, \
                                                           artist_id ORDER BY ord) AS held_printed \
            FROM ordered), \
         held AS ( \
            SELECT n.role, n.role_words, n.qualifiers, n.artist_id, MIN(n.ord) AS ord, \
                   MIN(n.held_printed) AS printed, \
                   CASE WHEN MAX(n.track_id IS NULL) = 1 \
                          OR COUNT(DISTINCT n.track_id) = (SELECT COUNT(*) FROM placed) \
                        THEN NULL \
                        ELSE (SELECT json_group_array(json_object( \
                                  'kind', p.side_kind, 'ordinal', p.side_ordinal, \
                                  'number', p.number) ORDER BY p.position) \
                              FROM placed p \
                              WHERE p.id IN (SELECT same.track_id FROM named same \
                                             WHERE same.role = n.role \
                                               AND same.role_words IS n.role_words \
                                               AND same.qualifiers = n.qualifiers \
                                               AND same.artist_id = n.artist_id)) \
                   END AS tracks \
            FROM named n \
            GROUP BY n.role, n.role_words, n.qualifiers, n.artist_id), \
         rows AS ( \
            SELECT role, role_words, qualifiers, MIN(ord) AS first_ord, \
                   json_group_array(printed ORDER BY ord) AS artists, tracks \
            FROM held \
            GROUP BY role, role_words, qualifiers, tracks) \
         SELECT json_group_array(json_object( \
                    'role', role, 'role_words', role_words, 'qualifiers', qualifiers, \
                    'artists', json(artists), 'tracks', json(tracks)) ORDER BY first_ord) \
         FROM rows)",
        placed = super::release_tracks::placed_tracks_ctes(release_expr, ""),
        release = release.select(),
        track = track.select(),
        work = work.select(),
    )
}

/// One place of a credit row's tracks, as the statement spells it in JSON.
#[derive(serde::Deserialize)]
struct PlaceJson {
    kind: String,
    ordinal: u32,
    number: u32,
}

/// One credit row, as the statement spells it in JSON.
#[derive(serde::Deserialize)]
struct RowJson {
    role: String,
    role_words: Option<String>,
    qualifiers: String,
    artists: Vec<String>,
    tracks: Option<Vec<PlaceJson>>,
}

/// The release's credit rows, in the order it lists them, as
/// [`release_credits_json_sql`] read them into `column`.
pub(super) fn credit_rows_column(
    row: &Row<'_>,
    column: &str,
) -> rusqlite::Result<Vec<DbCreditRow>> {
    let rows: Vec<RowJson> = bae_store::read::json_column(row, column)?;
    rows.into_iter()
        .map(|credit| {
            Ok(DbCreditRow {
                role: bae_types::credit_role::CreditedAs::from_columns(
                    &credit.role,
                    credit.role_words,
                    &credit.qualifiers,
                )
                .map_err(|error| column_conversion_error(row, column, error.to_string()))?,
                artists: credit.artists,
                tracks: credit
                    .tracks
                    .unwrap_or_default()
                    .into_iter()
                    .map(|place| {
                        super::release_tracks::track_position_of(
                            &place.kind,
                            place.ordinal,
                            place.number,
                        )
                        .map_err(|reason| column_conversion_error(row, column, reason))
                    })
                    .collect::<rusqlite::Result<_>>()?,
            })
        })
        .collect()
}
