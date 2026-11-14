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
    filter: &'a str,
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

/// The rows of the credits the detail of the release `?1` lists. The credits
/// are ordered as the release lists them: the release's, in the order the
/// catalog stated them; then for each track in playing order, its own
/// credits, then the credits in a role other than a writing one of the works
/// it performs, each work in the order the track performs it. An artist
/// holds a role (with its qualifiers) over the tracks its credits sit on, all
/// of them for a credit of the release; artists holding a role over the same
/// tracks are one row, in the order of their first credit, and rows go in the
/// order of their first credit. A row covering every track has no tracks.
fn release_credits_sql() -> String {
    let release = CreditPart {
        table: "release_artist_roles",
        part: 0,
        track_id: "NULL",
        track_position: "0",
        kind: 0,
        first_position: "credit.position",
        second_position: "0",
        joins: "",
        filter: "credit.release_id = ?1",
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
        filter: "track.release_id = ?1",
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
        filter: "track.release_id = ?1 \
                 AND credit.role NOT IN (SELECT role FROM writing_credit_roles)",
    };
    format!(
        "WITH {placed}, \
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
            GROUP BY n.role, n.role_words, n.qualifiers, n.artist_id) \
         SELECT role, role_words, qualifiers, \
                json_group_array(printed ORDER BY ord) AS artists, tracks \
         FROM held \
         GROUP BY role, role_words, qualifiers, tracks \
         ORDER BY MIN(ord)",
        placed = super::release_tracks::placed_tracks_ctes(""),
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

/// The credit rows of the release `release_id`'s detail, in the order it
/// lists them.
pub(super) fn release_credits_on(
    sql: &impl coven::SqlRead,
    release_id: &str,
) -> Result<Vec<DbCreditRow>, DbError> {
    Ok(sql.query(&release_credits_sql(), params![release_id], |row| {
        let role: String = row.get("role")?;
        let words: Option<String> = row.get("role_words")?;
        let qualifiers: String = row.get("qualifiers")?;
        let artists: String = row.get("artists")?;
        let tracks: Option<String> = row.get("tracks")?;
        let places: Vec<PlaceJson> = tracks
            .map(|tracks| serde_json::from_str(&tracks))
            .transpose()
            .map_err(|error| column_conversion_error(row, "tracks", error))?
            .unwrap_or_default();
        Ok(DbCreditRow {
            role: bae_types::credit_role::CreditedAs::from_columns(&role, words, &qualifiers)
                .map_err(|error| column_conversion_error(row, "role", error.to_string()))?,
            artists: serde_json::from_str(&artists)
                .map_err(|error| column_conversion_error(row, "artists", error))?,
            tracks: places
                .into_iter()
                .map(|place| {
                    super::release_tracks::track_position_of(
                        &place.kind,
                        place.ordinal,
                        place.number,
                    )
                    .map_err(|reason| column_conversion_error(row, "tracks", reason))
                })
                .collect::<rusqlite::Result<_>>()?,
        })
    })?)
}
