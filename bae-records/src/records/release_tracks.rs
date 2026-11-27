//! A release's tracks as its detail shows them: in groups, one for each side
//! or disc, with each track's credits, works and stars inside its group and
//! where each sits on the release's mediums.
//!
//! A track's place is its medium, side and number. A track on a medium
//! played side by side (a record, a cassette) is on a lettered side, the
//! letters counting the sides of the release's sided mediums in order; a
//! track on another medium is on a numbered disc when the release holds more
//! than one medium, and only a number otherwise.
//! [`TrackPlaces`](super::place_fragments::TrackPlaces) states that rule for
//! compiled reads, over the mediums and places each gives it, and
//! [`track_places_ctes`] states it for statements built as text, on the
//! mediums its [`MediumsSource`] names.

use super::read::*;
use super::*;
use bae_store::statement_rows;
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
/// release `release_filter` keeps, each with its count of sides, whether it
/// is played side by side, how many sided sides come before it on its release, and how many
/// mediums its release holds.
/// `mediums` is a table, or an SQL query in parentheses, with the columns of
/// `release_mediums`. `release_filter` is a condition over `m`, one row of
/// `mediums`.
fn medium_cte(mediums: &str, release_filter: &str) -> String {
    let sided = sided_media_sql();
    format!(
        "medium AS ( \
            SELECT m.id, m.release_id, m.position, m.medium, m.side_count, \
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
/// (its playing order), `medium_id`, `side` (its side on its medium, counted from one),
/// `number` (its number on its side),
/// `stated_length_ms`, `medium_position`, `carrier` (its medium's carrier
/// key, where the source states one), `side_kind` (`sided`, `disc` or
/// `flat`) and `side_ordinal` (the side's number among the sided sides, the
/// disc's number, or zero). [`track_position_of`] reads a `place` row's
/// kind, ordinal and number.
pub fn track_places_ctes(source: MediumsSource<'_>, release_filter: &str) -> String {
    format!(
        "{medium}, \
         place AS ( \
            SELECT p.id, p.release_id, p.position, p.medium_id, p.side, p.track_number AS number, \
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

/// The side a CUE sheet whose first track sits on `side` (an SQL
/// expression) is on, where `sided` (an SQL condition) says whether the
/// medium is played side by side: that side of such a medium, else the
/// medium's first side, since every side of another medium shares the
/// disc's heading.
#[cfg(desktop)]
pub(super) fn sheet_side_sql(sided: &str, side: &str) -> String {
    format!("CASE WHEN {sided} THEN {side} ELSE 1 END")
}

/// The kind of heading a CUE sheet's side has, where `sided` (an SQL
/// condition) says whether the medium is played side by side: `sided`, or
/// `disc` for every other medium, a disc of a release of one medium
/// included, since a sheet's place needs a name where a track's side has
/// none.
pub(super) fn sheet_side_kind_sql(sided: &str) -> String {
    format!("CASE WHEN {sided} THEN 'sided' ELSE 'disc' END")
}

/// The ordinal of the heading of a CUE sheet's side, where `sided` (an SQL
/// condition) says whether the medium is played side by side: the side's
/// number among the sided sides (`sided_ordinal`), else the disc's number
/// (`disc`), each an SQL expression.
pub(super) fn sheet_side_ordinal_sql(sided: &str, sided_ordinal: &str, disc: &str) -> String {
    format!("CASE WHEN {sided} THEN {sided_ordinal} ELSE {disc} END")
}

/// Every side or disc of the mediums `mediums` (a table, or an SQL query
/// in parentheses, with the columns of `release_mediums`, the mediums of one
/// release) that a CUE sheet can be put on, as a query in parentheses with
/// the columns `medium`, `side`, `kind` and `ordinal`, in order: each side
/// of a medium played side by side, lettered as the release's tracks are,
/// and the one side of every other medium, numbered as its disc.
/// [`DbSheetSideChoice::of_columns`] reads a row.
pub fn sheet_side_choices_sql(mediums: &str) -> String {
    // Every count of sides a medium can have, which a join reads its sides
    // from: a recursive count would hide from coven which rows the
    // statement reads.
    let counts: Vec<String> = (1..=u8::MAX).map(|side| side.to_string()).collect();
    format!(
        "(WITH {medium} \
          SELECT m.position AS medium, side.value AS side, {kind} AS kind, \
                 {ordinal} AS ordinal \
          FROM medium m JOIN json_each('[{counts}]') AS side \
              ON side.value = 1 OR (m.sided AND side.value <= m.side_count) \
          ORDER BY m.position, side.value)",
        medium = medium_cte(mediums, "1"),
        kind = sheet_side_kind_sql("m.sided"),
        ordinal = sheet_side_ordinal_sql("m.sided", "m.sides_before + side.value", "m.position"),
        counts = counts.join(","),
    )
}

impl DbSheetSideChoice {
    /// The choice a row of [`sheet_side_choices_sql`] names.
    pub fn of_columns(medium: u32, side: u32, kind: &str, ordinal: u32) -> Result<Self, String> {
        Ok(Self {
            side: bae_types::mediums::MediumSide { medium, side },
            heading: track_side_of(kind, ordinal)?,
        })
    }
}

impl crate::DbSheetSideColumns {
    pub fn decoded(self) -> Result<DbSheetSideChoice, String> {
        DbSheetSideChoice::of_columns(self.medium, self.side, &self.kind, self.ordinal)
    }

    pub(crate) fn decoded_list(choices: Vec<Self>) -> Result<Vec<DbSheetSideChoice>, String> {
        choices.into_iter().map(Self::decoded).collect()
    }
}

/// The choices of a CUE caption, as the track and Files queries return them.
#[cfg(desktop)]
pub(super) fn sheet_side_choices_json_sql() -> &'static str {
    "(SELECT json_group_array(json_object( \
         'medium', medium, 'side', side, 'kind', kind, 'ordinal', ordinal) ORDER BY medium, side) \
      FROM choices)"
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

/// The release's track groups, as
/// [`TrackGroups`](super::track_group_fragments::TrackGroups) wrote them into
/// its document `json`.
pub(super) fn track_groups_of_json(json: &str) -> Result<Vec<DbTrackGroup>, DbError> {
    const DOCUMENT: &str = "a release's track groups";
    let groups: Vec<GroupJson> = serde_json::from_str(json)
        .map_err(|error| DbError::unreadable_json(DOCUMENT, json, error))?;
    let unreadable = |reason: String| DbError::unreadable(DOCUMENT, reason);
    groups
        .into_iter()
        .map(|group| {
            let side = track_side_of(&group.side_kind, group.side_ordinal).map_err(unreadable)?;
            let tracks = group
                .tracks
                .into_iter()
                .map(|track| {
                    Ok(DbTrackWithArtists {
                        position: track_position_of(
                            &group.side_kind,
                            group.side_ordinal,
                            track.number,
                        )
                        .map_err(unreadable)?,
                        artists: credits_of(track.own_credits)?,
                        shown_artists: credits_of(track.shown_credits)?,
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
                .collect::<Result<Vec<_>, DbError>>()?;
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

// Where each track that the JSON array `:tracks` names sits, in the array's
// order, leaving out a track the library does not hold. Each side is lettered
// among all of its release's mediums, so the fragment places every track of
// the named tracks' releases.
sqlive_compose::read!(
    NamedTrackPlaces(tracks: String),
    r#"
    SELECT place.*
    FROM json_each(:tracks) AS wanted
    JOIN @crate::records::place_fragments::TrackPlaces(
        mediums: (SELECT medium.id, medium.release_id, medium.position, medium.medium,
                         medium.side_count
                  FROM release_mediums AS medium
                  WHERE medium.release_id IN (
                      SELECT held.release_id
                      FROM json_each(:tracks) AS named
                      JOIN tracks AS held ON held.id = named.value)),
        places: (SELECT held.id, held.release_id, held.medium_id, held.side, held.track_number,
                        held.stated_length_ms, held.position
                 FROM tracks AS held
                 WHERE held.release_id IN (
                     SELECT sibling.release_id
                     FROM json_each(:tracks) AS named
                     JOIN tracks AS sibling ON sibling.id = named.value))) AS place
      ON place.track_id = wanted.value
    ORDER BY wanted.key
"#
);

/// Where `place`'s track sits, as playback reads it.
fn track_place_of(place: super::place_fragments::TrackPlaces) -> Result<DbTrackPlace, DbError> {
    let track = place.track_id;
    let unreadable =
        |reason: String| DbError::unreadable(format!("the place of track {track}"), reason);
    let carrier = place
        .carrier
        .map(|key| {
            bae_types::pressing::Medium::from_key(&key)
                .ok_or_else(|| unreadable(format!("unknown medium {key:?}")))
        })
        .transpose()?;
    let on = side_of_columns(&place.side_kind, place.side_ordinal).map_err(unreadable)?;
    Ok(DbTrackPlace { track_id: track, release_id: place.release_id, carrier, on })
}

/// Where each of `track_ids` sits, in their order, each track the library
/// holds.
pub fn track_places_on(
    sql: &impl coven::SqlRead,
    track_ids: &[String],
) -> Result<Vec<DbTrackPlace>, DbError> {
    let statement = NamedTrackPlacesParams { tracks: serde_json::json!(track_ids).to_string() };
    bae_store::statement_rows(sql, &statement)?
        .into_iter()
        .map(|row| track_place_of(row.place))
        .collect()
}

/// The side a placed track's `side_kind` and `side_ordinal` name.
fn side_of_columns(kind: &str, ordinal: i64) -> Result<TrackSide, String> {
    let ordinal = u32::try_from(ordinal).map_err(|_error| format!("side ordinal {ordinal}"))?;
    track_side_of(kind, ordinal)
}

// Where each place the JSON array `places` gives sits, in their order, on the
// mediums the JSON array `mediums` gives, which no table holds.
sqlive_compose::read!(
    GivenSidePlaces(mediums: String, places: String) { side_kind: String, side_ordinal: i64 },
    r#"
    WITH placed AS (
        SELECT place.*
        FROM @crate::records::place_fragments::TrackPlaces(
            mediums: (SELECT given.key + 1, 'given', given.key + 1,
                             given.value ->> 'carrier', given.value ->> 'sides'
                      FROM json_each(:mediums) AS given),
            places: (SELECT asked.key, 'given', asked.value ->> 'medium',
                            asked.value ->> 'side', 1, NULL, asked.key
                     FROM json_each(:places) AS asked)) AS place)
    SELECT placed.side_kind, placed.side_ordinal
    FROM placed
    ORDER BY placed.position
"#
);

/// The heading of each side of the mediums numbered `numbered` (counted from
/// one, each one of `mediums`) of a release that no table holds, such as one
/// a catalog lists, and that holds `mediums` in order: each sided medium's
/// sides in order, and each other medium's disc. The release has more than
/// one medium or a record or cassette, since a flat side has no heading.
/// [`TrackPlaces`](super::place_fragments::TrackPlaces) places them, over the
/// given mediums and one place on each side of the numbered ones, so a
/// release in a catalog and one in the library name their sides alike.
pub(super) fn side_headings_on(
    sql: &impl coven::SqlRead,
    mediums: &[bae_types::mediums::ReleaseMedium],
    numbered: &[u32],
) -> Result<Vec<bae_types::mediums::SideHeading>, DbError> {
    let given: Vec<serde_json::Value> = mediums
        .iter()
        .map(|medium| {
            serde_json::json!({
                "carrier": medium.carrier.map(bae_types::pressing::Medium::key),
                "sides": medium.sides.get(),
            })
        })
        .collect();
    let places: Vec<serde_json::Value> = numbered
        .iter()
        .flat_map(|&number| {
            let sides = usize::try_from(number)
                .ok()
                .and_then(|number| mediums.get(number.checked_sub(1)?))
                .expect("a numbered medium is one of the release's")
                .sides
                .get();
            (1..=sides).map(move |side| serde_json::json!({ "medium": number, "side": side }))
        })
        .collect();
    let params = GivenSidePlacesParams {
        mediums: serde_json::Value::from(given).to_string(),
        places: serde_json::Value::from(places).to_string(),
    };
    statement_rows(sql, &params)?
        .into_iter()
        .map(|row| {
            let unreadable = |reason: String| DbError::unreadable("a side's heading", reason);
            side_of_columns(&row.side_kind, row.side_ordinal)
                .map_err(unreadable)?
                .heading()
                .ok_or_else(|| unreadable("the only medium of a release has no heading".into()))
        })
        .collect()
}

impl Records {
    /// The heading of each side of the mediums numbered `numbered` of a
    /// release no table holds, which holds `mediums` in order, named as the
    /// library names the sides of its own releases.
    pub async fn side_headings(
        &self,
        mediums: Vec<bae_types::mediums::ReleaseMedium>,
        numbered: Vec<u32>,
    ) -> Result<Vec<bae_types::mediums::SideHeading>, DbError> {
        self.store.read(move |sql| side_headings_on(&sql, &mediums, &numbered)).await
    }
}
