//! Where a release's tracks sit on its mediums once a release editor's form
//! is saved: the mediums in order, and each track in playing order with its
//! place and the length its source lists for it.
//!
//! The form puts the tracks at the places an applied catalog gives them, or
//! the library's own, and moves each CUE sheet the person picked a side or
//! disc for onto that side. One statement lays them out ([`layout_ctes`]), so the
//! editor's draft, which shows the rows laid out, and the save, which writes
//! them, never disagree, and an import draft moves a sheet the same way.

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

/// What the statements of [`layout_ctes`] read: the release, whether the
/// layout starts from `mediums` and `places` (JSON arrays: an applied
/// catalog's, or an import draft's) rather than the release's own, the CUE
/// sheets that carve tracks the library does not hold (`carving`), and the
/// sides picked for CUE sheets (`moves`, a JSON array of objects with a
/// `sheet_file_id`, a `medium` and a `side`), and what the form does to the
/// sheets (`edits`, the JSON object of a
/// [`SheetEdits`](super::sheet_files::SheetEdits)), from which the
/// statements work out the tracks a save deletes and adds.
pub struct LayoutParams<'a> {
    /// The release, `None` for an import draft, which is no release yet.
    pub release_id: Option<&'a str>,
    pub applied: bool,
    /// `[{position, carrier, sides}]`, the applied catalog's mediums.
    pub mediums: &'a str,
    /// `[{track_id, medium, side, number, stated_length_ms, position}]`, the
    /// applied catalog's places, in playing order.
    pub places: &'a str,
    /// `[{sheet_file_id, track_id}]`, each track of `places` a CUE sheet
    /// carves that `carved_tracks` does not hold, such as an import draft's;
    /// `[]` for a library release.
    pub carving: &'a str,
    pub moves: &'a str,
    pub edits: &'a str,
}

impl<'a> LayoutParams<'a> {
    /// The named parameters the statements of [`layout_ctes`] bind.
    pub fn named(&'a self) -> [(&'static str, &'a dyn rusqlite::ToSql); 7] {
        [
            (":release", &self.release_id),
            (":applied", &self.applied),
            (":mediums", &self.mediums),
            (":places", &self.places),
            (":carving", &self.carving),
            (":moves", &self.moves),
            (":edits", &self.edits),
        ]
    }
}

/// Whether the form's edits delete or add a track, as an SQL condition over
/// the `dropped_tracks` of [`sheet_changes_ctes`]: every track a save adds
/// replaces deleted ones.
const CHANGING: &str = "EXISTS (SELECT 1 FROM dropped_tracks)";

/// The common table expressions that lay a release out, for statements
/// binding [`LayoutParams::named`]: `laid_mediums` (`position`, `carrier`,
/// `sides`), `laid_places` (`track_id`, `medium`, `side`, `number`,
/// `stated_length_ms`, `position`), `laid_sheets` (each CUE sheet's
/// `sheet_file_id` with the `medium` and `side` its first track sits on) and
/// `honored` (the sheets whose picked side the layout gave them).
///
/// The layout starts from the given places, else the library's.
/// A sheet picked onto a side the layout holds, with a track not on it,
/// moves all its tracks there, after the tracks already on that side, each
/// numbered one more than the highest number on that side, so no two tracks
/// of a side share a number; sheets going to one side keep the order of
/// their first tracks. Then the media no track sits on are
/// dropped, the rest numbered from one, and the tracks take their playing
/// order from their places. A form that moves no sheet lays the tracks out
/// as they were.
///
/// A form that deletes or adds tracks (`edits`) lays out what is left: the
/// deleted tracks go, each added track takes the place of its anchor, the
/// first track of the sheet whose tracks it replaces, ahead of the track
/// that holds it, and every side numbers its tracks from one in playing
/// order. The media no track sits on are then dropped, as for a move. An
/// added track is named by its file's id. The tracks the edits delete and
/// add are those of [`sheet_changes_ctes`], which come first.
pub fn layout_ctes() -> String {
    let changes = super::sheet_files::sheet_changes_ctes(":release", ":edits");
    format!("{changes}, {LAYOUT}").replace("{changing}", CHANGING)
}

const LAYOUT: &str = "base_mediums (position, carrier, sides) AS ( \
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
     picks (sheet_file_id, medium, side) AS ( \
         SELECT j.value ->> 'sheet_file_id', j.value ->> 'medium', j.value ->> 'side' \
         FROM json_each(:moves) AS j), \
     carving (sheet_file_id, track_id) AS ( \
         SELECT sheet_file_id, track_id FROM carved_tracks \
         UNION ALL \
         SELECT j.value ->> 'sheet_file_id', j.value ->> 'track_id' FROM json_each(:carving) AS j), \
     sheet_tracks (sheet_file_id, track_id, position, first_position) AS ( \
         SELECT c.sheet_file_id, c.track_id, p.position, \
                MIN(p.position) OVER (PARTITION BY c.sheet_file_id) \
         FROM carving c JOIN base_places p ON p.track_id = c.track_id), \
     moves (track_id, medium, side, first_position, position) AS ( \
         SELECT st.track_id, pick.medium, pick.side, st.first_position, st.position \
         FROM picks pick JOIN sheet_tracks st ON st.sheet_file_id = pick.sheet_file_id \
         WHERE EXISTS (SELECT 1 FROM base_mediums target \
                       WHERE target.position = pick.medium \
                         AND pick.side BETWEEN 1 AND target.sides) \
           AND EXISTS (SELECT 1 FROM sheet_tracks other \
                       JOIN base_places place ON place.track_id = other.track_id \
                       WHERE other.sheet_file_id = pick.sheet_file_id \
                         AND (place.medium <> pick.medium OR place.side <> pick.side))), \
     moved (track_id, medium, side, number) AS ( \
         SELECT mv.track_id, mv.medium, mv.side, \
                COALESCE((SELECT MAX(place.number) FROM base_places place \
                          WHERE place.medium = mv.medium AND place.side = mv.side \
                            AND place.track_id NOT IN (SELECT track_id FROM moves)), 0) \
                + ROW_NUMBER() OVER (PARTITION BY mv.medium, mv.side \
                                     ORDER BY mv.first_position, mv.position) \
         FROM moves mv), \
     kept (track_id, medium, side, number, stated_length_ms, position) AS ( \
         SELECT track_id, medium, side, number, stated_length_ms, position FROM base_places \
         WHERE track_id NOT IN (SELECT track_id FROM moves) \
         UNION ALL \
         SELECT mv.track_id, mv.medium, mv.side, mv.number, place.stated_length_ms, \
                place.position \
         FROM moved mv JOIN base_places place ON place.track_id = mv.track_id), \
     numbered (track_id, medium, side, number, stated_length_ms, position) AS ( \
         SELECT CASE WHEN pass.fresh THEN a.file_id ELSE k.track_id END, k.medium, k.side, \
                CASE WHEN {changing} \
                     THEN ROW_NUMBER() OVER (PARTITION BY k.medium, k.side \
                                             ORDER BY k.number, pass.fresh DESC, a.ord, k.position) \
                     ELSE k.number END, \
                CASE WHEN pass.fresh THEN NULL ELSE k.stated_length_ms END, k.position \
         FROM kept k \
         JOIN (SELECT 0 AS fresh UNION ALL SELECT 1) AS pass \
         LEFT JOIN (SELECT added.file_id, st.track_id AS anchor, \
                           added.first_position AS ord \
                    FROM added_files added JOIN sheet_tracks st \
                        ON st.sheet_file_id = added.sheet_file_id \
                       AND st.position = st.first_position) AS a \
             ON pass.fresh AND a.anchor = k.track_id \
         WHERE (NOT pass.fresh AND k.track_id NOT IN (SELECT track_id FROM dropped_tracks)) \
            OR (pass.fresh AND a.file_id IS NOT NULL)), \
     moving (yes) AS (SELECT EXISTS (SELECT 1 FROM moves) OR {changing}), \
     used (medium, new_medium) AS ( \
         SELECT medium, DENSE_RANK() OVER (ORDER BY medium) \
         FROM (SELECT DISTINCT medium FROM numbered)), \
     laid_places (track_id, medium, side, number, stated_length_ms, position) AS ( \
         SELECT k.track_id, CASE WHEN moving.yes THEN k.new_medium ELSE k.medium END, \
                k.side, k.number, k.stated_length_ms, \
                CASE WHEN moving.yes \
                     THEN ROW_NUMBER() OVER (ORDER BY k.new_medium, k.side, k.number) - 1 \
                     ELSE k.position END \
         FROM (SELECT n.*, DENSE_RANK() OVER (ORDER BY n.medium) AS new_medium \
               FROM numbered n) k, moving), \
     laid_mediums (position, carrier, sides) AS ( \
         SELECT CASE WHEN moving.yes THEN u.new_medium ELSE m.position END, m.carrier, m.sides \
         FROM base_mediums m LEFT JOIN used u ON u.medium = m.position, moving \
         WHERE NOT moving.yes OR u.medium IS NOT NULL), \
     laid_sheets (sheet_file_id, medium, side) AS ( \
         SELECT sheet_file_id, medium, side FROM ( \
             SELECT st.sheet_file_id, place.medium, place.side, \
                    ROW_NUMBER() OVER (PARTITION BY st.sheet_file_id \
                                       ORDER BY place.position) AS n \
             FROM sheet_tracks st JOIN laid_places place ON place.track_id = st.track_id) \
         WHERE n = 1), \
     honored (sheet_file_id, medium, side) AS ( \
         SELECT pick.sheet_file_id, pick.medium, pick.side FROM picks pick \
         JOIN laid_sheets sheet ON sheet.sheet_file_id = pick.sheet_file_id \
         WHERE sheet.medium = pick.medium AND sheet.side = pick.side)";

/// A statement's table of the laid-out mediums in the shape
/// [`MediumsSource::Draft`] takes for `mediums`: one row per medium with the
/// columns of `release_mediums` it reads.
pub fn laid_mediums_source() -> String {
    mediums_source("laid_mediums")
}

fn mediums_source(mediums: &str) -> String {
    format!(
        "(WITH {} SELECT position AS id, :release AS release_id, position, \
                 carrier AS medium, sides AS side_count FROM {mediums})",
        layout_ctes()
    )
}

/// A statement's table of the laid-out places in the shape
/// [`MediumsSource::Draft`] takes for `places`.
pub fn laid_places_source() -> String {
    format!(
        "(WITH {} SELECT track_id AS id, :release AS release_id, medium AS medium_id, side, \
                 number AS track_number, stated_length_ms, position FROM laid_places)",
        layout_ctes()
    )
}

/// The sides picked for CUE sheets that the layout gave the sheets: each a
/// sheet's file id with the side.
pub fn honored_picks_on(
    sql: &impl coven::SqlRead,
    layout: &LayoutParams<'_>,
) -> Result<Vec<(String, bae_types::mediums::MediumSide)>, DbError> {
    Ok(sql.query(
        &format!("WITH {} SELECT sheet_file_id, medium, side FROM honored", layout_ctes()),
        layout.named().as_slice(),
        |row| {
            Ok((
                row.get(0)?,
                bae_types::mediums::MediumSide { medium: row.get(1)?, side: row.get(2)? },
            ))
        },
    )?)
}

/// The sides and discs of the layout a CUE sheet can be put on, by
/// [`super::sheet_side_choices_on`].
pub fn laid_sheet_side_choices_on(
    sql: &impl coven::SqlRead,
    layout: &LayoutParams<'_>,
) -> Result<Vec<DbSheetSideChoice>, DbError> {
    super::sheet_side_choices_on(sql, &laid_mediums_source(), layout.named().as_slice())
}

/// The sides a sheet can be restored to while editing, including a medium
/// that would become empty if the editor saved the form.
pub fn base_sheet_side_choices_on(
    sql: &impl coven::SqlRead,
    layout: &LayoutParams<'_>,
) -> Result<Vec<DbSheetSideChoice>, DbError> {
    super::sheet_side_choices_on(sql, &mediums_source("base_mediums"), layout.named().as_slice())
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

#[cfg(test)]
#[path = "layout_tests.rs"]
mod tests;
