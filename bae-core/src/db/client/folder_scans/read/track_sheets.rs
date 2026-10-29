//! The track sheets a scan stored, as a candidate file's row reads them: each
//! sheet's header with its tracks and their indexes, and the audio each bound
//! sheet describes.

use super::*;

/// The parsed sheet of the candidate file joined as `file`, as a JSON object
/// holding its tracks in order, each with its indexes in order; NULL for a
/// file that is no track sheet.
pub(super) const SHEET_JSON: &str = "(SELECT json_object( \
         'title', sheet.title, 'performer', sheet.performer, 'catalog', sheet.catalog, \
         'date', sheet.date, 'ripper', sheet.ripper, \
         'tracks', json((SELECT json_group_array(json_object( \
             'number', track.number, 'mode', track.mode, 'mode_other', track.mode_other, \
             'title', track.title, 'performer', track.performer, \
             'file_reference', track.file_reference, \
             'start_cue_frames', track.start_cue_frames, \
             'end_cue_frames', track.end_cue_frames, \
             'generated_pregap_frames', track.generated_pregap_frames, \
             'indexes', json((SELECT json_group_array(json_object( \
                 'number', cue_index.number, 'frames', cue_index.frames, \
                 'file_reference', cue_index.file_reference) ORDER BY cue_index.position) \
                 FROM scan_cue_index AS cue_index \
                 WHERE cue_index.watched_folder_path = track.watched_folder_path \
                   AND cue_index.candidate_path = track.candidate_path \
                   AND cue_index.sheet_relative_path = track.sheet_relative_path \
                   AND cue_index.track_position = track.position))) \
             ORDER BY track.position) \
             FROM scan_cue_track AS track \
             WHERE track.watched_folder_path = sheet.watched_folder_path \
               AND track.candidate_path = sheet.candidate_path \
               AND track.sheet_relative_path = sheet.sheet_relative_path))) \
     FROM scan_cue_sheet AS sheet \
     WHERE sheet.watched_folder_path = file.watched_folder_path \
       AND sheet.candidate_path = file.candidate_path \
       AND sheet.sheet_relative_path = file.relative_path)";

/// The audio the sheet `file` describes, as a JSON array in the sheet's
/// reference order.
pub(super) const SHEET_AUDIO_JSON: &str = "(SELECT json_group_array(json_object( \
         'file_reference', described.file_reference, \
         'file_id', described.audio_relative_path) ORDER BY described.position) \
     FROM scan_sheet_audio_file AS described \
     WHERE described.watched_folder_path = file.watched_folder_path \
       AND described.candidate_path = file.candidate_path \
       AND described.sheet_relative_path = file.relative_path)";

/// One parsed sheet as [`SHEET_JSON`] holds it.
#[derive(serde::Deserialize)]
pub(super) struct StoredSheet {
    title: Option<String>,
    performer: Option<String>,
    catalog: Option<String>,
    date: Option<String>,
    ripper: Option<String>,
    tracks: Vec<StoredCueTrack>,
}

#[derive(serde::Deserialize)]
struct StoredCueTrack {
    number: i64,
    mode: String,
    mode_other: Option<String>,
    title: Option<String>,
    performer: Option<String>,
    file_reference: String,
    start_cue_frames: i64,
    end_cue_frames: Option<i64>,
    generated_pregap_frames: Option<i64>,
    indexes: Vec<StoredCueIndex>,
}

#[derive(serde::Deserialize)]
struct StoredCueIndex {
    number: i64,
    frames: i64,
    file_reference: String,
}

/// One audio file a sheet describes as [`SHEET_AUDIO_JSON`] holds it.
#[derive(serde::Deserialize)]
pub(super) struct StoredSheetAudio {
    file_reference: String,
    file_id: String,
}

impl StoredSheetAudio {
    pub(super) fn audio_file(self) -> SheetAudioFile {
        SheetAudioFile {
            file_reference: self.file_reference,
            file_id: self.file_id,
        }
    }
}

impl StoredSheet {
    pub(super) fn sheet(self) -> Result<CueSheet, DbError> {
        Ok(CueSheet {
            title: self.title,
            performer: self.performer,
            catalog: self.catalog,
            date: self.date,
            ripper: self
                .ripper
                .map(|key| {
                    crate::cue_flac::CdRipper::from_key(&key)
                        .ok_or_else(|| unreadable("ripper", &key))
                })
                .transpose()?,
            tracks: self
                .tracks
                .into_iter()
                .map(StoredCueTrack::track)
                .collect::<Result<_, _>>()?,
        })
    }
}

impl StoredCueTrack {
    fn track(self) -> Result<CueTrack, DbError> {
        Ok(CueTrack {
            number: to_u32(self.number, "a cue track's number")?,
            mode: match self.mode.as_str() {
                "audio" => CueTrackMode::Audio,
                "other" => CueTrackMode::Other(self.mode_other.ok_or_else(|| {
                    DbError::Message("a non-audio cue track names no mode".to_string())
                })?),
                other => return Err(unreadable("mode", other)),
            },
            title: self.title,
            performer: self.performer,
            indexes: self
                .indexes
                .into_iter()
                .map(|index| {
                    Ok(CueIndex {
                        number: to_u32(index.number, "a cue index's number")?,
                        frames: to_u64(index.frames, "a cue index's frame position")?,
                        file_reference: index.file_reference,
                    })
                })
                .collect::<Result<_, DbError>>()?,
            file_reference: self.file_reference,
            start_cue_frames: to_u64(self.start_cue_frames, "a cue track's start")?,
            generated_pregap_frames: self
                .generated_pregap_frames
                .map(|frames| to_u64(frames, "a generated pregap's length"))
                .transpose()?,
            end_cue_frames: self
                .end_cue_frames
                .map(|frames| to_u64(frames, "a cue track's end"))
                .transpose()?,
        })
    }
}
