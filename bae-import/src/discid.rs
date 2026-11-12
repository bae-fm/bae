//! The MusicBrainz disc ID of a CD: its tracks' sector offsets and lead-out,
//! read off a rip log's table of contents, or laid out by a track sheet over
//! the measured lengths of the audio it names.

mod hash;

use crate::cue_sheet::CueSheet;
use std::collections::HashMap;
use tracing::{debug, trace, warn};

const CD_PREGAP_SECTORS: i32 = 150;

fn invalid_discid_data(message: impl Into<String>) -> std::io::Error {
    std::io::Error::new(std::io::ErrorKind::InvalidData, message.into())
}

/// Whether a line has the columns of a TOC table row: track, start, length,
/// start sector and end sector.
fn has_toc_row_columns(line: &str) -> bool {
    line.split('|').count() >= 5
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
/// Every row of the table must read and the track numbers must run 1, 2, 3 in
/// order: a row skipped would hash a disc one track short, a wrong DiscID that
/// looks as valid as the right one.
/// Format: "       10  | 37:42.72 |  4:14.43 |    169722    |   188814"
fn extract_log_toc_sectors(log_content: &str) -> Result<Vec<(i32, i32)>, std::io::Error> {
    trace!("Parsing LOG file TOC");
    let mut in_toc_section = false;
    let mut track_sectors = Vec::new();
    for line in log_content.lines() {
        let line = line.trim();
        let line_lower = line.to_ascii_lowercase();
        let toc_row = parse_log_toc_row(line);

        if line_lower.contains("toc")
            && (line_lower.contains("cd") || line_lower.contains("extracted"))
        {
            in_toc_section = true;
            trace!("Found TOC section header: {}", line);
            continue;
        }
        if !in_toc_section && toc_row.is_some() {
            in_toc_section = true;
            trace!("Found TOC table format directly (no header)");
        }
        if in_toc_section
            && (line_lower.contains("range status")
                || line_lower.contains("accuraterip")
                || (line.is_empty() && !track_sectors.is_empty()))
        {
            trace!("End of TOC section, found {} tracks", track_sectors.len());
            break;
        }
        if !in_toc_section {
            continue;
        }
        if line.contains("---")
            || line.is_empty()
            || (line_lower.contains("track")
                && (line_lower.contains("start") || line_lower.contains("sector")))
        {
            continue;
        }
        if !has_toc_row_columns(line) {
            continue;
        }
        let Some((track_number, start_sector, end_sector)) = toc_row else {
            return Err(invalid_discid_data(format!("LOG TOC row does not read: {line}")));
        };
        let expected = track_sectors.len() + 1;
        if usize::try_from(track_number) != Ok(expected) {
            return Err(invalid_discid_data(format!(
                "LOG TOC lists track {track_number} where track {expected} belongs"
            )));
        }
        track_sectors.push((start_sector, end_sector));
        trace!("  Track {} sectors: start={}, end={}", track_number, start_sector, end_sector);
    }
    if track_sectors.is_empty() {
        warn!("Could not find any TOC rows in LOG file");
        let toc_start = log_content.lines().position(|l| {
            let l_lower = l.to_ascii_lowercase();
            l_lower.contains("toc") && (l_lower.contains("cd") || l_lower.contains("extracted"))
        });
        let preview: String = if let Some(start_idx) = toc_start {
            log_content.lines().skip(start_idx).take(15).collect::<Vec<_>>().join("\n")
        } else {
            log_content.lines().take(30).collect::<Vec<_>>().join("\n")
        };
        debug!("LOG content preview (TOC section):\n{}", preview);
        return Err(invalid_discid_data("No TOC rows found in LOG file"));
    }

    Ok(track_sectors)
}

fn discid_from_raw_offsets(
    method_label: &str,
    raw_track_sectors: &[i32],
    raw_leadout_sector: i32,
    leadout_source: &str,
) -> Result<String, std::io::Error> {
    let track_offsets: Vec<i32> = raw_track_sectors
        .iter()
        .map(|sector| {
            sector.checked_add(CD_PREGAP_SECTORS).ok_or_else(|| {
                invalid_discid_data("track start sector out of range for DiscID calculation")
            })
        })
        .collect::<Result<_, _>>()?;
    let lead_out_sectors = raw_leadout_sector.checked_add(CD_PREGAP_SECTORS).ok_or_else(|| {
        invalid_discid_data("lead-out sector out of range for DiscID calculation")
    })?;
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

    let mb_discid_str = hash::musicbrainz_discid(&offsets)
        .map_err(|e| invalid_discid_data(format!("Failed to calculate DiscID: {e}")))?;
    debug!("MusicBrainz DiscID calculated: {}", mb_discid_str);
    Ok(mb_discid_str)
}

/// MusicBrainz DiscID from a rip log's text alone — the most direct method,
/// since the sector offsets are in the log and neither the CUE nor the audio
/// is needed.
pub(crate) fn discid_from_log_text(log_content: &str) -> Result<String, std::io::Error> {
    trace!("LOG file decoded, length: {} chars", log_content.len());
    let toc_sectors = extract_log_toc_sectors(log_content)?;
    let raw_track_sectors: Vec<i32> = toc_sectors.iter().map(|(start, _)| *start).collect();
    let last_end_sector = toc_sectors.last().expect("LOG TOC parser returned at least one row").1;
    let raw_leadout_sector = last_end_sector
        .checked_add(1)
        .ok_or_else(|| invalid_discid_data("lead-out sector out of range in LOG TOC"))?;
    debug!("Found {} track(s) in LOG file", raw_track_sectors.len());
    discid_from_raw_offsets("LOG", &raw_track_sectors, raw_leadout_sector, "from LOG lead-out")
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
) -> Result<String, std::io::Error> {
    if sheet.playable_tracks().next().is_none() {
        return Err(invalid_discid_data("CUE has no playable audio tracks"));
    }
    // Where each file begins on the disc.
    let mut file_start: HashMap<&str, u64> = HashMap::new();
    let mut laid = 0u64;
    for file_reference in sheet.audio_file_references() {
        let duration = audio
            .iter()
            .find(|duration| duration.file_reference == file_reference)
            .ok_or_else(|| {
                invalid_discid_data(format!("CUE FILE {file_reference:?} has no measured length"))
            })?;
        file_start.insert(file_reference, laid);
        laid += sectors_of(duration.duration_ms);
    }
    let sector = |frames: u64| {
        i32::try_from(frames).map_err(|error| {
            invalid_discid_data(format!(
                "CUE lays the disc out past the sector range ({frames} frames): {error}"
            ))
        })
    };
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
