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
/// audio file each plays whole. The statements of
/// [`sheet_files_ctes`] and of the layout read it as one JSON object, and
/// work out from it which tracks the save deletes and adds.
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

/// The SQL expression for the edits of a statement that makes none.
pub const NO_SHEET_EDITS: &str = "'{}'";

/// The common table expressions that work out what the edits `edits` do to
/// the tracks of the release `release` (SQL expressions; `edits` a JSON
/// object of [`SheetEdits`], or [`NO_SHEET_EDITS`]):
///
/// - `picked_lines` (`sheet_file_id`, `file_reference`, `audio_file_id`): the
///   edits' picks, a line that plays no audio with a NULL `audio_file_id`;
/// - `ignored_sheets` (`sheet_file_id`): the sheets the edits ignore;
/// - `line_files` (`sheet_file_id`, `file_reference`, `file_id`): each line
///   of the release's sheets with the file its tracks' bodies play from now;
/// - `moved_lines` (`sheet_file_id`, `file_reference`, `from_file_id`,
///   `to_file_id`): the lines picked onto a file they do not play from now;
/// - `dropped_tracks` (`track_id`, `sheet_file_id`, `position`): each track
///   the save deletes, under an ignored sheet or a line that plays no audio;
/// - `kept_segments` (the columns of `laid_segments`): each segment of the
///   tracks that stay, on the file it then reads;
/// - `added_files` (`file_id`, `sheet_file_id`, `first_position`): each file
///   that deleted tracks played and no track that stays plays once the picks
///   are written, which the save adds a track for, with the sheet and the
///   place in the release of the first deleted track that played it.
pub fn sheet_changes_ctes(release: &str, edits: &str) -> String {
    format!(
        "picked_lines (sheet_file_id, file_reference, audio_file_id) AS ( \
             SELECT j.value ->> 'sheet_file_id', j.value ->> 'file_reference', \
                    j.value ->> 'audio_file_id' \
             FROM json_each(json_extract({edits}, '$.picks')) AS j), \
         ignored_sheets (sheet_file_id) AS ( \
             SELECT value FROM json_each(json_extract({edits}, '$.ignored'))), \
         active_picks AS ( \
             SELECT * FROM picked_lines \
             WHERE sheet_file_id NOT IN (SELECT sheet_file_id FROM ignored_sheets)), \
         line_files (sheet_file_id, file_reference, file_id) AS ( \
             SELECT DISTINCT c.sheet_file_id, c.file_reference, s.file_id \
             FROM carved_tracks c JOIN tracks t ON t.id = c.track_id \
             JOIN audio_formats af ON af.track_id = c.track_id \
             JOIN audio_format_segments s ON s.audio_format_id = af.id AND s.role = 'main' \
             WHERE t.release_id = {release}), \
         moved_lines (sheet_file_id, file_reference, from_file_id, to_file_id) AS ( \
             SELECT line.sheet_file_id, line.file_reference, line.file_id, pick.audio_file_id \
             FROM line_files line JOIN active_picks pick \
                 ON pick.sheet_file_id = line.sheet_file_id \
                AND pick.file_reference = line.file_reference \
             WHERE pick.audio_file_id <> line.file_id), \
         dropped_tracks (track_id, sheet_file_id, position) AS ( \
             SELECT c.track_id, c.sheet_file_id, t.position \
             FROM carved_tracks c JOIN tracks t ON t.id = c.track_id \
             LEFT JOIN picked_lines pick ON pick.sheet_file_id = c.sheet_file_id \
                                        AND pick.file_reference = c.file_reference \
             WHERE t.release_id = {release} \
               AND (c.sheet_file_id IN (SELECT sheet_file_id FROM ignored_sheets) \
                    OR (pick.sheet_file_id IS NOT NULL AND pick.audio_file_id IS NULL))), \
         kept_segments (id, track_id, segment_index, role, file_id, start_sample, \
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
             WHERE t.release_id = {release} \
               AND t.id NOT IN (SELECT track_id FROM dropped_tracks)), \
         added_files (file_id, sheet_file_id, first_position) AS ( \
             SELECT s.file_id, dropped.sheet_file_id, MIN(dropped.position) \
             FROM dropped_tracks dropped \
             JOIN audio_formats af ON af.track_id = dropped.track_id \
             JOIN audio_format_segments s ON s.audio_format_id = af.id AND s.role = 'main' \
             WHERE NOT EXISTS ( \
                 SELECT 1 FROM tracks stays \
                 JOIN audio_formats stays_format ON stays_format.track_id = stays.id \
                 JOIN audio_format_segments played \
                     ON played.audio_format_id = stays_format.id AND played.role = 'main' \
                 LEFT JOIN carved_tracks carved ON carved.track_id = stays.id \
                 LEFT JOIN picked_lines pick ON pick.sheet_file_id = carved.sheet_file_id \
                                            AND pick.file_reference = carved.file_reference \
                 WHERE stays.release_id = {release} \
                   AND stays.id NOT IN (SELECT track_id FROM dropped_tracks) \
                   AND COALESCE(pick.audio_file_id, played.file_id) = s.file_id) \
             GROUP BY s.file_id)"
    )
}

/// The common table expressions that lay out the files of the release
/// `release` for the edits `edits` (SQL expressions as for
/// [`sheet_changes_ctes`], whose expressions come first):
///
/// - `laid_lines` (the columns of `line_files`, and `stored_file_id`): each
///   line with the file it plays from once picked, and the one it plays from
///   now;
/// - `laid_segments` (the columns of `kept_segments`, and `added`): each
///   segment of the release's tracks on the file it then reads, a segment of
///   a sheet's track on a moved line's file moving to the picked one, a
///   pregap at the tail of that file among them; a deleted track has none,
///   and a track the edits add has one, whole file, marked `added`;
/// - `laid_files`: the release's files, with the columns of
///   `release_files`, each picked file laid out as a CUE image, each file a
///   track is added for laid out as a whole-file track and each file a line
///   moved off that no segment then reads as one no track plays from.
pub fn sheet_files_ctes(release: &str, edits: &str) -> String {
    let columns = read::FILE_ROW_COLUMNS
        .iter()
        .filter(|column| **column != "source_audio_layout")
        .map(|column| format!("f.{column}"))
        .collect::<Vec<_>>()
        .join(", ");
    format!(
        "{changes}, \
         laid_lines (sheet_file_id, file_reference, file_id, stored_file_id) AS ( \
             SELECT line.sheet_file_id, line.file_reference, \
                    CASE WHEN pick.sheet_file_id IS NULL THEN line.file_id \
                         ELSE pick.audio_file_id END, line.file_id \
             FROM line_files line LEFT JOIN picked_lines pick \
                 ON pick.sheet_file_id = line.sheet_file_id \
                AND pick.file_reference = line.file_reference), \
         laid_segments (id, track_id, segment_index, role, file_id, start_sample, \
                        end_sample, sample_rate, moved, added) AS ( \
             SELECT *, 0 FROM kept_segments \
             UNION ALL \
             SELECT file_id, file_id, 0, 'main', file_id, 0, NULL, 1, 0, 1 FROM added_files), \
         laid_files AS ( \
             SELECT {columns}, \
                    CASE WHEN played.added THEN 'file' \
                         WHEN f.id IN (SELECT to_file_id FROM moved_lines) THEN 'cue' \
                         WHEN f.id IN (SELECT from_file_id FROM moved_lines) \
                              AND played.file_id IS NULL THEN NULL \
                         ELSE f.source_audio_layout END AS source_audio_layout \
             FROM release_files f \
             LEFT JOIN (SELECT file_id, MAX(added) AS added FROM laid_segments \
                        GROUP BY file_id) played ON played.file_id = f.id \
             WHERE f.release_id = {release})",
        changes = sheet_changes_ctes(release, edits),
    )
}

/// The caption's bound count and bound and unbound lines, with each
/// line's options. A sheet with no unbound lines has a NULL unbound list.
/// The query reads the stored tracks so a line keeps its options even when
/// the draft drops the tracks it carves.
pub fn sheet_audio_options_ctes(release: &str) -> String {
    format!(
        "line_ends (sheet_file_id, file_reference, end_ms, position) AS ( \
             SELECT c.sheet_file_id, c.file_reference, \
                    MAX(COALESCE(s.end_sample, s.start_sample) * 1000 / af.sample_rate), \
                    MIN(t.position) \
             FROM carved_tracks c JOIN tracks t ON t.id = c.track_id \
             JOIN audio_formats af ON af.track_id = c.track_id \
             JOIN audio_format_segments s ON s.audio_format_id = af.id AND s.role = 'main' \
             WHERE t.release_id = {release} \
             GROUP BY c.sheet_file_id, c.file_reference), \
         line_audio_options (sheet_file_id, file_reference, file_id, file_name, options, position) AS ( \
             SELECT line.sheet_file_id, line.file_reference, \
                    line.file_id, MAX(CASE WHEN f.id = line.file_id THEN f.original_filename END), \
                    json_group_array(json_object( \
                        'file_id', f.id, 'name', f.original_filename, \
                        'content_type', f.source_audio_content_type, \
                        'bound', json(CASE WHEN f.id = line.file_id THEN 'true' ELSE 'false' END), \
                        'fits', json(CASE WHEN ends.end_ms <= f.source_audio_duration_ms \
                                          THEN 'true' \
                                          WHEN ends.end_ms > f.source_audio_duration_ms \
                                          THEN 'false' END)) \
                        ORDER BY f.original_filename COLLATE natural_order, f.id), ends.position \
             FROM laid_lines line \
             JOIN line_ends ends ON ends.sheet_file_id = line.sheet_file_id \
                                AND ends.file_reference = line.file_reference \
             JOIN laid_files f ON f.role = 'audio' \
             LEFT JOIN coven_file_reads reads ON reads.file = f.file \
             WHERE f.id = line.file_id OR f.id = line.stored_file_id \
                OR (f.source_audio_layout IS NULL AND reads.source IS NOT NULL) \
             GROUP BY line.sheet_file_id, line.file_reference), \
         sheet_line_options (sheet_file_id, bound_count, bound, unbound) AS ( \
             SELECT o.sheet_file_id, COUNT(o.file_id), \
                    json_group_array(json_object( \
                        'line', {line}, 'file_id', o.file_id, 'file_name', o.file_name) \
                        ORDER BY o.position) FILTER (WHERE o.file_id IS NOT NULL), \
                    CASE WHEN COUNT(*) > COUNT(o.file_id) THEN \
                        json_group_array({line} ORDER BY o.position) \
                        FILTER (WHERE o.file_id IS NULL) END \
             FROM line_audio_options o GROUP BY o.sheet_file_id)",
        line = "json_object('file_reference', o.file_reference, 'options', json(o.options))",
    )
}

/// The names of the files the picks of `edits` move lines of the release
/// `release`'s sheets onto (SQL expressions over `params`, as for
/// [`sheet_files_ctes`]), which a save reads before it writes, in name
/// order; `None` when the picks move no line.
pub fn sheet_files_to_read_on(
    sql: &impl coven::SqlRead,
    release: &str,
    edits: &str,
    params: impl rusqlite::Params,
) -> Result<Option<Vec<String>>, DbError> {
    let names: Option<String> = sql.query_row(
        &format!(
            "WITH {} SELECT CASE WHEN EXISTS (SELECT 1 FROM moved_lines) THEN ( \
                 SELECT json_group_array(name ORDER BY name COLLATE natural_order) \
                 FROM (SELECT DISTINCT f.original_filename AS name FROM moved_lines moved \
                       JOIN release_files f ON f.id = moved.to_file_id)) END",
            sheet_files_ctes(release, edits),
        ),
        params,
        |row| row.get(0),
    )?;
    names
        .map(|names| {
            serde_json::from_str(&names).map_err(|error| {
                DbError::unreadable_json("the files a save reads first", names.clone(), error)
            })
        })
        .transpose()
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

/// What the edits `edits` (the JSON object of a [`SheetEdits`]) do to the
/// tracks of the release `release_id`.
pub fn sheet_track_changes_on(
    sql: &impl coven::SqlRead,
    release_id: &str,
    edits: &str,
) -> Result<SheetTrackChanges, DbError> {
    let params = rusqlite::named_params! { ":release": release_id, ":edits": edits };
    let changes = sheet_changes_ctes(":release", ":edits");
    let dropped = sql.query(
        &format!("WITH {changes} SELECT track_id FROM dropped_tracks ORDER BY position"),
        params,
        |row| row.get(0),
    )?;
    let bindings = sql.query(
        &format!(
            "WITH {changes} SELECT sheet_file_id, file_reference, audio_file_id FROM active_picks"
        ),
        params,
        |row| {
            Ok(SheetFilePick {
                sheet_file_id: row.get(0)?,
                file_reference: row.get(1)?,
                audio_file_id: row.get(2)?,
            })
        },
    )?;
    let added = sql.query(
        &format!(
            "WITH {changes} SELECT added.file_id, f.original_filename, \
                    (SELECT j.value FROM json_each(json_extract(:edits, '$.titles')) AS j \
                     WHERE j.key = added.file_id) \
             FROM added_files added JOIN release_files f ON f.id = added.file_id \
             ORDER BY added.first_position"
        ),
        params,
        |row| Ok(AddedFile { file_id: row.get(0)?, name: row.get(1)?, typed_title: row.get(2)? }),
    )?;
    let medium_id = sql.query_row(
        "SELECT id FROM release_mediums WHERE release_id = ? ORDER BY position LIMIT 1",
        [release_id],
        |row| row.get(0),
    )?;
    Ok(SheetTrackChanges { dropped, bindings, added, medium_id })
}

impl Records {
    /// What the edits `edits` do to the tracks of the release `release_id`.
    pub async fn get_sheet_track_changes(
        &self,
        release_id: &str,
        edits: &SheetEdits,
    ) -> Result<SheetTrackChanges, DbError> {
        let (release_id, edits) = (release_id.to_string(), edits.json()?);
        self.store.read(move |sql| sheet_track_changes_on(&sql, &release_id, &edits)).await
    }
}

/// Each sheet's place when the editor opens.
#[cfg(desktop)]
pub fn opened_sheet_places_on(
    sql: &impl coven::SqlRead,
    release_id: &str,
) -> Result<std::collections::BTreeMap<String, bae_types::mediums::MediumSide>, DbError> {
    let places = super::release_tracks::track_places_ctes(
        super::MediumsSource::Release,
        "m.release_id = ?1",
    );
    let first = first_sheet_places_sql();
    Ok(sql
        .query(
            &format!(
                "WITH {places} SELECT sheet_file_id, medium_position, side FROM ({first}) WHERE n = 1"
            ),
            [release_id],
            |row| {
                Ok((
                    row.get(0)?,
                    bae_types::mediums::MediumSide { medium: row.get(1)?, side: row.get(2)? },
                ))
            },
        )?
        .into_iter()
        .collect())
}

#[cfg(desktop)]
fn first_sheet_places_sql() -> String {
    let sided = super::release::SIDED;
    let side = super::release_tracks::sheet_side_sql(sided, "place.side");
    let kind = super::release_tracks::sheet_side_kind_sql(sided);
    let ordinal = super::release_tracks::sheet_side_ordinal_sql(
        sided,
        "place.side_ordinal",
        "place.medium_position",
    );
    format!(
        "SELECT c.sheet_file_id, place.medium_position, {side} AS side, \
                {kind} AS kind, {ordinal} AS ordinal, \
                ROW_NUMBER() OVER (PARTITION BY c.sheet_file_id ORDER BY place.position) AS n \
         FROM carved_tracks c JOIN place ON place.id = c.track_id"
    )
}

/// The Files list's caption for a sheet whose tracks the edit deletes,
/// with its bindings and the side it was on, if the form still places it.
#[cfg(desktop)]
pub fn sheet_without_tracks_json_sql(file: &str) -> String {
    let first = first_sheet_places_sql();
    format!(
        "(SELECT json_object( \
                'file_id', first.sheet_file_id, 'name', sheet.original_filename, \
                'place', json(CASE WHEN ignored.sheet_file_id IS NULL THEN json_object( \
                    'medium', first.medium_position, 'side', first.side, \
                    'kind', first.kind, 'ordinal', first.ordinal) END), \
                'bound_count', options.bound_count, \
                'bound', json(options.bound), 'unbound', json(options.unbound)) \
         FROM release_files sheet \
         JOIN sheet_line_options options ON options.sheet_file_id = sheet.id \
         LEFT JOIN ignored_sheets ignored ON ignored.sheet_file_id = sheet.id \
         JOIN ({first}) first \
             ON first.sheet_file_id = sheet.id AND first.n = 1 \
         WHERE sheet.id = {file} \
           AND NOT EXISTS (SELECT 1 FROM carved_tracks c \
                           WHERE c.sheet_file_id = sheet.id \
                             AND c.track_id NOT IN (SELECT track_id FROM dropped_tracks)))"
    )
}

/// The editor's Files caption for a sheet that carves no tracks in the draft.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DbSheetWithoutTracks {
    pub file_id: String,
    pub name: String,
    /// The original side for a sheet with unbound lines, or none when ignored.
    pub place: Option<DbSheetSideChoice>,
    pub bound_count: u32,
    pub bound: Vec<DbBoundLine>,
    pub unbound: Option<Vec<DbUnboundLine>>,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Deserialize)]
pub struct DbBoundLine {
    pub line: DbUnboundLine,
    pub file_id: String,
    pub file_name: String,
}

/// A [`DbSheetWithoutTracks`] with the columns of its side as
/// [`DbSheetSideChoice::of_columns`] reads them.
#[derive(Debug, Clone, serde::Deserialize)]
pub(super) struct SheetWithoutTracksColumns {
    file_id: String,
    name: String,
    place: Option<crate::DbSheetSideColumns>,
    bound_count: u32,
    bound: Vec<DbBoundLine>,
    unbound: Option<Vec<DbUnboundLine>>,
}

impl SheetWithoutTracksColumns {
    pub(super) fn decoded(self) -> Result<DbSheetWithoutTracks, String> {
        let place = self.place.map(crate::DbSheetSideColumns::decoded).transpose()?;
        Ok(DbSheetWithoutTracks {
            place,
            file_id: self.file_id,
            name: self.name,
            bound_count: self.bound_count,
            bound: self.bound,
            unbound: self.unbound,
        })
    }
}

/// The available audio files for one `FILE` line.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct DbUnboundLine {
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
    let edits = rebound.edits.json()?;
    tx.execute(
        &format!(
            "WITH {} UPDATE release_files SET source_audio_layout = \
                 (SELECT laid.source_audio_layout FROM laid_files laid \
                  WHERE laid.id = release_files.id) \
             WHERE release_id = :release",
            sheet_files_ctes(":release", ":edits")
        ),
        rusqlite::named_params! { ":release": release_id, ":edits": edits },
    )?;
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
