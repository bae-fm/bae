//! A release's tracks as its detail shows them: in groups, one for each side
//! or disc, with each track's credits, works and stars inside its group and
//! where each sits on the release's mediums.
//!
//! A track's place is its medium, side and number. A track on a medium
//! played side by side (a record, a cassette) is on a lettered side, the
//! letters counting the sides of the release's sided mediums in order; a
//! track on another medium is on a numbered disc when the release holds more
//! than one medium, and only a number otherwise. [`track_places_ctes`] is the
//! one statement of that rule: every statement that names a track's side or
//! disc, the import pane's among them, places its tracks through it, on the
//! mediums its [`MediumsSource`] names.

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

/// Where a statement over a release's tracks finds the release's mediums and
/// the place of each track on them: the library's, those of the catalog
/// release the person applied in a release editor's session, or those of an
/// import's draft.
#[derive(Debug, Clone, Copy)]
pub enum MediumsSource<'a> {
    /// The release's mediums and its tracks' places as the library holds
    /// them.
    Release,
    /// Mediums and places other than the library's, as two SQL queries in
    /// parentheses: `mediums` returns the columns `id`, `release_id`,
    /// `position`, `medium` and `side_count` of `release_mediums`, one row
    /// per medium, and `places` returns the columns `id` (the track's),
    /// `release_id`, `medium_id` (a medium's `id`), `side`, `track_number`,
    /// `stated_length_ms` and `position` of `tracks`, one row per track.
    Draft { mediums: &'a str, places: &'a str },
}

impl MediumsSource<'_> {
    fn mediums(&self) -> &str {
        match self {
            Self::Release => "release_mediums",
            Self::Draft { mediums, .. } => mediums,
        }
    }

    fn places(&self) -> &str {
        match self {
            Self::Release => "tracks",
            Self::Draft { places, .. } => places,
        }
    }
}

/// The common table expression `medium`: the rows of `mediums` whose
/// release `release_filter` keeps, each with whether it is played side by
/// side, how many sided sides come before it on its release, and how many
/// mediums its release holds.
/// `mediums` is a table, or an SQL query in parentheses, with the columns of
/// `release_mediums`. `release_filter` is a condition over `m`, one row of
/// `mediums`.
fn medium_cte(mediums: &str, release_filter: &str) -> String {
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
            FROM {mediums} m WHERE {release_filter})"
    )
}

/// The common table expressions `medium` and `place`, over the mediums and
/// places `source` names, for the releases `release_filter` keeps (a
/// condition over `m`, one row of the mediums). `medium` is each of those
/// mediums with whether it is played side by side, how many sided sides come
/// before it on its release, and how many mediums its release holds. `place`
/// is each track on them, with the columns `id`, `release_id`, `position`
/// (its playing order), `number` (its number on its side),
/// `stated_length_ms`, `medium_position`, `carrier` (its medium's carrier
/// key, where the source states one), `side_kind` (`sided`, `disc` or
/// `flat`) and `side_ordinal` (the side's number among the sided sides, the
/// disc's number, or zero). [`track_position_of`] reads a `place` row's
/// kind, ordinal and number.
pub fn track_places_ctes(source: MediumsSource<'_>, release_filter: &str) -> String {
    format!(
        "{medium}, \
         place AS ( \
            SELECT p.id, p.release_id, p.position, p.track_number AS number, \
                   p.stated_length_ms, m.position AS medium_position, m.medium AS carrier, \
                   CASE WHEN m.sided THEN 'sided' \
                        WHEN m.medium_count > 1 THEN 'disc' \
                        ELSE 'flat' END AS side_kind, \
                   CASE WHEN m.sided THEN m.sides_before + p.side \
                        WHEN m.medium_count > 1 THEN m.position \
                        ELSE 0 END AS side_ordinal \
            FROM {places} p JOIN medium m ON m.id = p.medium_id AND m.release_id = p.release_id)",
        medium = medium_cte(source.mediums(), release_filter),
        places = source.places(),
    )
}

/// The common table expressions the statements over a release's tracks
/// start from, for the release `release` (an SQL expression naming its id),
/// on the mediums `source` names: those of [`track_places_ctes`], and
/// `placed`, each of the release's tracks with its side's kind and ordinal,
/// its number on the side, and `extra_columns`, further columns of the track
/// `track` and its place `place` of the release `r` of the album `a`, each
/// starting with a comma.
pub(super) fn placed_tracks_ctes(
    release: &str,
    source: MediumsSource<'_>,
    extra_columns: &str,
    extra_joins: &str,
) -> String {
    format!(
        "{places}, \
         placed AS ( \
            SELECT track.id AS id, track.title AS title, track.duration_ms AS duration_ms, \
                   place.position AS position, place.number AS number, \
                   place.side_kind AS side_kind, place.side_ordinal AS side_ordinal \
                   {extra_columns} \
            FROM tracks track \
            JOIN place ON place.id = track.id \
            JOIN releases r ON r.id = track.release_id \
            JOIN albums a ON a.id = r.album_id \
            {extra_joins} \
            WHERE track.release_id = {release})",
        places = track_places_ctes(source, &format!("m.release_id = {release}")),
    )
}

/// The place a `place` row's columns name: the track's side or disc and its
/// number there.
pub fn track_position_of(kind: &str, ordinal: u32, number: u32) -> Result<TrackPosition, String> {
    match kind {
        "sided" => Ok(TrackPosition::Sided { side_letter: side_letters(ordinal), number }),
        "disc" => Ok(TrackPosition::Disc { disc: ordinal, number }),
        "flat" => Ok(TrackPosition::Flat { number }),
        other => Err(format!("unknown track side kind {other:?}")),
    }
}

/// The side a `place` row's kind and ordinal name.
pub fn track_side_of(kind: &str, ordinal: u32) -> Result<TrackSide, String> {
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
    second_column_start: Option<u32>,
    tracks: Vec<TrackJson>,
}

/// The tracks of the release `release` (an SQL expression naming its id) in
/// playing order, grouped by side or disc (a run of tracks on one side is one
/// group), as a JSON array of groups for [`track_groups_column`]: each group
/// with its tracks' lengths added up, whether the release has more than one
/// group, where its second column starts, and its tracks.
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
                   CASE WHEN COUNT(*) > 8 THEN (COUNT(*) + 1) / 2 END AS second_column_start, \
                   json_group_array(json_object( \
                       'id', id, 'title', title, 'duration_ms', duration_ms, 'number', number, \
                       'own_credits', json(own_credits), 'shown_credits', json(shown_credits), \
                       'works', json(works), 'my_stars', my_stars) ORDER BY position) AS tracks \
            FROM island \
            GROUP BY side_kind, side_ordinal, island) \
         SELECT json_group_array(json_object( \
                    'side_kind', side_kind, 'side_ordinal', side_ordinal, \
                    'total_duration_ms', total_duration_ms, 'shows_time', shows_time, \
                    'second_column_start', second_column_start, \
                    'tracks', json(tracks)) ORDER BY first_position) \
         FROM grouped)",
        placed = placed_tracks_ctes(
            release,
            MediumsSource::Release,
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
            ),
            ""
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
                second_column_start: group.second_column_start,
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
         {places} \
         SELECT place.id AS track_id, place.release_id AS release_id, \
                place.carrier AS carrier, place.side_kind AS side_kind, \
                place.side_ordinal AS side_ordinal \
         FROM wanted w \
         JOIN place ON place.id = w.track_id \
         ORDER BY w.ord",
        places = track_places_ctes(
            MediumsSource::Release,
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
