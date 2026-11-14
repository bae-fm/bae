//! A candidate's audio layout, written with its file rows: the audio units in
//! play order with each one's length and preview range, and the audio each
//! track sheet's FILE references can be bound to. The layout is a fact of the
//! files, so the write that stores the files stores it, and every reader
//! selects it.

use super::*;
use crate::folder_scanner::{CategorizedFiles, SheetBindingOffer};
use crate::types::AudioFile;
use crate::CandidateKey;

/// Write the units and sheet offers of `files`, the files of the candidate
/// `candidate_path`, whose file rows are written. A folder whose audio the
/// scan has not read yet has no layout to store.
pub(super) fn insert_units(
    sql: &CovenWrite<'_>,
    watched_folder_path: &Path,
    candidate_path: &CandidateKey,
    files: &CategorizedFiles,
) -> Result<(), DbError> {
    if files.audio().any(|audio| audio.source_audio.is_none()) {
        return Ok(());
    }
    let unreadable = |what: &str, error: crate::ImportError| DbError::unreadable(what, error);
    let durations = crate::probe::source_durations(files)
        .map_err(|error| unreadable("the audio lengths of a candidate's files", error))?;
    let carving = files.carving_sheets();
    for (position, track) in crate::audio_layout::folder_tracks(files).into_iter().enumerate() {
        let duration_ms = durations
            .duration_of(&track.file)
            .ok_or_else(|| DbError::unreadable("the length of an audio unit", &track.file))?;
        let (file_kind, sheet_id, slice_index, cue_position, start, end) = match &track.file {
            AudioFile::Standalone { .. } => ("standalone", None, None, None, None, None),
            AudioFile::SheetSlice { sheet_id, index, .. } => {
                let sheet = carving
                    .iter()
                    .find(|sheet| sheet.file.relative_path == *sheet_id)
                    .ok_or_else(|| DbError::unreadable("the sheet of an audio unit", sheet_id))?;
                let (cue_position, cue_track) = sheet
                    .sheet
                    .tracks
                    .iter()
                    .enumerate()
                    .filter(|(_, track)| track.mode.is_audio())
                    .nth(*index as usize)
                    .ok_or_else(|| DbError::unreadable("the CUE track of an audio unit", index))?;
                let audio = sheet.audio_for(cue_track);
                let rate = u64::try_from(
                    audio
                        .source_audio
                        .as_ref()
                        .expect("a scanned audio file has source facts")
                        .format
                        .sample_rate_hz,
                )
                .map_err(|error| {
                    DbError::unreadable("a sample rate", (&audio.relative_path, error))
                })?;
                (
                    "sheet_slice",
                    Some(sheet_id.as_str()),
                    Some(i64::from(*index)),
                    Some(cue_position as i64),
                    Some(to_i64(
                        crate::cue_sheet::cue_frames_to_samples(cue_track.start_cue_frames, rate),
                        "a slice's start",
                    )?),
                    cue_track
                        .end_cue_frames
                        .map(|frames| {
                            to_i64(
                                crate::cue_sheet::cue_frames_to_samples(frames, rate),
                                "a slice's end",
                            )
                        })
                        .transpose()?,
                )
            }
        };
        sql.execute(
            "INSERT INTO scan_candidate_unit \
                 (watched_folder_path, candidate_path, position, file_kind, file_id, sheet_id, \
                  slice_index, cue_track_position, duration_ms, start_sample, end_sample, \
                  disc, number) \
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
            params![
                coven::path_bytes(watched_folder_path),
                candidate_path,
                position as i64,
                file_kind,
                track.file.file_id(),
                sheet_id,
                slice_index,
                cue_position,
                to_i64(duration_ms, "an audio unit's length")?,
                start,
                end,
                track.disc,
                track.number.get(),
            ],
        )?;
    }
    for sheet in files.track_sheets() {
        for reference in files.sheet_binding_options(&sheet.file.relative_path) {
            for option in reference.options {
                let (offer, codec) = match &option.offer {
                    SheetBindingOffer::Offered => ("offered", None),
                    SheetBindingOffer::RefusedTiming => ("refused_timing", None),
                    SheetBindingOffer::RefusedCodec { codec } => {
                        ("refused_codec", Some(codec.as_str()))
                    }
                    SheetBindingOffer::RefusedUnreadable => ("refused_unreadable", None),
                };
                sql.execute(
                    "INSERT INTO scan_candidate_sheet_offer \
                         (watched_folder_path, candidate_path, sheet_relative_path, \
                          file_reference, audio_relative_path, offer, codec) \
                     VALUES (?, ?, ?, ?, ?, ?, ?)",
                    params![
                        coven::path_bytes(watched_folder_path),
                        candidate_path,
                        sheet.file.relative_path,
                        reference.file_reference,
                        option.file_id,
                        offer,
                        codec,
                    ],
                )?;
            }
        }
    }
    Ok(())
}

/// One stored audio unit, as a test reads it.
#[cfg(test)]
#[derive(Debug, PartialEq, Eq)]
pub(crate) struct StoredUnit {
    pub file_id: String,
    pub sheet_id: Option<String>,
    pub slice_index: Option<u32>,
    pub duration_ms: u64,
    pub start_sample: Option<u64>,
    pub end_sample: Option<u64>,
    pub disc: Option<u32>,
    pub number: u32,
}

#[cfg(test)]
impl crate::tables::ImportTables {
    /// The blank cover stored for the candidates reading `content_hash`'s
    /// files.
    pub(crate) async fn blank_cover_file_for_test(&self, content_hash: &str) -> Option<String> {
        let content_hash = content_hash.to_owned();
        self.store
            .read(move |sql| {
                Ok(sql.query_row(
                    "SELECT blank_cover_file FROM scan_candidate WHERE content_hash = ?",
                    [content_hash],
                    |row| row.get(0),
                )?)
            })
            .await
            .unwrap()
    }

    /// The units stored for the candidates reading `content_hash`'s files, in
    /// play order.
    pub(crate) async fn load_units_for_test(&self, content_hash: &str) -> Vec<StoredUnit> {
        let content_hash = content_hash.to_owned();
        self.store
            .read(move |sql| {
                sql.query(
                    "SELECT u.file_id, u.sheet_id, u.slice_index, u.duration_ms, u.start_sample, \
                            u.end_sample, u.disc, u.number \
                     FROM scan_candidate AS c JOIN scan_candidate_unit AS u \
                         ON (u.watched_folder_path, u.candidate_path) = (c.watched_folder_path, c.path) \
                     WHERE c.content_hash = ? ORDER BY u.position",
                    [content_hash],
                    |row| {
                        Ok(StoredUnit {
                            file_id: row.get(0)?,
                            sheet_id: row.get(1)?,
                            slice_index: row.get(2)?,
                            duration_ms: row.get(3)?,
                            start_sample: row.get(4)?,
                            end_sample: row.get(5)?,
                            disc: row.get(6)?,
                            number: row.get(7)?,
                        })
                    },
                )
                .map_err(Into::into)
            })
            .await
            .unwrap()
    }
}

#[cfg(test)]
#[path = "units_tests.rs"]
mod tests;
