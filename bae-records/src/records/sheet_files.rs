//! Which audio file each `FILE` line of a release's CUE sheets plays from
//! once a release editor's form binds lines to other audio files of the
//! release, and what a save then writes over the old file's rows.
//!
//! One statement lays the files out ([`sheet_files_ctes`]), so the editor's
//! draft, which shows the tracks, the sheet's Audio file pop-up and the
//! release's Files list from it, and the save, which writes each file's
//! layout from it, never disagree.

use super::*;

/// The audio file the person picked for one `FILE` line of a CUE sheet in a
/// release editor: the sheet's file id, the line's text and the audio file's
/// id, all files of the release.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct SheetFilePick {
    pub sheet_file_id: String,
    pub file_reference: String,
    pub audio_file_id: String,
}

/// `picks` as the JSON array the statements of [`sheet_files_ctes`] bind.
pub fn sheet_file_picks_json(picks: &[SheetFilePick]) -> Result<String, DbError> {
    serde_json::to_string(picks)
        .map_err(|error| DbError::encode("the audio files picked for CUE sheets", error))
}

/// The common table expressions that lay out the files of the release
/// `release` for the picks `picks` (SQL expressions; `picks` a JSON array of
/// [`SheetFilePick`]):
///
/// - `line_files` (`sheet_file_id`, `file_reference`, `file_id`): each line
///   of the release's sheets with the file its tracks' bodies play from now;
/// - `laid_lines` (the same, and `stored_file_id`): each line with the file
///   it plays from once picked, and the one it plays from now;
/// - `moved_lines` (`sheet_file_id`, `file_reference`, `from_file_id`,
///   `to_file_id`): the lines picked onto a file they do not play from now;
/// - `laid_segments` (`id`, `track_id`, `segment_index`, `role`, `file_id`,
///   `start_sample`, `end_sample`, `sample_rate`, `moved`): each segment of the release's
///   tracks on the file it then reads, a segment of a sheet's track on a
///   moved line's file moving to the picked one, a pregap at the tail of
///   that file among them;
/// - `laid_files`: the release's files, with the columns of
///   `release_files`, each picked file laid out as a CUE image and each file
///   a line moved off that no segment then reads as one no track plays from.
pub fn sheet_files_ctes(release: &str, picks: &str) -> String {
    let columns = read::FILE_ROW_COLUMNS
        .iter()
        .filter(|column| **column != "source_audio_layout")
        .map(|column| format!("f.{column}"))
        .collect::<Vec<_>>()
        .join(", ");
    format!(
        "picked_lines (sheet_file_id, file_reference, audio_file_id) AS ( \
             SELECT j.value ->> 'sheet_file_id', j.value ->> 'file_reference', \
                    j.value ->> 'audio_file_id' \
             FROM json_each({picks}) AS j), \
         line_files (sheet_file_id, file_reference, file_id) AS ( \
             SELECT DISTINCT c.sheet_file_id, c.file_reference, s.file_id \
             FROM carved_tracks c JOIN tracks t ON t.id = c.track_id \
             JOIN audio_formats af ON af.track_id = c.track_id \
             JOIN audio_format_segments s ON s.audio_format_id = af.id AND s.role = 'main' \
             WHERE t.release_id = {release}), \
         moved_lines (sheet_file_id, file_reference, from_file_id, to_file_id) AS ( \
             SELECT line.sheet_file_id, line.file_reference, line.file_id, pick.audio_file_id \
             FROM line_files line JOIN picked_lines pick \
                 ON pick.sheet_file_id = line.sheet_file_id \
                AND pick.file_reference = line.file_reference \
             WHERE pick.audio_file_id <> line.file_id), \
         laid_lines (sheet_file_id, file_reference, file_id, stored_file_id) AS ( \
             SELECT line.sheet_file_id, line.file_reference, \
                    COALESCE(moved.to_file_id, line.file_id), line.file_id \
             FROM line_files line LEFT JOIN moved_lines moved \
                 ON moved.sheet_file_id = line.sheet_file_id \
                AND moved.file_reference = line.file_reference), \
         laid_segments (id, track_id, segment_index, role, file_id, start_sample, \
                        end_sample, sample_rate, moved) AS ( \
             SELECT s.id, af.track_id, s.segment_index, s.role, \
                    COALESCE(moved.to_file_id, s.file_id), \
                    s.start_sample, s.end_sample, af.sample_rate, moved.to_file_id IS NOT NULL \
             FROM audio_format_segments s JOIN audio_formats af ON af.id = s.audio_format_id \
             JOIN tracks t ON t.id = af.track_id \
             LEFT JOIN carved_tracks c ON c.track_id = af.track_id \
             LEFT JOIN (SELECT sheet_file_id, from_file_id, MIN(to_file_id) AS to_file_id \
                        FROM moved_lines GROUP BY sheet_file_id, from_file_id) moved \
                 ON moved.sheet_file_id = c.sheet_file_id AND moved.from_file_id = s.file_id \
             WHERE t.release_id = {release}), \
         laid_files AS ( \
             SELECT {columns}, \
                    CASE WHEN f.id IN (SELECT to_file_id FROM moved_lines) THEN 'cue' \
                         WHEN f.id IN (SELECT from_file_id FROM moved_lines) \
                              AND NOT EXISTS (SELECT 1 FROM laid_segments laid \
                                              WHERE laid.file_id = f.id) THEN NULL \
                         ELSE f.source_audio_layout END AS source_audio_layout \
             FROM release_files f WHERE f.release_id = {release})"
    )
}

/// The common table expression `line_audio_options` (`sheet_file_id`,
/// `file_reference`, `options`), which follows [`sheet_files_ctes`] in a
/// `WITH`: the audio files each `FILE` line of the release's sheets can play
/// from once the picks are written, as a JSON array of
/// [`DbSheetAudioOption`]: the file it then plays from, the one it plays from
/// now, and every audio file of the release on this device that no track
/// then plays from, in file name order.
pub fn sheet_audio_options_ctes() -> &'static str {
    "line_ends (sheet_file_id, file_reference, end_ms) AS ( \
         SELECT c.sheet_file_id, c.file_reference, \
                MAX(COALESCE(s.end_sample, s.start_sample) * 1000 / s.sample_rate) \
         FROM carved_tracks c JOIN laid_segments s \
             ON s.track_id = c.track_id AND s.role = 'main' \
         GROUP BY c.sheet_file_id, c.file_reference), \
     line_audio_options (sheet_file_id, file_reference, options) AS ( \
         SELECT line.sheet_file_id, line.file_reference, \
                json_group_array(json_object( \
                    'file_id', f.id, 'name', f.original_filename, \
                    'content_type', f.source_audio_content_type, \
                    'bound', json(CASE WHEN f.id = line.file_id THEN 'true' ELSE 'false' END), \
                    'fits', json(CASE WHEN ends.end_ms <= f.source_audio_duration_ms \
                                      THEN 'true' \
                                      WHEN ends.end_ms > f.source_audio_duration_ms \
                                      THEN 'false' END)) \
                    ORDER BY f.original_filename COLLATE natural_order, f.id) \
         FROM laid_lines line \
         JOIN line_ends ends ON ends.sheet_file_id = line.sheet_file_id \
                            AND ends.file_reference = line.file_reference \
         JOIN laid_files f ON f.role = 'audio' \
         LEFT JOIN coven_file_reads reads ON reads.file = f.file \
         WHERE f.id = line.file_id OR f.id = line.stored_file_id \
            OR (f.source_audio_layout IS NULL AND reads.source = 'here') \
         GROUP BY line.sheet_file_id, line.file_reference)"
}

/// The names of the files the picks `picks` move lines of the release
/// `release`'s sheets onto (SQL expressions over `params`, as for
/// [`sheet_files_ctes`]), which a save reads before it writes, in name
/// order.
pub fn sheet_files_to_read_on(
    sql: &impl coven::SqlRead,
    release: &str,
    picks: &str,
    params: impl rusqlite::Params,
) -> Result<Vec<String>, DbError> {
    Ok(sql.query(
        &format!(
            "WITH {} SELECT DISTINCT f.original_filename FROM moved_lines moved \
             JOIN release_files f ON f.id = moved.to_file_id \
             ORDER BY f.original_filename COLLATE natural_order",
            sheet_files_ctes(release, picks),
        ),
        params,
        |row| row.get(0),
    )?)
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

/// What a save writes over the tracks the picks move to other files, read
/// from those files and measured: each moved track's new length, its audio
/// format with its loudness, every segment of those formats with their scrub
/// points, and the release's album loudness and peak, measured over all its
/// tracks.
#[derive(Debug, Clone, PartialEq)]
pub struct ReboundAudio {
    pub picks: Vec<SheetFilePick>,
    /// Each moved track's id and length.
    pub track_lengths: Vec<(String, u64)>,
    pub formats: Vec<DbAudioFormat>,
    pub segments: Vec<DbAudioSegment>,
    pub scrub_points: Vec<DbScrubPoints>,
    pub album_loudness_lufs: Option<f64>,
    pub album_peak_linear: Option<f64>,
}

/// Write `rebound` over the release `release_id`: each file's layout as the
/// picks lay it out, then each moved track's length and audio rows in place
/// of the ones it had, so no row of the file it played from is left, and the
/// album's loudness.
#[cfg(desktop)]
pub(super) fn write_rebound_audio_on(
    tx: &CovenWrite<'_>,
    release_id: &str,
    rebound: &ReboundAudio,
) -> Result<(), DbError> {
    let picks = sheet_file_picks_json(&rebound.picks)?;
    tx.execute(
        &format!(
            "WITH {} UPDATE release_files SET source_audio_layout = \
                 (SELECT laid.source_audio_layout FROM laid_files laid \
                  WHERE laid.id = release_files.id) \
             WHERE release_id = :release",
            sheet_files_ctes(":release", ":picks")
        ),
        rusqlite::named_params! { ":release": release_id, ":picks": picks },
    )?;
    for (track_id, length_ms) in &rebound.track_lengths {
        tx.execute("DELETE FROM audio_formats WHERE track_id = ?", params![track_id])?;
        tx.execute(
            "UPDATE tracks SET duration_ms = ? WHERE id = ? AND release_id = ?",
            params![to_i64(*length_ms, "duration_ms")?, track_id, release_id],
        )?;
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
