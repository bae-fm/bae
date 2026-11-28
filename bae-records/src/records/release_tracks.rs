//! A release's tracks as its detail shows them: in groups, one for each side
//! or disc, with each track's credits, works and stars inside its group and
//! where each sits on the release's mediums.
//!
//! A track's place is its medium, side and number. A track on a medium
//! played side by side (a record, a cassette) is on a lettered side, the
//! letters counting the sides of the release's sided mediums in order; a
//! track on another medium is on a numbered disc when the release holds more
//! than one medium, and only a number otherwise.
//! [`TrackPlaces`](super::place_fragments::TrackPlaces) states that rule, over
//! the mediums and places each read gives it.

use super::read::*;
use super::*;
use bae_store::statement_rows;
use bae_types::mediums::{side_letters, TrackPosition, TrackSide};

impl DbSheetSideChoice {
    /// The choice a row of
    /// [`SheetSideChoices`](super::place_fragments::SheetSideChoices) names.
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
    writer NamedTrackPlaces(tracks: String),
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
/// Where the track `place` names sits, as a track place.
pub fn track_place_of(place: super::place_fragments::TrackPlaces) -> Result<DbTrackPlace, DbError> {
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
pub(crate) fn track_places_on(
    sql: &coven::SqlReadContext<'_>,
    track_ids: &[String],
) -> Result<Vec<DbTrackPlace>, DbError> {
    sql.reads(&track_places_read(track_ids))?.into_iter().map(track_place_of).collect()
}

/// The read of where each of `track_ids` sits, in their order, each track
/// the library holds.
pub fn track_places_read(
    track_ids: &[String],
) -> coven::ReadAs<super::place_fragments::TrackPlaces> {
    let statement = NamedTrackPlacesParams { tracks: serde_json::json!(track_ids).to_string() };
    coven::ReadAs::new(statement, |row| row.place)
}

/// The side a placed track's `side_kind` and `side_ordinal` name.
fn side_of_columns(kind: &str, ordinal: i64) -> Result<TrackSide, String> {
    let ordinal = u32::try_from(ordinal).map_err(|_error| format!("side ordinal {ordinal}"))?;
    track_side_of(kind, ordinal)
}

// Where each place the JSON array `places` gives sits, in their order, on the
// mediums the JSON array `mediums` gives, which no table holds.
sqlive_compose::read!(
    writer GivenSidePlaces(mediums: String, places: String) { side_kind: String, side_ordinal: i64 },
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
    sql: &impl bae_store::WriterRead,
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
