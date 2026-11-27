//! Which audio file each `FILE` line of a release's CUE sheets plays from
//! once a release editor's form binds lines to other audio files of the
//! release, and what a save then writes over the old file's rows.
//!
//! [`LaidFiles`](super::sheet_fragments::LaidFiles) and the fragments beside
//! it lay the files out for the editor's draft, which shows the tracks, the
//! Files list and the sheet's Audio file pop-up from them, and for the save,
//! which writes each file's layout from them.

use super::*;
use bae_store::{statement_row, statement_rows};
#[cfg(desktop)]
use sqlive_compose::Statement as _;

/// The audio file the person picked for one `FILE` line of a CUE sheet in a
/// release editor: the sheet's file id, the line's text and the audio file's
/// id, a file of the release, or `None` for a line that plays no audio.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct SheetFilePick {
    pub sheet_file_id: String,
    pub file_reference: String,
    pub audio_file_id: Option<String>,
}

/// What a release editor's form chose about the release's CUE sheets and the
/// tracks a save adds: the audio files picked for `FILE` lines, the ignored sheets, and the
/// titles the person typed for the tracks a save adds, by the id of the
/// audio file each plays whole. The
/// [sheet fragments](super::sheet_fragments) and the layout read it as one
/// JSON object, and work out from it which tracks the save deletes and
/// adds.
#[derive(Debug, Clone, Default, PartialEq, Eq, serde::Serialize)]
pub struct SheetEdits {
    pub picks: Vec<SheetFilePick>,
    pub ignored: Vec<String>,
    pub titles: std::collections::BTreeMap<String, String>,
}

impl SheetEdits {
    /// The edits as the JSON object the statements bind.
    pub fn json(&self) -> Result<String, DbError> {
        serde_json::to_string(self)
            .map_err(|error| DbError::encode("the edits of a release's CUE sheets", error))
    }

    /// Whether the edits leave every sheet's place and binding alone.
    pub fn is_empty(&self) -> bool {
        self.picks.is_empty() && self.ignored.is_empty()
    }
}

/// The edits of a read that makes none.
pub const NO_SHEET_EDITS: &str = "{}";

// The names of the files the picks of `edits` move lines of the release
// `release`'s sheets onto, each once, in name order; no row when the picks
// move no line.
sqlive_compose::read!(
    SheetFilesToRead(release: String, edits: String),
    r#"
    WITH moved AS (
        SELECT lines.*
        FROM @crate::records::sheet_fragments::MovedSheetLines(
            release: :release, edits: :edits) AS lines)
    SELECT DISTINCT f.original_filename AS name
    FROM moved
    JOIN release_files AS f ON f.id = moved.to_file_id
    ORDER BY f.original_filename COLLATE natural_order
"#
);

/// The names of the files the picks of `edits` move lines of the release
/// `release_id`'s sheets onto, which a save reads before it writes, in name
/// order; `None` when the picks move no line.
pub fn sheet_files_to_read_on(
    sql: &impl coven::SqlRead,
    release_id: &str,
    edits: &str,
) -> Result<Option<Vec<String>>, DbError> {
    let params =
        SheetFilesToReadParams { release: release_id.to_string(), edits: edits.to_string() };
    let rows = statement_rows(sql, &params)?;
    Ok((!rows.is_empty()).then(|| rows.into_iter().map(|row| row.name).collect()))
}

/// An audio file a save adds a track for: the file's id and name, and the
/// title the person typed for the track, when they typed one.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AddedFile {
    pub file_id: String,
    pub name: String,
    pub typed_title: Option<String>,
}

/// The tracks a save of the edits `edits` deletes, in release order, and the
/// files it adds a track for, in the order their first deleted track sat,
/// with the medium, the release's first, that an added track sits on until
/// the layout the save writes places it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SheetTrackChanges {
    pub dropped: Vec<String>,
    /// The bindings a save writes for sheets the form still places.
    pub bindings: Vec<SheetFilePick>,
    pub added: Vec<AddedFile>,
    pub medium_id: String,
}

// The tracks a save of the edits `edits` deletes from the release
// `release`, in release order.
sqlive_compose::read!(
    DroppedTracksInOrder(release: String, edits: String),
    r#"
    SELECT dropped.*
    FROM @crate::records::sheet_fragments::DroppedSheetTracks(
        release: :release, edits: :edits) AS dropped
    ORDER BY dropped.position
"#
);

// The files a save of the edits `edits` adds a track for, each with its name,
// in the order their first deleted track sat.
sqlive_compose::read!(
    AddedFilesInOrder(release: String, edits: String),
    r#"
    WITH added AS (
        SELECT fresh.*
        FROM @crate::records::sheet_fragments::SheetAddedFiles(
            release: :release, edits: :edits) AS fresh)
    SELECT added.file_id AS file_id, f.original_filename AS name
    FROM added
    JOIN release_files AS f ON f.id = added.file_id
    ORDER BY added.first_position
"#
);

// The release's first medium.
sqlive_compose::read!(
    FirstMedium(release: String),
    r#"
    SELECT held.id
    FROM release_mediums AS held
    WHERE held.release_id = :release
    ORDER BY held.position
    LIMIT 1
"#
);

/// What the edits `edits` do to the tracks of the release `release_id`.
pub fn sheet_track_changes_on(
    sql: &impl coven::SqlRead,
    release_id: &str,
    edits: &SheetEdits,
) -> Result<SheetTrackChanges, DbError> {
    let release = release_id.to_string();
    let json = edits.json()?;
    let dropped = DroppedTracksInOrderParams { release: release.clone(), edits: json.clone() };
    let dropped =
        statement_rows(sql, &dropped)?.into_iter().map(|row| row.dropped.into_track_id()).collect();
    let bindings = edits
        .picks
        .iter()
        .filter(|pick| !edits.ignored.contains(&pick.sheet_file_id))
        .cloned()
        .collect();
    let added = AddedFilesInOrderParams { release: release.clone(), edits: json };
    let added = statement_rows(sql, &added)?
        .into_iter()
        .map(|row| AddedFile {
            typed_title: edits.titles.get(&row.file_id).cloned(),
            file_id: row.file_id,
            name: row.name,
        })
        .collect();
    let medium_id = statement_row(sql, &FirstMediumParams { release })?.id;
    Ok(SheetTrackChanges { dropped, bindings, added, medium_id })
}

impl Records {
    /// What the edits `edits` do to the tracks of the release `release_id`.
    pub async fn get_sheet_track_changes(
        &self,
        release_id: &str,
        edits: &SheetEdits,
    ) -> Result<SheetTrackChanges, DbError> {
        let (release_id, edits) = (release_id.to_string(), edits.clone());
        self.store.read(move |sql| sheet_track_changes_on(&sql, &release_id, &edits)).await
    }
}

/// The editor's Files caption for a sheet that carves no tracks in the draft.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DbSheetWithoutTracks {
    pub file_id: String,
    pub name: String,
    /// The original side for a sheet with unbound lines, or none when ignored.
    pub place: Option<DbSheetSideChoice>,
    pub side_choices: Vec<DbSheetSideChoice>,
    pub bound_count: u32,
    /// The lines that play audio; each line's `bound` option is the file.
    pub bound: Vec<DbSheetLine>,
    pub unbound: Option<Vec<DbSheetLine>>,
}

/// A [`DbSheetWithoutTracks`] with the columns of its side as
/// [`DbSheetSideChoice::of_columns`] reads them.
#[derive(Debug, Clone, serde::Deserialize)]
pub(super) struct SheetWithoutTracksColumns {
    file_id: String,
    name: String,
    place: Option<crate::DbSheetSideColumns>,
    side_choices: Vec<crate::DbSheetSideColumns>,
    bound_count: u32,
    bound: Vec<DbSheetLine>,
    unbound: Option<Vec<DbSheetLine>>,
}

impl SheetWithoutTracksColumns {
    pub(super) fn decoded(self) -> Result<DbSheetWithoutTracks, String> {
        let place = self.place.map(crate::DbSheetSideColumns::decoded).transpose()?;
        Ok(DbSheetWithoutTracks {
            place,
            side_choices: crate::DbSheetSideColumns::decoded_list(self.side_choices)?,
            file_id: self.file_id,
            name: self.name,
            bound_count: self.bound_count,
            bound: self.bound,
            unbound: self.unbound,
        })
    }
}

/// A `FILE` line of a CUE sheet with the audio files it can play from.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct DbSheetLine {
    pub file_reference: String,
    pub options: Vec<DbSheetAudioOption>,
}

/// A release's tracks' audio as a save that binds its sheets' lines to other
/// files reads it: each track's audio format, every segment of those
/// formats, and the sheet and line of each track a sheet carves.
#[derive(Debug, Clone, PartialEq)]
pub struct DbReleaseAudio {
    pub formats: Vec<DbAudioFormat>,
    pub segments: Vec<DbAudioSegment>,
    pub carved: Vec<DbCarvedTrack>,
}

/// A track a CUE sheet carves: the sheet's file id, and the text of the
/// sheet's `FILE` line the track is under.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DbCarvedTrack {
    pub track_id: String,
    pub sheet_file_id: String,
    pub file_reference: String,
}

impl Records {
    /// The audio of the release `release_id`'s tracks, from one read.
    pub async fn get_release_audio(&self, release_id: &str) -> Result<DbReleaseAudio, DbError> {
        let release_id = release_id.to_string();
        self.store
            .read(move |sql| {
                let formats = sql.query(
                    "SELECT af.* FROM audio_formats af JOIN tracks t ON t.id = af.track_id \
                     WHERE t.release_id = ? ORDER BY t.position",
                    params![release_id],
                    row_to_audio_format,
                )?;
                let segments = sql.query(
                    "SELECT s.* FROM audio_format_segments s \
                     JOIN audio_formats af ON af.id = s.audio_format_id \
                     JOIN tracks t ON t.id = af.track_id \
                     WHERE t.release_id = ? ORDER BY t.position, s.segment_index",
                    params![release_id],
                    row_to_audio_segment,
                )?;
                let carved = sql.query(
                    "SELECT c.track_id, c.sheet_file_id, c.file_reference FROM carved_tracks c \
                     JOIN tracks t ON t.id = c.track_id WHERE t.release_id = ? \
                     ORDER BY t.position",
                    params![release_id],
                    |row| {
                        Ok(DbCarvedTrack {
                            track_id: row.get(0)?,
                            sheet_file_id: row.get(1)?,
                            file_reference: row.get(2)?,
                        })
                    },
                )?;
                Ok(DbReleaseAudio { formats, segments, carved })
            })
            .await
    }
}

/// A track a save adds, under the key of its file: the track's row, which
/// sits on the release's first medium, side 1 and number 1 until the layout
/// the save writes sets its place.
#[derive(Debug, Clone, PartialEq)]
pub struct NewTrack {
    pub file_id: String,
    pub track: DbTrack,
}

/// What a save writes over the tracks of the release's CUE sheets once the
/// form's edits are applied, read from the files the tracks play and
/// measured: each moved track's new length, the rows of the tracks the save
/// adds, the ids of the tracks it deletes, the audio format with its
/// loudness of each moved and each added track, every segment of those
/// formats with their scrub points, and the release's album loudness and
/// peak, measured over all its tracks.
#[derive(Debug, Clone, PartialEq)]
pub struct ReboundAudio {
    pub edits: SheetEdits,
    /// The edited bindings the save writes for sheets that still have a place.
    pub bindings: Vec<SheetFilePick>,
    pub dropped: Vec<String>,
    /// Each moved track's id and length.
    pub track_lengths: Vec<(String, u64)>,
    /// The tracks the save adds, each with its row.
    pub added: Vec<NewTrack>,
    pub formats: Vec<DbAudioFormat>,
    pub segments: Vec<DbAudioSegment>,
    pub scrub_points: Vec<DbScrubPoints>,
    pub album_loudness_lufs: Option<f64>,
    pub album_peak_linear: Option<f64>,
}

// Give each of the release's files the layout the edits `edits` lay it out
// with.
#[cfg(desktop)]
sqlive_compose::write!(
    LayOutSheetFiles(release: String, edits: String),
    r#"
    UPDATE release_files
    SET source_audio_layout = (
        SELECT laid.source_audio_layout
        FROM @crate::records::sheet_fragments::LaidFiles(
            release: :release, edits: :edits) AS laid
        WHERE laid.id = release_files.id)
    WHERE release_id = :release
"#
);

/// Write `rebound` over the release `release_id`: each file's layout as the
/// edits lay it out, then each moved track's length and audio rows in place
/// of the ones it had, so no row of the file it played from is left, the
/// deletion of the tracks the edits drop, the rows of the tracks they add,
/// and the album's loudness. The new tracks sit on the release's first
/// medium until the layout the save writes places them.
#[cfg(desktop)]
pub(super) fn write_rebound_audio_on(
    tx: &CovenWrite<'_>,
    release_id: &str,
    rebound: &ReboundAudio,
) -> Result<(), DbError> {
    let layouts =
        LayOutSheetFilesParams { release: release_id.to_string(), edits: rebound.edits.json()? };
    tx.execute(LayOutSheetFiles::SQL, layouts.params().as_slice())?;
    for (track_id, length_ms) in &rebound.track_lengths {
        tx.execute("DELETE FROM audio_formats WHERE track_id = ?", params![track_id])?;
        tx.execute(
            "UPDATE tracks SET duration_ms = ? WHERE id = ? AND release_id = ?",
            params![to_i64(*length_ms, "duration_ms")?, track_id, release_id],
        )?;
    }
    let dropped = serde_json::to_string(&rebound.dropped)
        .map_err(|error| DbError::encode("the tracks a release editor save deletes", error))?;
    tx.execute(
        "DELETE FROM tracks WHERE release_id = ? AND id IN (SELECT value FROM json_each(?))",
        params![release_id, dropped],
    )?;
    for added in &rebound.added {
        insert_track_row(tx, &added.track)?;
    }
    for format in &rebound.formats {
        insert_audio_format_row(tx, format)?;
    }
    for segment in &rebound.segments {
        insert_audio_segment_row(tx, segment)?;
    }
    for points in &rebound.scrub_points {
        insert_scrub_points_row(tx, points)?;
    }
    tx.execute(
        "UPDATE releases SET album_loudness_lufs = ?, album_peak_linear = ? WHERE id = ?",
        params![rebound.album_loudness_lufs, rebound.album_peak_linear, release_id],
    )?;
    Ok(())
}

#[cfg(test)]
#[path = "sheet_files_tests.rs"]
mod tests;
