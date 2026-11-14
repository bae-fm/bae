//! A release's tracks as its detail shows them: in groups, one for each side
//! or disc, with each track's credits, works and stars inside its group and
//! where each sits on the release's mediums.
//!
//! A track's place is its medium, side and number. A track on a medium
//! played side by side (a record, a cassette) is on a lettered side, the
//! letters counting the sides of the release's sided mediums in order; a
//! track on another medium is on a numbered disc when the release holds more
//! than one medium, and plain otherwise. [`bae_types::mediums::track_position`]
//! states the same rule over decoded rows, which the release editor reads
//! by.

use super::query::*;
use super::read::*;
use super::*;
use bae_types::mediums::{side_letters, TrackPosition, TrackSide};

/// The keys of the carriers played side by side, as an SQL list.
fn sided_media_sql() -> String {
    let keys: Vec<String> = bae_types::pressing::Medium::ALL
        .iter()
        .filter(|medium| medium.is_sided())
        .map(|medium| format!("'{}'", medium.key()))
        .collect();
    format!("({})", keys.join(", "))
}

/// The two common table expressions the statements over a release's tracks
/// start from, binding the release as `?1`: `medium`, each of its mediums
/// with how many sided sides come before it, and `placed`, each of its
/// tracks with its side's kind (`sided`, `disc` or `flat`) and ordinal (the
/// side's number among the sided sides, the disc's number, or zero), its
/// number on the side, and `extra_columns`, further columns of the track
/// `track` of the release `r` of the album `a`, each starting with a comma.
pub(super) fn placed_tracks_ctes(extra_columns: &str) -> String {
    let sided = sided_media_sql();
    format!(
        "medium AS ( \
            SELECT m.id, m.position, \
                   COALESCE(m.medium IN {sided}, 0) AS sided, \
                   COALESCE(SUM(CASE WHEN COALESCE(m.medium IN {sided}, 0) THEN m.side_count \
                                     ELSE 0 END) \
                            OVER (ORDER BY m.position \
                                  ROWS BETWEEN UNBOUNDED PRECEDING AND 1 PRECEDING), 0) \
                       AS sides_before, \
                   COUNT(*) OVER () AS medium_count \
            FROM release_mediums m WHERE m.release_id = ?1), \
         placed AS ( \
            SELECT track.id AS id, track.title AS title, track.duration_ms AS duration_ms, \
                   track.position AS position, track.track_number AS number, \
                   CASE WHEN m.sided THEN 'sided' \
                        WHEN m.medium_count > 1 THEN 'disc' \
                        ELSE 'flat' END AS side_kind, \
                   CASE WHEN m.sided THEN m.sides_before + track.side \
                        WHEN m.medium_count > 1 THEN m.position \
                        ELSE 0 END AS side_ordinal \
                   {extra_columns} \
            FROM tracks track \
            JOIN releases r ON r.id = track.release_id \
            JOIN albums a ON a.id = r.album_id \
            JOIN medium m ON m.id = track.medium_id \
            WHERE track.release_id = ?1)"
    )
}

/// The place a `placed` row's columns name: how a track reads on its
/// release.
pub(super) fn track_position_of(
    kind: &str,
    ordinal: u32,
    number: u32,
) -> Result<TrackPosition, String> {
    match kind {
        "sided" => Ok(TrackPosition::Sided { side_letter: side_letters(ordinal), number }),
        "disc" => Ok(TrackPosition::Disc { disc: ordinal, number }),
        "flat" => Ok(TrackPosition::Flat { number }),
        other => Err(format!("unknown track side kind {other:?}")),
    }
}

/// The side a `placed` row's kind and ordinal name.
fn track_side_of(kind: &str, ordinal: u32) -> Result<TrackSide, String> {
    Ok(track_position_of(kind, ordinal, 1)?.side())
}

/// One track of a group, as the groups' statement spells it in JSON.
#[derive(serde::Deserialize)]
struct TrackJson {
    id: String,
    title: String,
    duration_ms: u64,
    number: u32,
    own_credits: Vec<CreditJson>,
    shown_credits: Vec<CreditJson>,
    works: Vec<WorkJson>,
    my_stars: Option<i64>,
}

#[derive(serde::Deserialize)]
struct WorkJson {
    work_id: String,
    title: String,
    composer_names: Option<String>,
}

/// The tracks of the release `?1` in playing order, grouped by side or disc
/// (a run of tracks on one side is one group), each group with its tracks'
/// lengths added up, whether the release has more than one group, and its
/// tracks as one JSON column.
fn track_groups_sql(my_stars: &str) -> String {
    format!(
        "WITH {placed}, \
         island AS ( \
            SELECT placed.*, \
                   ROW_NUMBER() OVER (ORDER BY position) \
                   - ROW_NUMBER() OVER (PARTITION BY side_kind, side_ordinal ORDER BY position) \
                       AS island \
            FROM placed) \
         SELECT side_kind, side_ordinal, SUM(duration_ms) AS total_duration_ms, \
                COUNT(*) OVER () > 1 AS shows_time, \
                json_group_array(json_object( \
                    'id', id, 'title', title, 'duration_ms', duration_ms, 'number', number, \
                    'own_credits', json(own_credits), 'shown_credits', json(shown_credits), \
                    'works', json(works), 'my_stars', my_stars) ORDER BY position) AS tracks \
         FROM island \
         GROUP BY side_kind, side_ordinal, island \
         ORDER BY MIN(position)",
        placed = placed_tracks_ctes(&format!(
            ", {own} AS own_credits, {shown} AS shown_credits, \
               (SELECT json_group_array(json_object( \
                    'work_id', w.id, 'title', w.title, 'composer_names', {composer_names}) \
                    ORDER BY tw.position, tw.id) \
                FROM track_works tw JOIN works w ON w.id = tw.work_id \
                WHERE tw.track_id = track.id) AS works, \
               {my_stars} AS my_stars",
            own = track_own_credits_json_sql("track"),
            shown = track_credits_json_sql("track"),
            composer_names = work_composer_names_sql("w.id"),
        )),
    )
}

/// The release's tracks in groups, each track with the stars `my_stars`, an
/// SQL expression over the track `track`, gives it.
pub(super) fn release_track_groups_on(
    sql: &impl coven::SqlRead,
    release_id: &str,
    my_stars: &str,
) -> Result<Vec<DbTrackGroup>, DbError> {
    Ok(sql.query(&track_groups_sql(my_stars), params![release_id], |row| {
        let kind: String = row.get("side_kind")?;
        let ordinal: u32 = row.get("side_ordinal")?;
        let unreadable =
            |column: &str, reason: String| column_conversion_error(row, column, reason);
        let side =
            track_side_of(&kind, ordinal).map_err(|reason| unreadable("side_kind", reason))?;
        let json: String = row.get("tracks")?;
        let tracks: Vec<TrackJson> = serde_json::from_str(&json)
            .map_err(|error| column_conversion_error(row, "tracks", error))?;
        let tracks = tracks
            .into_iter()
            .map(|track| {
                let credits = |credits| {
                    credits_of(credits).map_err(|error| unreadable("tracks", error.to_string()))
                };
                Ok(DbTrackWithArtists {
                    position: track_position_of(&kind, ordinal, track.number)
                        .map_err(|reason| unreadable("tracks", reason))?,
                    artists: credits(track.own_credits)?,
                    shown_artists: credits(track.shown_credits)?,
                    works: track
                        .works
                        .into_iter()
                        .map(|work| DbPerformedWork {
                            work_id: work.work_id,
                            title: work.title,
                            composer_names: work.composer_names,
                        })
                        .collect(),
                    my_stars: track
                        .my_stars
                        .map(|stars| {
                            bae_types::stars::Stars::from_stored(stars).ok_or_else(|| {
                                unreadable("tracks", format!("{stars} is not a number of stars"))
                            })
                        })
                        .transpose()?,
                    id: track.id,
                    title: track.title,
                    duration_ms: track.duration_ms,
                })
            })
            .collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(DbTrackGroup {
            side,
            tracks,
            total_duration_ms: row.get("total_duration_ms")?,
            shows_time: row.get("shows_time")?,
        })
    })?)
}

/// How long the release's tracks play.
pub(super) fn release_total_duration_on(
    sql: &impl coven::SqlRead,
    release_id: &str,
) -> Result<u64, DbError> {
    Ok(sql.query_row(
        "SELECT COALESCE(SUM(duration_ms), 0) FROM tracks WHERE release_id = ?1",
        params![release_id],
        |row| row.get(0),
    )?)
}
