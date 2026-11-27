//! The MusicBrainz disc ID of a CD: its tracks' sector offsets and lead-out,
//! read off a rip log's table of contents, or laid out by a track sheet over
//! the measured lengths of the audio it names.

mod hash;

use crate::cue_sheet::CueSheet;
use std::collections::HashMap;
use tracing::{debug, trace, warn};

const CD_PREGAP_SECTORS: i32 = 150;

/// Why a disc a log or sheet describes can't be hashed to its Disc ID.
#[derive(Debug, thiserror::Error)]
pub enum DiscIdError {
    #[error("LOG TOC row does not read: {row}")]
    RowDoesNotRead { row: String },
    #[error("LOG TOC lists track {found} where track {expected} belongs: {row}")]
    TrackOutOfPlace { found: u32, expected: usize, row: String },
    #[error("CUE has no playable audio tracks")]
    NoPlayableTracks,
    #[error("CUE FILE {file_reference:?} has no measured length")]
    NoMeasuredLength { file_reference: String },
    #[error("a track or the lead-out is past the sector range")]
    PastSectorRange,
    #[error("Failed to calculate DiscID: {0}")]
    Hash(String),
}

impl DiscIdError {
    /// The reason with no row of a log and no file name.
    pub(crate) fn reported(&self) -> String {
        match self {
            Self::RowDoesNotRead { .. } => "a table of contents row does not read".to_string(),
            Self::TrackOutOfPlace { found, expected, .. } => {
                format!("the table of contents lists track {found} where track {expected} belongs")
            }
            Self::NoMeasuredLength { .. } => {
                "a file the sheet names has no measured length".to_string()
            }
            Self::NoPlayableTracks | Self::PastSectorRange | Self::Hash(_) => self.to_string(),
        }
    }
}

/// Whether a line is a track row of a TOC table: its first cell, before a
/// `|`, is a number.
fn is_log_toc_row(line: &str) -> bool {
    line.split_once('|').is_some_and(|(first, _)| {
        let first = first.trim();
        !first.is_empty() && first.bytes().all(|byte| byte.is_ascii_digit())
    })
}

/// A TOC table row's `(track number, start sector, end sector)`; `None` for
/// a line whose cells do not read as one.
fn parse_log_toc_row(line: &str) -> Option<(u32, i32, i32)> {
    let parts: Vec<&str> = line.split('|').collect();
    if parts.len() < 5 {
        return None;
    }

    let track_num = parts[0].trim().parse::<u32>().ok()?;
    if !(1..=99).contains(&track_num) {
        return None;
    }

    let start_sector = parts[3].trim().parse::<i32>().ok()?;
    let end_sector = parts[4].trim().parse::<i32>().ok()?;
    if start_sector < 0 || end_sector <= 0 {
        return None;
    }

    Some((track_num, start_sector, end_sector))
}

/// Extract raw `(start_sector, end_sector)` pairs from an EAC/XLD LOG TOC table.
/// The rippers write the words around the table in the person's language, so no
/// word of the log marks the table: it opens at the first track row (see
/// [`is_log_toc_row`]) and ends at the first blank line after it. Any other
/// line between the two is skipped.
/// Every track row must read and the track numbers must run 1, 2, 3 in order:
/// a row left out would hash a disc one track short, a wrong DiscID that looks
/// as valid as the right one. `None` when the log holds no track row, which is
/// a log that states no disc.
/// Row format: "       10  | 37:42.72 |  4:14.43 |    169722    |   188814"
fn extract_log_toc_sectors(log_content: &str) -> Result<Option<Vec<(i32, i32)>>, DiscIdError> {
    trace!("Parsing LOG file TOC");
    let table = log_content
        .lines()
        .map(str::trim)
        .skip_while(|line| !is_log_toc_row(line))
        .take_while(|line| !line.is_empty());
    let mut track_sectors = Vec::new();
    for line in table.filter(|line| is_log_toc_row(line)) {
        let Some((track_number, start_sector, end_sector)) = parse_log_toc_row(line) else {
            return Err(DiscIdError::RowDoesNotRead { row: line.to_string() });
        };
        let expected = track_sectors.len() + 1;
        if usize::try_from(track_number) != Ok(expected) {
            return Err(DiscIdError::TrackOutOfPlace {
                found: track_number,
                expected,
                row: line.to_string(),
            });
        }
        track_sectors.push((start_sector, end_sector));
        trace!("  Track {} sectors: start={}, end={}", track_number, start_sector, end_sector);
    }
    if track_sectors.is_empty() {
        warn!("Could not find any TOC rows in LOG file");
        debug!(
            "LOG content preview:\n{}",
            log_content.lines().take(30).collect::<Vec<_>>().join("\n")
        );
        return Ok(None);
    }

    Ok(Some(track_sectors))
}

fn discid_from_raw_offsets(
    method_label: &str,
    raw_track_sectors: &[i32],
    raw_leadout_sector: i32,
    leadout_source: &str,
) -> Result<String, DiscIdError> {
    let track_offsets: Vec<i32> = raw_track_sectors
        .iter()
        .map(|sector| sector.checked_add(CD_PREGAP_SECTORS).ok_or(DiscIdError::PastSectorRange))
        .collect::<Result<_, _>>()?;
    let lead_out_sectors =
        raw_leadout_sector.checked_add(CD_PREGAP_SECTORS).ok_or(DiscIdError::PastSectorRange)?;
    debug!("{method_label} raw track start sectors before adding 150: {:?}", raw_track_sectors);
    debug!("{method_label} raw lead-out sector {leadout_source}: {raw_leadout_sector}");
    debug!(
        "{method_label} lead-out offset: {lead_out_sectors} sectors (raw: {raw_leadout_sector} + 150)"
    );

    let mut offsets = Vec::with_capacity(track_offsets.len() + 1);
    offsets.push(lead_out_sectors);
    offsets.extend_from_slice(&track_offsets);

    debug!(
        "{method_label} DiscID offsets: first_track=1, last_track={}, offsets={:?}",
        track_offsets.len(),
        offsets
    );

    let mb_discid_str =
        hash::musicbrainz_discid(&offsets).map_err(|e| DiscIdError::Hash(e.to_string()))?;
    debug!("MusicBrainz DiscID calculated: {}", mb_discid_str);
    Ok(mb_discid_str)
}

/// MusicBrainz DiscID from a rip log's text alone — the most direct method,
/// since the sector offsets are in the log and neither the CUE nor the audio
/// is needed. `None` for a log that holds no table of contents, which states
/// no disc; a table that is there and cannot be hashed is an error.
pub(crate) fn discid_from_log_text(log_content: &str) -> Result<Option<String>, DiscIdError> {
    trace!("LOG file decoded, length: {} chars", log_content.len());
    let Some(toc_sectors) = extract_log_toc_sectors(log_content)? else {
        return Ok(None);
    };
    let raw_track_sectors: Vec<i32> = toc_sectors.iter().map(|(start, _)| *start).collect();
    let last_end_sector = toc_sectors.last().expect("LOG TOC parser returned at least one row").1;
    let raw_leadout_sector = last_end_sector.checked_add(1).ok_or(DiscIdError::PastSectorRange)?;
    debug!("Found {} track(s) in LOG file", raw_track_sectors.len());
    discid_from_raw_offsets("LOG", &raw_track_sectors, raw_leadout_sector, "from LOG lead-out")
        .map(Some)
}
/// The measured length of one audio file a sheet names.
#[derive(Debug, Clone, Copy)]
pub struct SheetAudioDuration<'a> {
    /// The path as the sheet's `FILE` directive spells it.
    pub file_reference: &'a str,
    pub duration_ms: u64,
}

/// A file's length in CD sectors. A rip is a whole number of sectors, so
/// each file rounds on its own: rounding a sum of millisecond lengths would
/// let truncation drift across many files.
fn sectors_of(duration_ms: u64) -> u64 {
    (duration_ms * 75 + 500) / 1000
}

/// The disc a sheet and its audio describe, as MusicBrainz hashes it: every
/// audio track's INDEX 01 as a sector on the disc, and the lead-out after the
/// last, each offset including the 150-sector lead-in.
///
/// The sheet lays the disc out across its files in `FILE` order — one file
/// for the whole disc, or one per track. A track starts where its INDEX 01
/// sits inside its file, after every file before that one, and after the
/// silence every `PREGAP` directive up to and including its own generates:
/// that silence is on the disc and in no file. The lead-out is everything
/// laid end to end.
pub(crate) fn calculate_mb_discid_from_cue(
    sheet: &CueSheet,
    audio: &[SheetAudioDuration<'_>],
    method_label: &str,
) -> Result<String, DiscIdError> {
    if sheet.playable_tracks().next().is_none() {
        return Err(DiscIdError::NoPlayableTracks);
    }
    // Where each file begins on the disc.
    let mut file_start: HashMap<&str, u64> = HashMap::new();
    let mut laid = 0u64;
    for file_reference in sheet.audio_file_references() {
        let duration =
            audio.iter().find(|duration| duration.file_reference == file_reference).ok_or_else(
                || DiscIdError::NoMeasuredLength { file_reference: file_reference.to_string() },
            )?;
        file_start.insert(file_reference, laid);
        laid += sectors_of(duration.duration_ms);
    }
    #[expect(
        clippy::map_err_ignore,
        reason = "a conversion out of range says only that it is past the range"
    )]
    let sector = |frames: u64| i32::try_from(frames).map_err(|_| DiscIdError::PastSectorRange);
    let mut generated = 0u64;
    let mut raw_track_sectors = Vec::with_capacity(sheet.playable_track_count());
    for track in sheet.playable_tracks() {
        generated += track.generated_pregap_frames.unwrap_or(0);
        raw_track_sectors.push(sector(
            file_start[track.file_reference.as_str()] + track.start_cue_frames + generated,
        )?);
    }
    let raw_leadout_sector = sector(laid + generated)?;
    debug!(
        "Found {} track(s) across {} file(s) in CUE file",
        raw_track_sectors.len(),
        file_start.len()
    );
    discid_from_raw_offsets(
        method_label,
        &raw_track_sectors,
        raw_leadout_sector,
        "from the audio laid end to end",
    )
}

#[cfg(test)]
#[path = "discid_tests.rs"]
mod tests;
