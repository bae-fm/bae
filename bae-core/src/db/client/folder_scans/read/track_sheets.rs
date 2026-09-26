//! The track sheets a scan stored: each sheet's header, its tracks and their
//! indexes, and the audio each bound sheet describes.

use super::*;

type AudioFilesBySheet = HashMap<SheetKey, Vec<SheetAudioFile>>;
type TracksBySheet = HashMap<SheetKey, Vec<CueTrack>>;
type IndexesByTrack = HashMap<TrackKey, Vec<CueIndex>>;

/// One parsed sheet: the candidate it sits under, and its path within it.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub(super) struct SheetKey {
    pub(super) candidate_path: String,
    pub(super) sheet_relative_path: String,
}

/// The audio each bound sheet describes, in the sheet's reference order.
pub(super) fn load_sheet_audio_files(
    sql: &(impl QueryOne + QueryRows),
    watched_folder_path: &str,
    only: Option<&str>,
) -> Result<impl FnOnce() -> Result<AudioFilesBySheet, DbError> + Send + 'static, DbError> {
    let rows = sql.query(
        "SELECT candidate_path, sheet_relative_path, file_reference, audio_relative_path \
         FROM scan_sheet_audio_file \
         WHERE watched_folder_path = :root AND (:only IS NULL OR candidate_path = :only) \
         ORDER BY candidate_path, sheet_relative_path, position",
        named_params! { ":root": watched_folder_path, ":only": only },
        |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, String>(3)?,
            ))
        },
    )?;
    Ok(move || {
        let mut audio_files: AudioFilesBySheet = HashMap::new();
        for (candidate_path, sheet_relative_path, file_reference, audio_relative_path) in rows {
            audio_files
                .entry(SheetKey {
                    candidate_path,
                    sheet_relative_path,
                })
                .or_default()
                .push(SheetAudioFile {
                    file_reference,
                    file_id: audio_relative_path,
                });
        }
        Ok(audio_files)
    })
}

/// One track of one sheet, by its position in that sheet.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct TrackKey {
    sheet: SheetKey,
    position: i64,
}

pub(super) fn load_cue_sheets(
    sql: &(impl QueryOne + QueryRows),
    watched_folder_path: &str,
    only: Option<&str>,
) -> Result<impl FnOnce() -> Result<HashMap<SheetKey, CueSheet>, DbError> + Send + 'static, DbError>
{
    let rows = sql.query(
        "SELECT candidate_path, sheet_relative_path, title, performer, catalog, date, ripper \
         FROM scan_cue_sheet \
         WHERE watched_folder_path = :root AND (:only IS NULL OR candidate_path = :only)",
        named_params! { ":root": watched_folder_path, ":only": only },
        |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                CueSheet {
                    title: row.get(2)?,
                    performer: row.get(3)?,
                    catalog: row.get(4)?,
                    date: row.get(5)?,
                    ripper: row
                        .get::<_, Option<String>>(6)?
                        .map(|key| {
                            crate::cue_flac::CdRipper::from_key(&key).ok_or_else(|| {
                                super::super::super::read::column_conversion_error(
                                    row,
                                    "ripper",
                                    format!("ripper {key:?}"),
                                )
                            })
                        })
                        .transpose()?,
                    tracks: Vec::new(),
                },
            ))
        },
    )?;
    let tracks = load_cue_tracks(sql, watched_folder_path, only)?;
    Ok(move || {
        let mut tracks = tracks()?;
        let mut sheets = HashMap::with_capacity(rows.len());
        for (candidate_path, sheet_relative_path, mut sheet) in rows {
            let key = SheetKey {
                candidate_path,
                sheet_relative_path,
            };
            sheet.tracks = tracks.remove(&key).unwrap_or_default();
            sheets.insert(key, sheet);
        }
        Ok(sheets)
    })
}

struct CueTrackRow {
    candidate_path: String,
    sheet_relative_path: String,
    position: i64,
    number: i64,
    mode: String,
    mode_other: Option<String>,
    title: Option<String>,
    performer: Option<String>,
    file_reference: String,
    start_cue_frames: i64,
    end_cue_frames: Option<i64>,
    pregap_kind: String,
    pregap_frames: Option<i64>,
    pregap_index_number: Option<i64>,
    pregap_index_file_reference: Option<String>,
}

fn load_cue_tracks(
    sql: &(impl QueryOne + QueryRows),
    watched_folder_path: &str,
    only: Option<&str>,
) -> Result<impl FnOnce() -> Result<TracksBySheet, DbError> + Send + 'static, DbError> {
    let rows = sql.query(
        "SELECT candidate_path, sheet_relative_path, position, number, mode, mode_other, \
                title, performer, file_reference, start_cue_frames, end_cue_frames, \
                pregap_kind, pregap_frames, pregap_index_number, pregap_index_file_reference \
         FROM scan_cue_track \
         WHERE watched_folder_path = :root AND (:only IS NULL OR candidate_path = :only) \
         ORDER BY candidate_path, sheet_relative_path, position",
        named_params! { ":root": watched_folder_path, ":only": only },
        |row| {
            Ok(CueTrackRow {
                candidate_path: row.get(0)?,
                sheet_relative_path: row.get(1)?,
                position: row.get(2)?,
                number: row.get(3)?,
                mode: row.get(4)?,
                mode_other: row.get(5)?,
                title: row.get(6)?,
                performer: row.get(7)?,
                file_reference: row.get(8)?,
                start_cue_frames: row.get(9)?,
                end_cue_frames: row.get(10)?,
                pregap_kind: row.get(11)?,
                pregap_frames: row.get(12)?,
                pregap_index_number: row.get(13)?,
                pregap_index_file_reference: row.get(14)?,
            })
        },
    )?;
    let indexes = load_cue_indexes(sql, watched_folder_path, only)?;
    Ok(move || {
        let mut indexes = indexes()?;
        let mut tracks: TracksBySheet = HashMap::new();
        for row in rows {
            let pregap = match row.pregap_kind.as_str() {
                "none" => CuePregap::None,
                "silence" => CuePregap::Silence {
                    frames: to_u64(
                        row.pregap_frames.ok_or_else(|| {
                            DbError::Message("a silent pregap states no length".to_string())
                        })?,
                        "a generated pregap's length",
                    )?,
                },
                "audio" => {
                    let missing =
                        |what: &str| DbError::Message(format!("an audio pregap states no {what}"));
                    CuePregap::Audio(CueIndex {
                        number: to_u32(
                            row.pregap_index_number.ok_or_else(|| missing("index"))?,
                            "a pregap's index number",
                        )?,
                        frames: to_u64(
                            row.pregap_frames.ok_or_else(|| missing("position"))?,
                            "a pregap's frame position",
                        )?,
                        file_reference: row
                            .pregap_index_file_reference
                            .ok_or_else(|| missing("file"))?,
                    })
                }
                other => return Err(unreadable("pregap_kind", other)),
            };
            let sheet = SheetKey {
                candidate_path: row.candidate_path,
                sheet_relative_path: row.sheet_relative_path,
            };
            let indexes = indexes
                .remove(&TrackKey {
                    sheet: sheet.clone(),
                    position: row.position,
                })
                .unwrap_or_default();
            tracks.entry(sheet).or_default().push(CueTrack {
                number: to_u32(row.number, "a cue track's number")?,
                mode: match row.mode.as_str() {
                    "audio" => CueTrackMode::Audio,
                    "other" => CueTrackMode::Other(row.mode_other.ok_or_else(|| {
                        DbError::Message("a non-audio cue track names no mode".to_string())
                    })?),
                    other => return Err(unreadable("mode", other)),
                },
                title: row.title,
                performer: row.performer,
                indexes,
                file_reference: row.file_reference,
                start_cue_frames: to_u64(row.start_cue_frames, "a cue track's start")?,
                pregap,
                end_cue_frames: row
                    .end_cue_frames
                    .map(|frames| to_u64(frames, "a cue track's end"))
                    .transpose()?,
            });
        }
        Ok(tracks)
    })
}

fn load_cue_indexes(
    sql: &(impl QueryOne + QueryRows),
    watched_folder_path: &str,
    only: Option<&str>,
) -> Result<impl FnOnce() -> Result<IndexesByTrack, DbError> + Send + 'static, DbError> {
    let rows = sql.query(
        "SELECT candidate_path, sheet_relative_path, track_position, number, frames, \
                file_reference \
         FROM scan_cue_index \
         WHERE watched_folder_path = :root AND (:only IS NULL OR candidate_path = :only) \
         ORDER BY candidate_path, sheet_relative_path, track_position, position",
        named_params! { ":root": watched_folder_path, ":only": only },
        |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, i64>(2)?,
                row.get::<_, i64>(3)?,
                row.get::<_, i64>(4)?,
                row.get::<_, String>(5)?,
            ))
        },
    )?;
    Ok(move || {
        let mut indexes: IndexesByTrack = HashMap::new();
        for (candidate_path, sheet_relative_path, track_position, number, frames, file_reference) in
            rows
        {
            indexes
                .entry(TrackKey {
                    sheet: SheetKey {
                        candidate_path,
                        sheet_relative_path,
                    },
                    position: track_position,
                })
                .or_default()
                .push(CueIndex {
                    number: to_u32(number, "a cue index's number")?,
                    frames: to_u64(frames, "a cue index's frame position")?,
                    file_reference,
                });
        }
        Ok(indexes)
    })
}

// ── Boundaries ──────────────────────────────────────────────────────────────
