//! Where a release's tracks sit on its mediums once a release editor's form
//! is saved: the mediums in order, and each track in playing order with its
//! place and the length its source lists for it.
//!
//! The form puts the tracks at the places an applied catalog gives them, or
//! the library's own, and moves each CUE sheet the person picked a disc for
//! onto that disc. One statement lays them out ([`layout_ctes`]), so the
//! editor's draft, which shows the rows laid out, and the save, which writes
//! them, never disagree.

use super::*;
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

/// What the statements of [`layout_ctes`] read: the release, whether a
/// catalog was applied (its mediums and places are `mediums` and `places`,
/// JSON arrays) and the discs picked for CUE sheets (`moves`, a JSON array
/// of objects with a `sheet_file_id` and a `disc`).
pub struct LayoutParams<'a> {
    pub release_id: &'a str,
    pub applied: bool,
    /// `[{position, carrier, sides}]`, the applied catalog's mediums.
    pub mediums: &'a str,
    /// `[{track_id, medium, side, number, stated_length_ms, position}]`, the
    /// applied catalog's places, in playing order.
    pub places: &'a str,
    pub moves: &'a str,
}

impl<'a> LayoutParams<'a> {
    /// The named parameters the statements of [`layout_ctes`] bind.
    pub fn named(&'a self) -> [(&'static str, &'a dyn rusqlite::ToSql); 5] {
        [
            (":release", &self.release_id),
            (":applied", &self.applied),
            (":mediums", &self.mediums),
            (":places", &self.places),
            (":moves", &self.moves),
        ]
    }
}

/// The common table expressions that lay a release out, for statements
/// binding [`LayoutParams::named`]: `laid_mediums` (`position`, `carrier`,
/// `sides`), `laid_places` (`track_id`, `medium`, `side`, `number`,
/// `stated_length_ms`, `position`), `laid_sheets` (each CUE sheet's
/// `sheet_file_id` with the `disc` its first track sits on) and `honored`
/// (the sheets whose picked disc the layout gave them).
///
/// The layout starts from the applied catalog's places, else the library's.
/// A sheet picked onto a disc the layout holds, with a track not on it,
/// moves all its tracks there, after the tracks already on that disc, on its
/// last side, each numbered one more than the highest number on that side,
/// so no two tracks of a side share a number; sheets going to one disc keep
/// the order of their first tracks. Then the media no track sits on are
/// dropped, the rest numbered from one, and the tracks take their playing
/// order from their places. A form that moves no sheet lays the tracks out
/// as they were.
pub fn layout_ctes() -> &'static str {
    "base_mediums (position, carrier, sides) AS ( \
         SELECT j.value ->> 'position', j.value ->> 'carrier', j.value ->> 'sides' \
         FROM json_each(:mediums) AS j WHERE :applied \
         UNION ALL \
         SELECT position, medium, side_count FROM release_mediums \
         WHERE release_id = :release AND NOT :applied), \
     base_places (track_id, medium, side, number, stated_length_ms, position) AS ( \
         SELECT j.value ->> 'track_id', j.value ->> 'medium', j.value ->> 'side', \
                j.value ->> 'number', j.value ->> 'stated_length_ms', j.value ->> 'position' \
         FROM json_each(:places) AS j WHERE :applied \
         UNION ALL \
         SELECT t.id, m.position, t.side, t.track_number, t.stated_length_ms, t.position \
         FROM tracks t JOIN release_mediums m ON m.id = t.medium_id \
         WHERE t.release_id = :release AND NOT :applied), \
     picks (sheet_file_id, disc) AS ( \
         SELECT j.value ->> 'sheet_file_id', j.value ->> 'disc' FROM json_each(:moves) AS j), \
     sheet_tracks (sheet_file_id, track_id, position, first_position) AS ( \
         SELECT c.sheet_file_id, c.track_id, p.position, \
                MIN(p.position) OVER (PARTITION BY c.sheet_file_id) \
         FROM carved_tracks c JOIN base_places p ON p.track_id = c.track_id), \
     moves (track_id, disc, first_position, position) AS ( \
         SELECT st.track_id, pick.disc, st.first_position, st.position \
         FROM picks pick JOIN sheet_tracks st ON st.sheet_file_id = pick.sheet_file_id \
         WHERE pick.disc IN (SELECT position FROM base_mediums) \
           AND EXISTS (SELECT 1 FROM sheet_tracks other \
                       JOIN base_places place ON place.track_id = other.track_id \
                       WHERE other.sheet_file_id = pick.sheet_file_id \
                         AND place.medium <> pick.disc)), \
     moved (track_id, medium, side, number) AS ( \
         SELECT mv.track_id, mv.disc, target.sides, \
                COALESCE((SELECT MAX(place.number) FROM base_places place \
                          WHERE place.medium = mv.disc AND place.side = target.sides \
                            AND place.track_id NOT IN (SELECT track_id FROM moves)), 0) \
                + ROW_NUMBER() OVER (PARTITION BY mv.disc \
                                     ORDER BY mv.first_position, mv.position) \
         FROM moves mv JOIN base_mediums target ON target.position = mv.disc), \
     kept (track_id, medium, side, number, stated_length_ms, position) AS ( \
         SELECT track_id, medium, side, number, stated_length_ms, position FROM base_places \
         WHERE track_id NOT IN (SELECT track_id FROM moves) \
         UNION ALL \
         SELECT mv.track_id, mv.medium, mv.side, mv.number, place.stated_length_ms, \
                place.position \
         FROM moved mv JOIN base_places place ON place.track_id = mv.track_id), \
     moving (yes) AS (SELECT EXISTS (SELECT 1 FROM moves)), \
     used (medium, new_medium) AS ( \
         SELECT medium, DENSE_RANK() OVER (ORDER BY medium) \
         FROM (SELECT DISTINCT medium FROM kept)), \
     laid_places (track_id, medium, side, number, stated_length_ms, position) AS ( \
         SELECT k.track_id, CASE WHEN moving.yes THEN u.new_medium ELSE k.medium END, \
                k.side, k.number, k.stated_length_ms, \
                CASE WHEN moving.yes \
                     THEN ROW_NUMBER() OVER (ORDER BY u.new_medium, k.side, k.number) - 1 \
                     ELSE k.position END \
         FROM kept k JOIN used u ON u.medium = k.medium, moving), \
     laid_mediums (position, carrier, sides) AS ( \
         SELECT CASE WHEN moving.yes THEN u.new_medium ELSE m.position END, m.carrier, m.sides \
         FROM base_mediums m LEFT JOIN used u ON u.medium = m.position, moving \
         WHERE NOT moving.yes OR u.medium IS NOT NULL), \
     laid_sheets (sheet_file_id, disc) AS ( \
         SELECT st.sheet_file_id, place.medium FROM sheet_tracks st \
         JOIN laid_places place ON place.track_id = st.track_id \
         WHERE st.position = st.first_position), \
     honored (sheet_file_id, disc) AS ( \
         SELECT pick.sheet_file_id, pick.disc FROM picks pick \
         JOIN laid_sheets sheet ON sheet.sheet_file_id = pick.sheet_file_id \
         WHERE sheet.disc = pick.disc)"
}

/// A statement's table of the laid-out mediums in the shape
/// [`MediumsSource::Draft`] takes for `mediums`: one row per medium with the
/// columns of `release_mediums` it reads.
pub fn laid_mediums_source() -> String {
    format!(
        "(WITH {} SELECT position AS id, :release AS release_id, position, \
                 carrier AS medium, sides AS side_count FROM laid_mediums)",
        layout_ctes()
    )
}

/// A statement's table of the laid-out places in the shape
/// [`MediumsSource::Draft`] takes for `places`.
pub fn laid_places_source() -> String {
    format!(
        "(WITH {} SELECT track_id AS id, medium AS medium_id, side, number AS track_number, \
                 stated_length_ms, position FROM laid_places)",
        layout_ctes()
    )
}

/// The disc each CUE sheet of the release is on once the layout is written,
/// by the sheet's file id.
pub fn laid_sheet_discs_on(
    sql: &impl coven::SqlRead,
    layout: &LayoutParams<'_>,
) -> Result<Vec<(String, u32)>, DbError> {
    Ok(sql.query(
        &format!("WITH {} SELECT sheet_file_id, disc FROM laid_sheets", layout_ctes()),
        layout.named().as_slice(),
        |row| Ok((row.get(0)?, row.get(1)?)),
    )?)
}

/// The discs picked for CUE sheets that the layout gave the sheets: each a
/// sheet's file id with the disc.
pub fn honored_picks_on(
    sql: &impl coven::SqlRead,
    layout: &LayoutParams<'_>,
) -> Result<Vec<(String, u32)>, DbError> {
    Ok(sql.query(
        &format!("WITH {} SELECT sheet_file_id, disc FROM honored", layout_ctes()),
        layout.named().as_slice(),
        |row| Ok((row.get(0)?, row.get(1)?)),
    )?)
}

/// How many discs the layout holds.
pub fn laid_disc_count_on(
    sql: &impl coven::SqlRead,
    layout: &LayoutParams<'_>,
) -> Result<usize, DbError> {
    Ok(sql.query_row(
        &format!("WITH {} SELECT COUNT(*) FROM laid_mediums", layout_ctes()),
        layout.named().as_slice(),
        |row| row.get(0),
    )?)
}

/// The layout the statements of [`layout_ctes`] give, as one value.
pub fn laid_out_on(
    sql: &impl coven::SqlRead,
    layout: &LayoutParams<'_>,
) -> Result<ReleaseLayout, DbError> {
    let mediums = sql.query(
        &format!(
            "WITH {} SELECT carrier, sides FROM laid_mediums ORDER BY position",
            layout_ctes()
        ),
        layout.named().as_slice(),
        |row| {
            let carrier: Option<String> = row.get(0)?;
            let carrier = carrier
                .map(|key| {
                    bae_types::pressing::Medium::from_key(&key).ok_or_else(|| {
                        rusqlite::Error::InvalidParameterName(format!("unknown medium {key:?}"))
                    })
                })
                .transpose()?;
            Ok(ReleaseMedium { carrier, sides: row.get(1)? })
        },
    )?;
    let tracks = sql.query(
        &format!(
            "WITH {} SELECT track_id, medium, side, number, stated_length_ms \
             FROM laid_places ORDER BY position",
            layout_ctes()
        ),
        layout.named().as_slice(),
        |row| {
            Ok(LaidTrack {
                track_id: row.get(0)?,
                place: TrackPlace { medium: row.get(1)?, side: row.get(2)?, number: row.get(3)? },
                stated_length_ms: row.get(4)?,
            })
        },
    )?;
    Ok(ReleaseLayout { mediums, tracks })
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
        "UPDATE tracks SET position = position + ? WHERE release_id = ?",
        params![layout.tracks.len(), release_id],
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

/// Make each CUE sheet of `release_id` the disc its first track sits on.
#[cfg(desktop)]
pub(super) fn refresh_sheet_discs_on(tx: &CovenWrite<'_>, release_id: &str) -> Result<(), DbError> {
    tx.execute(
        "UPDATE release_carving_sheets SET disc_number = ( \
             SELECT m.position FROM carved_tracks c \
             JOIN tracks t ON t.id = c.track_id \
             JOIN release_mediums m ON m.id = t.medium_id \
             WHERE c.sheet_file_id = release_carving_sheets.file_id \
             ORDER BY t.position LIMIT 1) \
         WHERE file_id IN (SELECT id FROM release_files WHERE release_id = ?)",
        params![release_id],
    )?;
    Ok(())
}

#[cfg(test)]
#[path = "layout_tests.rs"]
mod tests;
