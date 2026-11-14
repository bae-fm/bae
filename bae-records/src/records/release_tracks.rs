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

/// The common table expression `medium`: the mediums of the releases
/// `release_filter` (a condition over `m`, a row of `release_mediums`) keeps,
/// each with whether it is played side by side, how many sided sides come
/// before it on its release, and how many mediums its release holds.
fn medium_cte(release_filter: &str) -> String {
    let sided = sided_media_sql();
    format!(
        "medium AS ( \
            SELECT m.id, m.release_id, m.position, m.medium, \
                   COALESCE(m.medium IN {sided}, 0) AS sided, \
                   COALESCE(SUM(CASE WHEN COALESCE(m.medium IN {sided}, 0) THEN m.side_count \
                                     ELSE 0 END) \
                            OVER (PARTITION BY m.release_id ORDER BY m.position \
                                  ROWS BETWEEN UNBOUNDED PRECEDING AND 1 PRECEDING), 0) \
                       AS sides_before, \
                   COUNT(*) OVER (PARTITION BY m.release_id) AS medium_count \
            FROM release_mediums m WHERE {release_filter})"
    )
}

/// What kind of side the track `track` sits on, over a join of `medium` as
/// `m`: `sided`, `disc` or `flat`.
const SIDE_KIND_SQL: &str = "CASE WHEN m.sided THEN 'sided' \
                                  WHEN m.medium_count > 1 THEN 'disc' \
                                  ELSE 'flat' END";

/// The ordinal of the side the track `track` sits on, over a join of
/// `medium` as `m`: its number among the sided sides, the disc's number, or
/// zero.
const SIDE_ORDINAL_SQL: &str = "CASE WHEN m.sided THEN m.sides_before + track.side \
                                     WHEN m.medium_count > 1 THEN m.position \
                                     ELSE 0 END";

/// The two common table expressions the statements over a release's tracks
/// start from, for the release `release` (an SQL expression naming its id):
/// `medium`, each of its mediums
/// with how many sided sides come before it, and `placed`, each of its
/// tracks with its side's kind (`sided`, `disc` or `flat`) and ordinal (the
/// side's number among the sided sides, the disc's number, or zero), its
/// number on the side, and `extra_columns`, further columns of the track
/// `track` of the release `r` of the album `a`, each starting with a comma.
pub(super) fn placed_tracks_ctes(release: &str, extra_columns: &str) -> String {
    format!(
        "{medium}, \
         placed AS ( \
            SELECT track.id AS id, track.title AS title, track.duration_ms AS duration_ms, \
                   track.position AS position, track.track_number AS number, \
                   {SIDE_KIND_SQL} AS side_kind, {SIDE_ORDINAL_SQL} AS side_ordinal \
                   {extra_columns} \
            FROM tracks track \
            JOIN releases r ON r.id = track.release_id \
            JOIN albums a ON a.id = r.album_id \
            JOIN medium m ON m.id = track.medium_id \
            WHERE track.release_id = {release})",
        medium = medium_cte(&format!("m.release_id = {release}")),
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

/// One group of tracks, as the groups' statement spells it in JSON.
#[derive(serde::Deserialize)]
struct GroupJson {
    side_kind: String,
    side_ordinal: u32,
    total_duration_ms: u64,
    shows_time: i64,
    tracks: Vec<TrackJson>,
}

/// The tracks of the release `release` (an SQL expression naming its id) in
/// playing order, grouped by side or disc (a run of tracks on one side is one
/// group), as a JSON array of groups for [`track_groups_column`]: each group
/// with its tracks' lengths added up, whether the release has more than one
/// group, and its tracks.
pub(super) fn track_groups_json_sql(release: &str, my_stars: &str) -> String {
    format!(
        "(WITH {placed}, \
         island AS ( \
            SELECT placed.*, \
                   ROW_NUMBER() OVER (ORDER BY position) \
                   - ROW_NUMBER() OVER (PARTITION BY side_kind, side_ordinal ORDER BY position) \
                       AS island \
            FROM placed), \
         grouped AS ( \
            SELECT side_kind, side_ordinal, MIN(position) AS first_position, \
                   SUM(duration_ms) AS total_duration_ms, \
                   COUNT(*) OVER () > 1 AS shows_time, \
                   json_group_array(json_object( \
                       'id', id, 'title', title, 'duration_ms', duration_ms, 'number', number, \
                       'own_credits', json(own_credits), 'shown_credits', json(shown_credits), \
                       'works', json(works), 'my_stars', my_stars) ORDER BY position) AS tracks \
            FROM island \
            GROUP BY side_kind, side_ordinal, island) \
         SELECT json_group_array(json_object( \
                    'side_kind', side_kind, 'side_ordinal', side_ordinal, \
                    'total_duration_ms', total_duration_ms, 'shows_time', shows_time, \
                    'tracks', json(tracks)) ORDER BY first_position) \
         FROM grouped)",
        placed = placed_tracks_ctes(
            release,
            &format!(
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
            )
        ),
    )
}

/// The release's track groups as [`track_groups_json_sql`] read them into
/// `column`.
pub(super) fn track_groups_column(
    row: &Row<'_>,
    column: &str,
) -> rusqlite::Result<Vec<DbTrackGroup>> {
    let groups: Vec<GroupJson> = bae_store::read::json_column(row, column)?;
    groups
        .into_iter()
        .map(|group| {
            let unreadable = |reason: String| column_conversion_error(row, column, reason);
            let side = track_side_of(&group.side_kind, group.side_ordinal).map_err(unreadable)?;
            let tracks = group
                .tracks
                .into_iter()
                .map(|track| {
                    let credits = |credits| {
                        credits_of(credits).map_err(|error| unreadable(error.to_string()))
                    };
                    Ok(DbTrackWithArtists {
                        position: track_position_of(
                            &group.side_kind,
                            group.side_ordinal,
                            track.number,
                        )
                        .map_err(unreadable)?,
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
                                    unreadable(format!("{stars} is not a number of stars"))
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
                total_duration_ms: group.total_duration_ms,
                shows_time: group.shows_time != 0,
            })
        })
        .collect()
}

/// How long the tracks of the release `release` (an SQL expression naming
/// its id) play.
pub(super) fn total_duration_sql(release: &str) -> String {
    format!("(SELECT COALESCE(SUM(duration_ms), 0) FROM tracks WHERE release_id = {release})")
}

/// The places of the tracks the JSON array `?1` names, in the array's order,
/// leaving out a track the library does not hold.
fn track_places_sql() -> String {
    format!(
        "WITH wanted AS (SELECT j.key AS ord, j.value AS track_id FROM json_each(?1) j), \
         {medium} \
         SELECT track.id AS track_id, track.release_id AS release_id, \
                m.position AS medium_position, track.side AS side, m.medium AS carrier, \
                {SIDE_KIND_SQL} AS side_kind, {SIDE_ORDINAL_SQL} AS side_ordinal \
         FROM wanted w \
         JOIN tracks track ON track.id = w.track_id \
         JOIN medium m ON m.id = track.medium_id \
         ORDER BY w.ord",
        medium = medium_cte(
            "m.release_id IN (SELECT t.release_id FROM tracks t \
                              JOIN wanted w ON w.track_id = t.id)"
        ),
    )
}

/// Where each of `track_ids` sits, in their order, each track the library
/// holds.
pub fn track_places_on(
    sql: &impl coven::SqlRead,
    track_ids: &[String],
) -> Result<Vec<DbTrackPlace>, DbError> {
    Ok(sql.query(
        &track_places_sql(),
        params![serde_json::json!(track_ids).to_string()],
        |row| {
            let unreadable =
                |column: &str, reason: String| column_conversion_error(row, column, reason);
            let kind: String = row.get("side_kind")?;
            let ordinal: u32 = row.get("side_ordinal")?;
            let carrier: Option<String> = row.get("carrier")?;
            Ok(DbTrackPlace {
                track_id: row.get("track_id")?,
                release_id: row.get("release_id")?,
                medium: row.get("medium_position")?,
                side: row.get("side")?,
                carrier: carrier
                    .map(|key| {
                        bae_types::pressing::Medium::from_key(&key)
                            .ok_or_else(|| unreadable("carrier", format!("unknown medium {key:?}")))
                    })
                    .transpose()?,
                on: track_side_of(&kind, ordinal)
                    .map_err(|reason| unreadable("side_kind", reason))?,
            })
        },
    )?)
}
