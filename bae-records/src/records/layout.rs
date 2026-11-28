//! Where a release's tracks sit on its mediums once a release editor's form
//! is saved: the mediums in order, and each track in playing order with its
//! place and the length its source lists for it.
//!
//! The form puts the tracks at the places an applied catalog gives them, or
//! the library's own, and moves each CUE sheet the person picked a side or
//! disc for onto that side. One rule lays them out
//! ([`LaidPlaces`](super::layout_fragments::LaidPlaces)), so the
//! editor's draft, which shows the rows laid out, and the save, which writes
//! them, never disagree, and an import draft moves a sheet the same way.

use super::read::rows_of;
use super::*;
use bae_store::statement_row;
#[cfg(desktop)]
use bae_store::WriteStatements as _;
use bae_types::mediums::{ReleaseMedium, TrackPlace};

/// One track of a [`ReleaseLayout`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LaidTrack {
    pub track_id: String,
    pub place: TrackPlace,
    /// The length the source of the release's values lists for the track.
    pub stated_length_ms: Option<u64>,
}

/// A release's mediums, in order from the first, and its tracks in playing
/// order.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReleaseLayout {
    pub mediums: Vec<ReleaseMedium>,
    pub tracks: Vec<LaidTrack>,
}

/// A release editor's layout of its release's tracks, as
/// [`ReleaseLaidPlaces`](super::layout_fragments::ReleaseLaidPlaces) binds
/// it: the release, whether the layout starts from `mediums` and `places`
/// (an applied catalog's) rather than the release's own, the sides picked for
/// CUE sheets (`moves`), and what the form does to the sheets (`edits`, the
/// JSON object of a [`SheetEdits`](super::sheet_files::SheetEdits)), from
/// which the layout works out the tracks a save deletes and adds.
#[derive(Debug, Clone)]
pub struct LibraryLayout {
    pub release_id: String,
    pub applied: bool,
    /// `[{position, carrier, sides}]`, the applied catalog's mediums.
    pub mediums: String,
    /// `[{track_id, medium, side, number, stated_length_ms, position}]`, the
    /// applied catalog's places, in playing order.
    pub places: String,
    /// `[{sheet_file_id, medium, side}]`.
    pub moves: String,
    pub edits: String,
}

/// An import draft's layout, which is no release's: its mediums and the
/// places of its tracks (as [`LibraryLayout`] holds them), the track each of
/// its CUE sheets carves (`carving`, `[{sheet_file_id, track_id}]`) and the
/// sides picked for its sheets (`moves`).
#[derive(Debug, Clone)]
pub struct DraftLayout {
    pub mediums: String,
    pub places: String,
    pub carving: String,
    pub moves: String,
}

// The layout a release editor's form gives its release's tracks: the JSON
// array `mediums` of the mediums it starts from, in order, the JSON array
// `tracks` of the tracks it lays out on them, in playing order, and the JSON
// array `honored` of the sides picked for CUE sheets that it gave the sheets,
// those on which a sheet's first laid track sits.
coven::read!(
    writer LaidReleaseLayout(
        release: String, applied: bool, mediums: String, places: String, moves: String,
        edits: String
    ) { laid_mediums: String, laid_tracks: String, honored: String },
    r#"
    WITH laid AS MATERIALIZED (
        SELECT place.*
        FROM @crate::records::layout_fragments::ReleaseLaidPlaces(
            release: :release, applied: :applied, mediums: :mediums, places: :places,
            moves: :moves, edits: :edits) AS place),
    firsts AS (
        SELECT cut.sheet_file_id, laid.medium, laid.side,
               ROW_NUMBER() OVER (PARTITION BY cut.sheet_file_id ORDER BY laid.position) AS n
        FROM laid
        JOIN carved_tracks AS cut ON cut.track_id = laid.track_id),
    picks AS (
        SELECT picked.value ->> 'sheet_file_id' AS sheet_file_id,
               picked.value ->> 'medium' AS medium, picked.value ->> 'side' AS side
        FROM json_each(:moves) AS picked)
    SELECT (SELECT json_group_array(json_object('carrier', base.carrier, 'sides', base.sides)
                       ORDER BY base.position)
            FROM @crate::records::layout_fragments::BaseMediums(
                release: :release, applied: :applied, mediums: :mediums) AS base)
               AS laid_mediums,
           (SELECT json_group_array(json_object(
                       'track_id', laid.track_id, 'medium', laid.medium, 'side', laid.side,
                       'number', laid.number, 'stated_length_ms', laid.stated_length_ms)
                       ORDER BY laid.position)
            FROM laid) AS laid_tracks,
           (SELECT json_group_array(json_object(
                       'sheet_file_id', picks.sheet_file_id, 'medium', picks.medium,
                       'side', picks.side))
            FROM picks
            JOIN firsts ON firsts.sheet_file_id = picks.sheet_file_id AND firsts.n = 1
            WHERE firsts.medium = picks.medium AND firsts.side = picks.side) AS honored
"#
);

// The layout an import draft's sheet moves give its tracks, as
// [`LaidReleaseLayout`] gives its mediums and tracks.
coven::read!(
    writer LaidDraftLayout(mediums: String, places: String, carving: String, moves: String) {
        laid_mediums: String,
        laid_tracks: String
    },
    r#"
    WITH given_mediums AS (
        SELECT given.*
        FROM @crate::records::layout_fragments::GivenMediums(mediums: :mediums) AS given),
    laid AS (
        SELECT place.*
        FROM @crate::records::layout_fragments::LaidPlaces(
            base_mediums: (SELECT given_mediums.position, given_mediums.carrier,
                                  given_mediums.sides
                           FROM given_mediums),
            base_places: (SELECT placed.value ->> 'track_id', placed.value ->> 'medium',
                                 placed.value ->> 'side', placed.value ->> 'number',
                                 placed.value ->> 'stated_length_ms',
                                 placed.value ->> 'position'
                          FROM json_each(:places) AS placed),
            picks: (SELECT picked.value ->> 'sheet_file_id', picked.value ->> 'medium',
                           picked.value ->> 'side'
                    FROM json_each(:moves) AS picked),
            carving: (SELECT cut.value ->> 'sheet_file_id', cut.value ->> 'track_id'
                      FROM json_each(:carving) AS cut),
            dropped: (SELECT NULL WHERE 0),
            added: (SELECT NULL, NULL, NULL WHERE 0)) AS place)
    SELECT (SELECT json_group_array(
                       json_object('carrier', given_mediums.carrier,
                                   'sides', given_mediums.sides)
                       ORDER BY given_mediums.position)
            FROM given_mediums) AS laid_mediums,
           (SELECT json_group_array(json_object(
                       'track_id', laid.track_id, 'medium', laid.medium, 'side', laid.side,
                       'number', laid.number, 'stated_length_ms', laid.stated_length_ms)
                       ORDER BY laid.position)
            FROM laid) AS laid_tracks
"#
);

/// The layout a release editor's form gives its release's tracks, with the
/// sides picked for CUE sheets that it gave the sheets: each a sheet's file
/// id with the side.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LaidRelease {
    pub layout: ReleaseLayout,
    pub honored: Vec<(String, bae_types::mediums::MediumSide)>,
}

/// The layout `layout` asks for, as the library holds the release now.
pub fn laid_out_on(
    sql: &impl bae_store::WriterRead,
    layout: &LibraryLayout,
) -> Result<LaidRelease, DbError> {
    let statement = LaidReleaseLayoutParams {
        release: layout.release_id.clone(),
        applied: layout.applied,
        mediums: layout.mediums.clone(),
        places: layout.places.clone(),
        moves: layout.moves.clone(),
        edits: layout.edits.clone(),
    };
    let row = statement_row(sql, &statement)?;
    let honored = rows_of::<HonoredJson>(&row.honored, "the sides a layout gave its sheets")?
        .into_iter()
        .map(|pick| {
            let side = bae_types::mediums::MediumSide {
                medium: fits(pick.medium, "a picked medium")?,
                side: fits(pick.side, "a picked side")?,
            };
            Ok((pick.sheet_file_id, side))
        })
        .collect::<Result<_, DbError>>()?;
    Ok(LaidRelease { layout: layout_of(&row.laid_mediums, &row.laid_tracks)?, honored })
}

/// The layout an import draft's sheet moves give its tracks.
pub fn draft_laid_out_on(
    sql: &impl bae_store::WriterRead,
    layout: &DraftLayout,
) -> Result<ReleaseLayout, DbError> {
    let statement = LaidDraftLayoutParams {
        mediums: layout.mediums.clone(),
        places: layout.places.clone(),
        carving: layout.carving.clone(),
        moves: layout.moves.clone(),
    };
    let row = statement_row(sql, &statement)?;
    layout_of(&row.laid_mediums, &row.laid_tracks)
}

/// One medium as a layout read writes it.
#[derive(serde::Deserialize)]
struct MediumJson {
    carrier: Option<String>,
    sides: Option<i64>,
}

/// One laid track as a layout read writes it.
#[derive(serde::Deserialize)]
struct TrackJson {
    track_id: String,
    medium: i64,
    side: i64,
    number: i64,
    stated_length_ms: Option<i64>,
}

/// One honored side as [`LaidReleaseLayout`] writes it.
#[derive(serde::Deserialize)]
struct HonoredJson {
    sheet_file_id: String,
    medium: i64,
    side: i64,
}

/// The layout whose mediums and tracks a layout read wrote as the JSON arrays
/// `mediums` and `tracks`.
fn layout_of(mediums: &str, tracks: &str) -> Result<ReleaseLayout, DbError> {
    let mediums = rows_of::<MediumJson>(mediums, "a layout's mediums")?
        .into_iter()
        .map(|medium| medium_of(medium.carrier, medium.sides))
        .collect::<Result<_, _>>()?;
    let tracks = rows_of::<TrackJson>(tracks, "a layout's tracks")?
        .into_iter()
        .map(laid_track)
        .collect::<Result<_, _>>()?;
    Ok(ReleaseLayout { mediums, tracks })
}

/// `value`, which holds `what`, in the integer type a layout keeps it in.
fn fits<T: TryFrom<i64>>(value: i64, what: &str) -> Result<T, DbError> {
    T::try_from(value).map_err(|_error| DbError::unreadable(what, value))
}

/// The medium of one row of a layout's mediums.
fn medium_of(carrier: Option<String>, sides: Option<i64>) -> Result<ReleaseMedium, DbError> {
    let carrier = carrier
        .map(|key| {
            bae_types::pressing::Medium::from_key(&key)
                .ok_or_else(|| DbError::unreadable("a laid medium's carrier", key))
        })
        .transpose()?;
    let sides = sides
        .and_then(|sides| u8::try_from(sides).ok())
        .and_then(std::num::NonZeroU8::new)
        .ok_or_else(|| DbError::unreadable("a laid medium's sides", sides))?;
    Ok(ReleaseMedium { carrier, sides })
}

/// The track of one row of a layout's tracks.
fn laid_track(row: TrackJson) -> Result<LaidTrack, DbError> {
    let TrackJson { track_id, medium, side, number, stated_length_ms } = row;
    let unreadable = || DbError::unreadable("a laid track's place", &track_id);
    let place = TrackPlace {
        medium: std::num::NonZeroU16::new(fits(medium, "a laid track's medium")?)
            .ok_or_else(unreadable)?,
        side: std::num::NonZeroU8::new(fits(side, "a laid track's side")?)
            .ok_or_else(unreadable)?,
        number: std::num::NonZeroU32::new(fits(number, "a laid track's number")?)
            .ok_or_else(unreadable)?,
    };
    let stated_length_ms =
        stated_length_ms.map(|length| fits(length, "a laid track's listed length")).transpose()?;
    Ok(LaidTrack { track_id, place, stated_length_ms })
}

#[cfg(desktop)]
coven::write!(
    MoveTracksPastEnd(release_id: &'p str),
    r#"
    WITH past_end(value) AS MATERIALIZED (
        SELECT MAX(position) + 1 FROM tracks WHERE release_id = :release_id)
    UPDATE tracks SET position = position + (SELECT value FROM past_end)
    WHERE release_id = :release_id
"#
);

#[cfg(desktop)]
coven::write!(
    PlaceTrack(
        id: &'p str,
        release_id: &'p str,
        medium_id: &'p str,
        side: std::num::NonZeroU8,
        track_number: std::num::NonZeroU32,
        stated_length_ms: Option<u64>,
        position: i64,
    ),
    r#"
    UPDATE tracks
    SET medium_id = :medium_id, side = :side, track_number = :track_number,
        stated_length_ms = :stated_length_ms, position = :position
    WHERE id = :id AND release_id = :release_id
"#
);

/// Write `layout` over the release `release_id`: each medium at its
/// position, a position the release held keeping its row, and each track's
/// medium, side, number, listed length and playing position. A medium the
/// layout no longer holds goes after the tracks leave it.
#[cfg(desktop)]
pub(super) fn write_layout_on(
    tx: &CovenWrite<'_>,
    release_id: &str,
    layout: &ReleaseLayout,
    ids: &dyn coven::IdSource,
    now: &str,
) -> Result<(), DbError> {
    let held = mediums_held_on(tx, release_id)?;
    let mut medium_ids = Vec::with_capacity(layout.mediums.len());
    for (index, medium) in layout.mediums.iter().enumerate() {
        let position = u16::try_from(index + 1).expect("a release holds fewer than 65536 mediums");
        let id = match held.iter().find(|(held, _)| *held == position) {
            Some((_, id)) => {
                update_release_medium_row(tx, id, medium)?;
                id.clone()
            }
            None => {
                let id = ids.new_id().to_string();
                tx.run(&super::write::InsertReleaseMediumParams {
                    id: &id,
                    release_id,
                    position: std::num::NonZeroU16::new(position)
                        .expect("a medium's position starts at one"),
                    medium: medium.carrier.map(bae_types::pressing::Medium::key),
                    side_count: medium.sides,
                    created_at: now.to_string(),
                })?;
                id
            }
        };
        medium_ids.push(id);
    }
    // Playing positions are unique in a release, so every track first moves
    // past the end, and then each takes its place.
    tx.run(&MoveTracksPastEndParams { release_id })?;
    for (position, track) in layout.tracks.iter().enumerate() {
        tx.run(&PlaceTrackParams {
            id: &track.track_id,
            release_id,
            medium_id: &medium_ids[track.place.medium_index()],
            side: track.place.side,
            track_number: track.place.number,
            stated_length_ms: track.stated_length_ms,
            position: position as i64,
        })?;
    }
    for (_, stale) in held.iter().filter(|(position, _)| usize::from(*position) > medium_ids.len())
    {
        delete_release_medium_row(tx, stale)?;
    }
    Ok(())
}

#[cfg(test)]
#[path = "layout_tests.rs"]
mod tests;
