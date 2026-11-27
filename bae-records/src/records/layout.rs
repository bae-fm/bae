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

use super::*;
use bae_store::statement_rows;
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

// The mediums a release editor's layout starts from, in order.
sqlive_compose::read!(
    LaidMediums(release: String, applied: bool, mediums: String),
    r#"
    SELECT base.*
    FROM @crate::records::layout_fragments::BaseMediums(
        release: :release, applied: :applied, mediums: :mediums) AS base
    ORDER BY base.position
"#
);

// The tracks a release editor's layout lays out, in playing order.
sqlive_compose::read!(
    LaidReleaseTracks(
        release: String,
        applied: bool,
        mediums: String,
        places: String,
        moves: String,
        edits: String,
    ),
    r#"
    SELECT laid.*
    FROM @crate::records::layout_fragments::ReleaseLaidPlaces(
        release: :release, applied: :applied, mediums: :mediums, places: :places,
        moves: :moves, edits: :edits) AS laid
    ORDER BY laid.position
"#
);

// The sides picked for CUE sheets that a release editor's layout gave the
// sheets: those on which a sheet's first laid track sits.
sqlive_compose::read!(
    HonoredSheetPicks(
        release: String, applied: bool, mediums: String, places: String, moves: String,
        edits: String
    ) { sheet_file_id: String, medium: i64, side: i64 },
    r#"
    WITH laid AS (
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
    SELECT picks.sheet_file_id AS sheet_file_id, picks.medium AS medium, picks.side AS side
    FROM picks
    JOIN firsts ON firsts.sheet_file_id = picks.sheet_file_id AND firsts.n = 1
    WHERE firsts.medium = picks.medium AND firsts.side = picks.side
"#
);

// An import draft's mediums, in order.
sqlive_compose::read!(
    DraftLayoutMediums(mediums: String),
    r#"
    SELECT given.*
    FROM @crate::records::layout_fragments::GivenMediums(mediums: :mediums) AS given
    ORDER BY given.position
"#
);

// The tracks of an import draft's layout, in playing order.
sqlive_compose::read!(
    LaidDraftTracks(mediums: String, places: String, carving: String, moves: String),
    r#"
    WITH given_mediums AS (
        SELECT given.*
        FROM @crate::records::layout_fragments::GivenMediums(mediums: :mediums) AS given)
    SELECT laid.*
    FROM @crate::records::layout_fragments::LaidPlaces(
        base_mediums: (SELECT position, carrier, sides FROM given_mediums),
        base_places: (SELECT placed.value ->> 'track_id', placed.value ->> 'medium',
                             placed.value ->> 'side', placed.value ->> 'number',
                             placed.value ->> 'stated_length_ms', placed.value ->> 'position'
                      FROM json_each(:places) AS placed),
        picks: (SELECT picked.value ->> 'sheet_file_id', picked.value ->> 'medium',
                       picked.value ->> 'side'
                FROM json_each(:moves) AS picked),
        carving: (SELECT cut.value ->> 'sheet_file_id', cut.value ->> 'track_id'
                  FROM json_each(:carving) AS cut),
        dropped: (SELECT NULL WHERE 0),
        added: (SELECT NULL, NULL, NULL WHERE 0)) AS laid
    ORDER BY laid.position
"#
);

/// The sides picked for CUE sheets that the layout gave the sheets: each a
/// sheet's file id with the side.
pub fn honored_picks_on(
    sql: &impl coven::SqlRead,
    layout: &LibraryLayout,
) -> Result<Vec<(String, bae_types::mediums::MediumSide)>, DbError> {
    let params = HonoredSheetPicksParams {
        release: layout.release_id.clone(),
        applied: layout.applied,
        mediums: layout.mediums.clone(),
        places: layout.places.clone(),
        moves: layout.moves.clone(),
        edits: layout.edits.clone(),
    };
    statement_rows(sql, &params)?
        .into_iter()
        .map(|row| {
            let side = bae_types::mediums::MediumSide {
                medium: fits(row.medium, "a picked medium")?,
                side: fits(row.side, "a picked side")?,
            };
            Ok((row.sheet_file_id, side))
        })
        .collect()
}

/// The layout a release editor's form gives its release's tracks.
pub fn laid_out_on(
    sql: &impl coven::SqlRead,
    layout: &LibraryLayout,
) -> Result<ReleaseLayout, DbError> {
    let mediums = LaidMediumsParams {
        release: layout.release_id.clone(),
        applied: layout.applied,
        mediums: layout.mediums.clone(),
    };
    let mediums = statement_rows(sql, &mediums)?
        .into_iter()
        .map(|row| medium_of(row.base.carrier, row.base.sides))
        .collect::<Result<_, _>>()?;
    let tracks = LaidReleaseTracksParams {
        release: layout.release_id.clone(),
        applied: layout.applied,
        mediums: layout.mediums.clone(),
        places: layout.places.clone(),
        moves: layout.moves.clone(),
        edits: layout.edits.clone(),
    };
    let tracks = statement_rows(sql, &tracks)?
        .into_iter()
        .map(|row| laid_track(row.laid.into_laid()))
        .collect::<Result<_, _>>()?;
    Ok(ReleaseLayout { mediums, tracks })
}

/// The layout an import draft's sheet moves give its tracks.
pub fn draft_laid_out_on(
    sql: &impl coven::SqlRead,
    layout: &DraftLayout,
) -> Result<ReleaseLayout, DbError> {
    let mediums = DraftLayoutMediumsParams { mediums: layout.mediums.clone() };
    let mediums = statement_rows(sql, &mediums)?
        .into_iter()
        .map(|row| medium_of(row.given.carrier, row.given.sides))
        .collect::<Result<_, _>>()?;
    let tracks = LaidDraftTracksParams {
        mediums: layout.mediums.clone(),
        places: layout.places.clone(),
        carving: layout.carving.clone(),
        moves: layout.moves.clone(),
    };
    let tracks = statement_rows(sql, &tracks)?
        .into_iter()
        .map(|row| laid_track(row.laid))
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
fn laid_track(row: super::layout_fragments::LaidPlaces) -> Result<LaidTrack, DbError> {
    let (track_id, medium, side, number, stated_length_ms) = row.into_place();
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
    let held: Vec<(u16, String)> = tx.query(
        "SELECT position, id FROM release_mediums WHERE release_id = ? ORDER BY position",
        params![release_id],
        |row| Ok((row.get(0)?, row.get(1)?)),
    )?;
    let mut medium_ids = Vec::with_capacity(layout.mediums.len());
    for (index, medium) in layout.mediums.iter().enumerate() {
        let position = u16::try_from(index + 1).expect("a release holds fewer than 65536 mediums");
        let carrier = medium.carrier.map(bae_types::pressing::Medium::key);
        let id = match held.iter().find(|(held, _)| *held == position) {
            Some((_, id)) => {
                tx.execute(
                    "UPDATE release_mediums SET medium = ?, side_count = ? WHERE id = ?",
                    params![carrier, medium.sides, id],
                )?;
                id.clone()
            }
            None => {
                let id = ids.new_id().to_string();
                tx.execute(
                    "INSERT INTO release_mediums \
                         (id, release_id, position, medium, side_count, created_at) \
                     VALUES (?, ?, ?, ?, ?, ?)",
                    params![id, release_id, position, carrier, medium.sides, now],
                )?;
                id
            }
        };
        medium_ids.push(id);
    }
    // Playing positions are unique in a release, so every track first moves
    // past the end, and then each takes its place.
    tx.execute(
        "WITH past_end(value) AS MATERIALIZED ( \
             SELECT MAX(position) + 1 FROM tracks WHERE release_id = ?1) \
         UPDATE tracks SET position = position + (SELECT value FROM past_end) WHERE release_id = ?1",
        [release_id],
    )?;
    for (position, track) in layout.tracks.iter().enumerate() {
        tx.execute(
            "UPDATE tracks SET medium_id = ?, side = ?, track_number = ?, \
                 stated_length_ms = ?, position = ? \
             WHERE id = ? AND release_id = ?",
            params![
                medium_ids[track.place.medium_index()],
                track.place.side,
                track.place.number,
                track.stated_length_ms,
                position,
                track.track_id,
                release_id
            ],
        )?;
    }
    for (_, stale) in held.iter().filter(|(position, _)| usize::from(*position) > medium_ids.len())
    {
        tx.execute("DELETE FROM release_mediums WHERE id = ?", params![stale])?;
    }
    Ok(())
}

#[cfg(test)]
#[path = "layout_tests.rs"]
mod tests;
