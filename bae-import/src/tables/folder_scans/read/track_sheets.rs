//! The track sheets a scan stored, as a candidate file's row reads them: each
//! sheet's header with its tracks and their indexes, and the audio each bound
//! sheet describes.

use super::*;

/// One parsed sheet as [`ScanItems`](crate::tables::folder_scans::fragments::ScanItems)
/// holds it: its header with its tracks in order, each with its indexes in
/// order.
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

/// One audio file a sheet describes as
/// [`ScanItems`](crate::tables::folder_scans::fragments::ScanItems) holds it,
/// in the sheet's reference order.
#[derive(serde::Deserialize)]
pub(super) struct StoredSheetAudio {
    file_reference: String,
    file_id: String,
}

impl StoredSheetAudio {
    pub(super) fn audio_file(self) -> SheetAudioFile {
        SheetAudioFile { file_reference: self.file_reference, file_id: self.file_id }
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
                .map(|stored| crate::tables::folder_scans::columns::ripper_of(&stored))
                .transpose()?,
            tracks: self.tracks.into_iter().map(StoredCueTrack::track).collect::<Result<_, _>>()?,
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
                    DbError::unreadable(
                        "scan_cue_track.mode_other of an other track",
                        None::<String>,
                    )
                })?),
                other => return Err(DbError::unreadable("scan_cue_track.mode", other)),
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

#[cfg(test)]
#[path = "track_sheets_tests.rs"]
mod tests;
