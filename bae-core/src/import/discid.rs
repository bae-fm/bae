use crate::cue_flac::CueSheet;
use crate::util::content_type_hint::ContentTypeHint;
use std::path::{Path, PathBuf};
use thiserror::Error;
use tracing::{debug, info, trace, warn};
#[derive(Debug, Error)]
pub enum MetadataDetectionError {
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
}
/// Find a matching audio file for a CUE file (for CUE-pair DiscID calculation).
/// Accepts any extension `ContentTypeHint::is_audio()` recognises so the
/// downstream dispatcher can route by container, not by hardcoded list.
/// Only returns a match for single-file CUE sheets (one FILE directive scoping
/// every TRACK). Multi-FILE sheets are one-file-per-track releases the disc-ID
/// path can't compute — the sectors come from one concatenated container.
pub(crate) fn find_matching_audio_for_cue<'a>(
    cue_path: &Path,
    sheet: &CueSheet,
    audio_files: &'a [PathBuf],
) -> Option<&'a PathBuf> {
    let Some(file_reference) = sheet.single_file() else {
        debug!(
            "CUE is multi-FILE (one file per track) — not a disc-ID candidate: {:?}",
            cue_path
        );
        return None;
    };
    let file_stem = match Path::new(file_reference)
        .file_stem()
        .and_then(|s| s.to_str())
    {
        Some(s) => s,
        None => {
            warn!("CUE FILE reference has no UTF-8 stem: {:?}", file_reference);
            return None;
        }
    };
    debug!(
        "CUE references file with stem: '{}', looking for match",
        file_stem
    );
    audio_files.iter().find(|p| {
        ContentTypeHint::path_is_audio(p)
            && p.file_stem().and_then(|s| s.to_str()) == Some(file_stem)
    })
}
/// Get FLAC file duration in seconds using libFLAC
fn get_flac_duration_seconds(flac_path: &Path) -> Result<f64, MetadataDetectionError> {
    use crate::cue_flac::CueFlacProcessor;
    let flac_info = CueFlacProcessor::analyze_flac(flac_path).map_err(|e| {
        MetadataDetectionError::Io(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            format!("Failed to read FLAC metadata: {}", e),
        ))
    })?;
    let duration_seconds = flac_info.duration_ms() as f64 / 1000.0;
    Ok(duration_seconds)
}
/// Extract lead-out sector from EAC/XLD log file
/// Looks for the "End sector" column in the TOC table
/// Format: "       10  | 37:42.72 |  4:14.43 |    169722    |   188814"
/// The 5th column (index 4) contains the end sector for each track
/// Returns (final offset with 150 added, raw sector without 150)
fn extract_leadout_from_log(log_content: &str) -> Option<(i32, i32)> {
    trace!("Parsing LOG file to extract lead-out sector");
    let mut in_toc_section = false;
    let mut last_end_sector = None;
    let mut track_count = 0;
    for line in log_content.lines() {
        let line = line.trim();
        let line_lower = line.to_ascii_lowercase();
        if line_lower.contains("toc")
            && (line_lower.contains("cd") || line_lower.contains("extracted"))
        {
            in_toc_section = true;
            trace!("Found TOC section header: {}", line);
            continue;
        }
        if !in_toc_section && line.contains('|') {
            let parts: Vec<&str> = line.split('|').collect();
            if parts.len() >= 5 {
                let first_col = parts[0].trim();
                if let Ok(track_num) = first_col.parse::<u32>() {
                    if (1..=99).contains(&track_num) {
                        let end_sector_str = parts[4].trim();
                        if end_sector_str.parse::<i32>().is_ok() {
                            in_toc_section = true;
                            trace!("Found TOC table format directly (no header)");
                        }
                    }
                }
            }
        }
        if in_toc_section
            && (line_lower.contains("range status")
                || line_lower.contains("accuraterip")
                || (line.is_empty() && track_count > 0 && last_end_sector.is_some()))
        {
            trace!("End of TOC section, found {} tracks", track_count);
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
        if line.contains('|') {
            let parts: Vec<&str> = line.split('|').collect();
            if parts.len() >= 5 {
                let end_sector_str = parts[4].trim();
                if let Ok(sector) = end_sector_str.parse::<i32>() {
                    if sector > 0 {
                        track_count += 1;
                        last_end_sector = Some(sector);
                        trace!("  Track {} end sector: {}", track_count, sector);
                    }
                }
            }
        }
    }
    if let Some(sector) = last_end_sector {
        let lead_out_start = sector + 1;
        let lead_out = lead_out_start + 150;
        info!(
            "Extracted lead-out from LOG: {} sectors (last track end: {}, lead-out start: {}, tracks found: {})",
            lead_out, sector, lead_out_start, track_count
        );
        Some((lead_out, lead_out_start))
    } else {
        warn!("Could not find any end sectors in LOG file");
        let toc_start = log_content.lines().position(|l| {
            let l_lower = l.to_ascii_lowercase();
            l_lower.contains("toc") && (l_lower.contains("cd") || l_lower.contains("extracted"))
        });
        let preview: String = if let Some(start_idx) = toc_start {
            log_content
                .lines()
                .skip(start_idx)
                .take(15)
                .collect::<Vec<_>>()
                .join("\n")
        } else {
            log_content.lines().take(30).collect::<Vec<_>>().join("\n")
        };
        debug!("LOG content preview (TOC section):\n{}", preview);
        None
    }
}
/// Extract track offsets from EAC/XLD log file
/// Looks for the "Start sector" column in the TOC table
/// Format: "       10  | 37:42.72 |  4:14.43 |    169722    |   188814"
/// The 4th column (index 3) contains the start sector for each track
/// Returns (final offsets with 150 added, raw sectors without 150)
fn extract_track_offsets_from_log(
    log_content: &str,
) -> Result<(Vec<i32>, Vec<i32>), MetadataDetectionError> {
    trace!("Parsing LOG file to extract track offsets");
    let mut in_toc_section = false;
    let mut track_offsets = Vec::new();
    let mut raw_sectors = Vec::new();
    for line in log_content.lines() {
        let line = line.trim();
        let line_lower = line.to_ascii_lowercase();
        if line_lower.contains("toc")
            && (line_lower.contains("cd") || line_lower.contains("extracted"))
        {
            in_toc_section = true;
            trace!("Found TOC section header: {}", line);
            continue;
        }
        if !in_toc_section && line.contains('|') {
            let parts: Vec<&str> = line.split('|').collect();
            if parts.len() >= 5 {
                let first_col = parts[0].trim();
                if let Ok(track_num) = first_col.parse::<u32>() {
                    if (1..=99).contains(&track_num) {
                        let start_sector_str = parts[3].trim();
                        if start_sector_str.parse::<i32>().is_ok() {
                            in_toc_section = true;
                            trace!("Found TOC table format directly (no header)");
                        }
                    }
                }
            }
        }
        if in_toc_section
            && (line_lower.contains("range status")
                || line_lower.contains("accuraterip")
                || (line.is_empty() && !track_offsets.is_empty()))
        {
            trace!("End of TOC section, found {} tracks", track_offsets.len());
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
        if line.contains('|') {
            let parts: Vec<&str> = line.split('|').collect();
            if parts.len() >= 5 {
                let start_sector_str = parts[3].trim();
                if let Ok(sector) = start_sector_str.parse::<i32>() {
                    if sector >= 0 {
                        raw_sectors.push(sector);
                        let offset = sector + 150;
                        track_offsets.push(offset);
                        debug!(
                            "  Track {} start sector: {} (offset: {})",
                            track_offsets.len(),
                            sector,
                            offset
                        );
                    }
                }
            }
        }
    }
    if track_offsets.is_empty() {
        return Err(MetadataDetectionError::Io(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            "No track offsets found in LOG file",
        )));
    }
    info!("Extracted {} track offset(s) from LOG", track_offsets.len());
    Ok((track_offsets, raw_sectors))
}
/// Calculate MusicBrainz DiscID from LOG file alone
/// This is the most efficient method as it doesn't require CUE or audio files
pub fn calculate_mb_discid_from_log(log_path: &Path) -> Result<String, MetadataDetectionError> {
    info!("Calculating MusicBrainz DiscID from LOG: {:?}", log_path);
    info!("Reading LOG file: {:?}", log_path);
    let log_content = crate::text_encoding::read_text_file(log_path)?.text;

    info!("LOG file decoded, length: {} chars", log_content.len());
    let (track_offsets, raw_track_sectors) = extract_track_offsets_from_log(&log_content)?;
    info!("Found {} track(s) in LOG file", track_offsets.len());
    info!(
        "LOG METHOD - Raw track start sectors (before adding 150): {:?}",
        raw_track_sectors
    );
    let (lead_out_sectors, raw_leadout_sector) = extract_leadout_from_log(&log_content)
        .ok_or_else(|| {
            warn!(
                "Could not extract lead-out sector from log file. Log content preview (first 500 chars):\n{}",
                log_content.chars().take(500).collect::< String > ()
            );
            MetadataDetectionError::Io(
                std::io::Error::new(
                    std::io::ErrorKind::InvalidData,
                    "Could not extract lead-out sector from log file",
                ),
            )
        })?;
    info!(
        "LOG METHOD - Raw lead-out sector (before adding 150): {}",
        raw_leadout_sector
    );
    info!(
        "LOG METHOD - Lead-out offset: {} sectors (raw: {} + 150)",
        lead_out_sectors, raw_leadout_sector
    );
    let mut offsets = Vec::with_capacity(track_offsets.len() + 1);
    offsets.push(lead_out_sectors);
    offsets.extend_from_slice(&track_offsets);
    let first_track = 1;
    let last_track = track_offsets.len() as i32;
    info!(
        "First track: {}, Last track: {}, Total offsets: {}",
        first_track,
        last_track,
        offsets.len()
    );
    info!("LOG METHOD - Offsets array (lead-out first, then tracks):");
    info!("   Lead-out: {} sectors", offsets[0]);
    for (i, offset) in offsets.iter().enumerate().skip(1) {
        info!("   Track {}: {} sectors", i, offset);
    }
    info!("LOG METHOD - Raw offsets array: {:?}", offsets);
    let disc = discid::DiscId::put(first_track, &offsets).map_err(|e| {
        MetadataDetectionError::Io(std::io::Error::other(format!(
            "Failed to calculate DiscID: {}",
            e
        )))
    })?;
    let mb_discid_str = disc.id();
    info!("MusicBrainz DiscID: {}", mb_discid_str);
    info!("MusicBrainz DiscID result: {}", mb_discid_str);
    Ok(mb_discid_str.to_string())
}
/// Calculate MusicBrainz DiscID from a parsed CUE sheet and its FLAC file.
/// Track offsets come from the CUE; the lead-out is derived from FLAC duration.
pub fn calculate_mb_discid_from_cue_flac(
    sheet: &CueSheet,
    flac_path: &Path,
) -> Result<String, MetadataDetectionError> {
    info!(
        "Calculating MusicBrainz DiscID from CUE+FLAC, FLAC: {:?}",
        flac_path
    );
    let duration_seconds = get_flac_duration_seconds(flac_path)?;
    info!("⏱️ FLAC duration: {:.2} seconds", duration_seconds);
    calculate_mb_discid_from_cue_duration(sheet, duration_seconds, "CUE/FLAC")
}

/// Calculate MusicBrainz DiscID from a parsed CUE sheet and any audio file
/// FFmpeg can probe for duration. Container-agnostic: works for ALAC/AAC
/// `.m4a`, APE, MP3, WAV, OGG, and anything else FFmpeg recognises. Track
/// offsets come from the CUE; the lead-out is derived from probe duration.
///
/// FLAC has its own entry point (`calculate_mb_discid_from_cue_flac`) because
/// it reads STREAMINFO directly instead of going through FFmpeg.
pub fn calculate_mb_discid_from_cue_probe(
    sheet: &CueSheet,
    audio_path: &Path,
) -> Result<String, MetadataDetectionError> {
    info!(
        "Calculating MusicBrainz DiscID from CUE+probe, audio: {:?}",
        audio_path
    );
    let path_str = audio_path.to_str().ok_or_else(|| {
        MetadataDetectionError::Io(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            format!("Non-UTF-8 audio path: {:?}", audio_path),
        ))
    })?;
    let probe = crate::audio_codec::probe_audio_from_path(path_str).ok_or_else(|| {
        MetadataDetectionError::Io(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            format!("Failed to probe audio file: {:?}", audio_path),
        ))
    })?;
    let duration_seconds = probe.duration.as_secs_f64();
    info!("Audio duration: {:.2} seconds", duration_seconds);
    calculate_mb_discid_from_cue_duration(sheet, duration_seconds, "CUE/probe")
}

/// Shared MusicBrainz DiscID calculation: given a parsed CUE sheet and the
/// container's total duration in seconds, build the offsets array (lead-out
/// first, then track starts, all offsets include the 150-sector pregap) and
/// hand it to the `discid` crate. Used by both `_from_cue_flac` (FLAC
/// STREAMINFO duration) and `_from_cue_probe` (FFmpeg duration).
fn calculate_mb_discid_from_cue_duration(
    sheet: &CueSheet,
    duration_seconds: f64,
    method_label: &str,
) -> Result<String, MetadataDetectionError> {
    if sheet.tracks.is_empty() {
        return Err(MetadataDetectionError::Io(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            "CUE has no tracks",
        )));
    }
    let raw_track_sectors: Vec<i32> = sheet
        .tracks
        .iter()
        .map(|t| t.start_cue_frames as i32)
        .collect();
    let track_offsets: Vec<i32> = raw_track_sectors.iter().map(|&r| r + 150).collect();
    info!("Found {} track(s) in CUE file", track_offsets.len());
    info!(
        "{} METHOD - Raw track start sectors (before adding 150): {:?}",
        method_label, raw_track_sectors
    );
    let raw_leadout_sector = (duration_seconds * 75.0).round() as i32;
    let lead_out_sectors = raw_leadout_sector + 150;
    info!(
        "{} METHOD - Raw lead-out sector (from container duration): {} sectors",
        method_label, raw_leadout_sector
    );
    info!(
        "{} METHOD - Lead-out offset: {} sectors (raw: {} + 150)",
        method_label, lead_out_sectors, raw_leadout_sector
    );
    let mut offsets = Vec::with_capacity(track_offsets.len() + 1);
    offsets.push(lead_out_sectors);
    offsets.extend_from_slice(&track_offsets);
    let first_track = 1;
    let last_track = track_offsets.len() as i32;
    info!(
        "First track: {}, Last track: {}, Total offsets: {}",
        first_track,
        last_track,
        offsets.len()
    );
    info!(
        "{} METHOD - Offsets array (lead-out first, then tracks):",
        method_label
    );
    info!("   Lead-out: {} sectors", offsets[0]);
    for (i, offset) in offsets.iter().enumerate().skip(1) {
        info!("   Track {}: {} sectors", i, offset);
    }
    info!("{} METHOD - Raw offsets array: {:?}", method_label, offsets);
    let disc = discid::DiscId::put(first_track, &offsets).map_err(|e| {
        MetadataDetectionError::Io(std::io::Error::other(format!(
            "Failed to calculate DiscID: {}",
            e
        )))
    })?;
    let mb_discid_str = disc.id();
    info!("MusicBrainz DiscID calculated: {}", mb_discid_str);
    Ok(mb_discid_str.to_string())
}

/// Compute a MusicBrainz DiscID from pre-resolved LOG/CUE/audio paths.
/// Tries LOG files first (most accurate — sector offsets come from the EAC
/// or XLD log directly), then CUE+audio pairs as fallback. Returns `None`
/// when no strategy produces a DiscID; failures along the way are logged at
/// `debug!` so the chain is visible in traces.
pub fn compute_discid_from_paths(
    log_paths: &[PathBuf],
    cue_paths: &[PathBuf],
    audio_paths: &[PathBuf],
) -> Option<String> {
    for log_path in log_paths {
        match calculate_mb_discid_from_log(log_path) {
            Ok(id) => return Some(id),
            Err(e) => debug!("DiscID from LOG failed for {:?}: {}", log_path, e),
        }
    }

    for cue_path in cue_paths {
        let sheet = match crate::cue_flac::CueFlacProcessor::parse_cue_sheet(cue_path) {
            Ok(s) => s,
            Err(e) => {
                debug!("Skipping unparseable CUE {:?}: {}", cue_path, e);
                continue;
            }
        };
        let Some(audio_path) = find_matching_audio_for_cue(cue_path, &sheet, audio_paths) else {
            debug!("Skipping CUE with no matching audio file: {:?}", cue_path);
            continue;
        };
        if let Some(id) = discid_from_cue_audio(&sheet, audio_path) {
            return Some(id);
        }
    }

    None
}

/// Compute a MusicBrainz DiscID from an already-parsed CUE sheet and its audio
/// file, dispatched by audio codec. Returns `None` (logging at `debug`) when
/// the extension isn't supported audio or the computation fails — the caller
/// advances to the next candidate. Shared by the path-based and
/// `CategorizedFiles`-based computers so the codec dispatch lives in one place.
fn discid_from_cue_audio(sheet: &CueSheet, audio_path: &Path) -> Option<String> {
    let Some(ext) = audio_path.extension().and_then(|e| e.to_str()) else {
        debug!(
            "Skipping CUE-paired audio with no UTF-8 extension: {:?}",
            audio_path
        );
        return None;
    };
    let result = match ContentTypeHint::from_extension(ext) {
        ContentTypeHint::Flac => calculate_mb_discid_from_cue_flac(sheet, audio_path),
        ContentTypeHint::Mp3 | ContentTypeHint::Ape | ContentTypeHint::Mp4Container => {
            calculate_mb_discid_from_cue_probe(sheet, audio_path)
        }
        other @ (ContentTypeHint::Jpeg
        | ContentTypeHint::Png
        | ContentTypeHint::Gif
        | ContentTypeHint::Webp
        | ContentTypeHint::Bmp
        | ContentTypeHint::Svg
        | ContentTypeHint::PlainText
        | ContentTypeHint::Pdf
        | ContentTypeHint::Unknown(_)) => {
            debug!(
                "Skipping non-audio extension for CUE+audio DiscID: {:?} ({:?})",
                audio_path, other
            );
            return None;
        }
    };
    match result {
        Ok(id) => Some(id),
        Err(e) => {
            debug!("DiscID from CUE+audio failed for {:?}: {}", audio_path, e);
            None
        }
    }
}

/// Compute a MusicBrainz DiscID from already-categorized files, using the CUE
/// sheets the folder scan already parsed — no re-read, no re-parse. LOG first
/// (most accurate), then CUE+audio pairs.
pub fn compute_discid_from_categorized(
    categorized: &crate::import::folder_scanner::CategorizedFiles,
) -> Option<String> {
    use crate::import::folder_scanner::AudioContent;

    for doc in &categorized.documents {
        let is_log = doc
            .path
            .extension()
            .and_then(|e| e.to_str())
            .map(|e| e.eq_ignore_ascii_case("log"))
            .unwrap_or(false);
        if !is_log {
            continue;
        }
        match calculate_mb_discid_from_log(&doc.path) {
            Ok(id) => return Some(id),
            Err(e) => debug!("DiscID from LOG failed for {:?}: {}", doc.path, e),
        }
    }

    if let AudioContent::CueFlacPairs { pairs, .. } = &categorized.audio {
        for pair in pairs {
            let Some(sheet) = &pair.cue_sheet else {
                continue;
            };
            if let Some(id) = discid_from_cue_audio(sheet, &pair.audio_file.path) {
                return Some(id);
            }
        }
    }

    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;
    #[test]
    fn test_extract_leadout_from_log() {
        let log_path = PathBuf::from("tests/fixtures/test_album.log");
        let log_path = if log_path.exists() {
            log_path
        } else {
            PathBuf::from("bae/tests/fixtures/test_album.log")
        };
        if !log_path.exists() {
            eprintln!("LOG file not found at: {:?}", log_path);
            eprintln!("Current directory: {:?}", std::env::current_dir().unwrap());
            return;
        }
        println!("Testing LOG file parsing");
        println!("   LOG: {:?}", log_path);
        if let Err(e) = tracing_subscriber::fmt()
            .with_max_level(tracing::Level::DEBUG)
            .try_init()
        {
            eprintln!("tracing init: {}", e);
        }
        let log_content = crate::text_encoding::read_text_file(&log_path)
            .expect("Failed to read LOG file")
            .text;
        println!(
            "LOG file decoded, length: {} chars, {} lines",
            log_content.len(),
            log_content.lines().count(),
        );
        println!("TOC section:");
        let mut in_toc = false;
        for (i, line) in log_content.lines().enumerate() {
            if line.contains("TOC of the extracted") {
                in_toc = true;
            }
            if in_toc {
                println!("   {}: {}", i + 1, line);
                if line.contains("Range status") || line.contains("AccurateRip") {
                    break;
                }
            }
        }
        let lead_out = extract_leadout_from_log(&log_content);
        match lead_out {
            Some((final_offset, raw_sector)) => {
                println!(
                    "Successfully extracted lead-out: {} sectors (raw: {})",
                    final_offset, raw_sector,
                );
                assert_eq!(
                    final_offset, 188965,
                    "Expected lead-out to be 188965 (188814 + 1 + 150)",
                );
                assert_eq!(
                    raw_sector, 188815,
                    "Expected raw lead-out sector to be 188815 (188814 + 1)",
                );
            }
            None => {
                eprintln!("❌ Failed to extract lead-out from LOG file");
                eprintln!(
                    "LOG content preview (TOC section):\n{}",
                    log_content
                        .lines()
                        .skip_while(|l| !l.contains("TOC of the extracted"))
                        .take(15)
                        .collect::<Vec<_>>()
                        .join("\n"),
                );
                panic!("Failed to extract lead-out");
            }
        }
    }
    #[test]
    fn test_calculate_mb_discid_from_log() {
        let log_path = PathBuf::from("tests/fixtures/test_album.log");
        let log_path = if log_path.exists() {
            log_path
        } else {
            PathBuf::from("bae/tests/fixtures/test_album.log")
        };
        if !log_path.exists() {
            eprintln!("LOG file not found at: {:?}", log_path);
            eprintln!("Current directory: {:?}", std::env::current_dir().unwrap());
            return;
        }
        println!("Testing MB DiscID calculation from LOG file alone");
        println!("   LOG: {:?}", log_path);
        if let Err(e) = tracing_subscriber::fmt()
            .with_max_level(tracing::Level::DEBUG)
            .try_init()
        {
            eprintln!("tracing init: {}", e);
        }
        match calculate_mb_discid_from_log(&log_path) {
            Ok(discid) => {
                println!(
                    "Successfully calculated MusicBrainz DiscID from LOG: {}",
                    discid,
                );
                assert_eq!(discid.len(), 28, "DiscID should be 28 characters");
                assert!(
                    discid
                        .chars()
                        .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_'),
                    "DiscID should contain only alphanumeric characters, dashes, and underscores",
                );
            }
            Err(e) => {
                eprintln!("❌ Failed to calculate DiscID from LOG: {}", e);
                panic!("Failed to calculate DiscID from LOG: {}", e);
            }
        }
    }
    #[test]
    fn test_calculate_mb_discid_from_log_cue_log() {
        let log_path = PathBuf::from("tests/fixtures/test_album.log");
        let log_path = if log_path.exists() {
            log_path
        } else {
            PathBuf::from("bae/tests/fixtures/test_album.log")
        };
        if !log_path.exists() {
            eprintln!("LOG file not found, skipping test");
            eprintln!("  LOG: {:?} (exists: {})", log_path, log_path.exists());
            return;
        }
        println!("Testing MB DiscID calculation from LOG file alone");
        println!("   LOG: {:?}", log_path);
        if let Err(e) = tracing_subscriber::fmt()
            .with_max_level(tracing::Level::DEBUG)
            .try_init()
        {
            eprintln!("tracing init: {}", e);
        }
        match calculate_mb_discid_from_log(&log_path) {
            Ok(discid) => {
                println!("Successfully calculated MusicBrainz DiscID: {}", discid);
                assert_eq!(discid.len(), 28, "DiscID should be 28 characters");
                assert!(
                    discid
                        .chars()
                        .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_'),
                    "DiscID should contain only alphanumeric characters, dashes, and underscores",
                );
            }
            Err(e) => {
                eprintln!("❌ Failed to calculate DiscID: {}", e);
                panic!("Failed to calculate DiscID: {}", e);
            }
        }
    }

    /// The shared duration-based disc ID path is container-agnostic: given a
    /// CUE sheet and a container duration in seconds, the FLAC and probe
    /// wrappers produce the same disc ID. This test fixes that contract
    /// without requiring a real audio file — the wrappers just source the
    /// duration differently (FLAC STREAMINFO vs FFmpeg probe).
    #[test]
    fn test_cue_duration_discid_matches_across_codecs() {
        use crate::cue_flac::{CueSheet, CueTrack};

        // Three tracks, 75 CUE frames/sec → minute 0, minute 3, minute 6.
        let sheet = CueSheet {
            title: Some("Album Title".to_string()),
            performer: Some("Artist Name".to_string()),
            catalog: None,
            date: None,
            tracks: vec![
                CueTrack {
                    number: 1,
                    title: Some("Track 01".to_string()),
                    performer: None,
                    isrc: None,
                    file_reference: "Album.flac".to_string(),
                    start_cue_frames: 0,
                    pregap_cue_frames: None,
                    end_cue_frames: Some(3 * 60 * 75),
                },
                CueTrack {
                    number: 2,
                    title: Some("Track 02".to_string()),
                    performer: None,
                    isrc: None,
                    file_reference: "Album.flac".to_string(),
                    start_cue_frames: 3 * 60 * 75,
                    pregap_cue_frames: None,
                    end_cue_frames: Some(6 * 60 * 75),
                },
                CueTrack {
                    number: 3,
                    title: Some("Track 03".to_string()),
                    performer: None,
                    isrc: None,
                    file_reference: "Album.flac".to_string(),
                    start_cue_frames: 6 * 60 * 75,
                    pregap_cue_frames: None,
                    end_cue_frames: None,
                },
            ],
        };

        let duration_seconds = 9.0 * 60.0;
        let id_a = calculate_mb_discid_from_cue_duration(&sheet, duration_seconds, "CUE/FLAC")
            .expect("disc ID from FLAC path should compute");
        let id_b = calculate_mb_discid_from_cue_duration(&sheet, duration_seconds, "CUE/probe")
            .expect("disc ID from probe path should compute");

        assert_eq!(
            id_a, id_b,
            "duration-based disc ID is codec-agnostic: same sheet + same duration \
             must produce the same disc ID regardless of which method label is logged"
        );
        assert_eq!(id_a.len(), 28, "MusicBrainz disc IDs are 28 chars");
    }

    #[test]
    fn test_cue_duration_discid_empty_tracks_is_error() {
        use crate::cue_flac::CueSheet;

        let sheet = CueSheet {
            title: None,
            performer: None,
            catalog: None,
            date: None,
            tracks: vec![],
        };

        let result = calculate_mb_discid_from_cue_duration(&sheet, 300.0, "CUE/probe");
        assert!(result.is_err(), "empty track list must return an error");
    }

    /// A single-FILE rip with `.cue` + `.ape` produces a disc ID — the
    /// dispatcher routes APE through the FFmpeg-probe path.
    #[test]
    fn test_compute_discid_routes_cue_ape() {
        use tempfile::TempDir;
        let fixture_dir = PathBuf::from("tests/fixtures/cue_ape");
        if !fixture_dir.exists() {
            eprintln!("CUE+APE fixture not found at {:?}", fixture_dir);
            return;
        }
        let tmp = TempDir::new().unwrap();
        let folder = tmp.path();
        std::fs::copy(
            fixture_dir.join("Test Album.ape"),
            folder.join("Test Album.ape"),
        )
        .unwrap();
        std::fs::copy(
            fixture_dir.join("Test Album.cue"),
            folder.join("Test Album.cue"),
        )
        .unwrap();

        let categorized =
            crate::import::folder_scanner::collect_release_candidate_files(folder).unwrap();
        let disc_id = compute_discid_from_categorized(&categorized)
            .expect("CUE+APE pair must compute a disc ID");
        assert_eq!(disc_id.len(), 28, "MusicBrainz disc IDs are 28 chars");
    }

    /// A single-FILE rip with `.cue` + `.mp3` produces a disc ID — the
    /// dispatcher routes MP3 through the FFmpeg-probe path.
    ///
    /// Drives `compute_discid_from_paths` directly with constructed paths
    /// rather than the outer `compute_discid` (which goes through
    /// `folder_scanner`'s CUE+audio pair detection — MP3 pair detection in
    /// the folder scanner is a separate concern).
    #[test]
    fn test_compute_discid_routes_cue_mp3() {
        use std::process::Command;
        use tempfile::TempDir;

        let tmp = TempDir::new().unwrap();
        let folder = tmp.path();
        let mp3_path = folder.join("Test Album.mp3");
        // 9s silent stereo MP3 — short enough to keep the test fast, long
        // enough to span the CUE's three 3-second tracks.
        let output = Command::new("ffmpeg")
            .args([
                "-y",
                "-f",
                "lavfi",
                "-i",
                "anullsrc=channel_layout=stereo:sample_rate=44100",
                "-t",
                "9",
                "-c:a",
                "libmp3lame",
                "-b:a",
                "128k",
                mp3_path.to_str().unwrap(),
            ])
            .output();
        let Ok(output) = output else {
            eprintln!("ffmpeg not available; skipping CUE+MP3 dispatcher test");
            return;
        };
        if !output.status.success() {
            eprintln!(
                "ffmpeg failed to generate MP3 fixture:\nstderr: {}",
                String::from_utf8_lossy(&output.stderr)
            );
            return;
        }

        let cue_body = "PERFORMER \"Artist Name\"\n\
                        TITLE \"Album Title\"\n\
                        FILE \"Test Album.mp3\" WAVE\n  \
                        TRACK 01 AUDIO\n    INDEX 01 00:00:00\n  \
                        TRACK 02 AUDIO\n    INDEX 01 00:03:00\n  \
                        TRACK 03 AUDIO\n    INDEX 01 00:06:00\n";
        let cue_path = folder.join("Test Album.cue");
        std::fs::write(&cue_path, cue_body).unwrap();

        let disc_id = compute_discid_from_paths(&[], &[cue_path], &[mp3_path])
            .expect("CUE+MP3 pair must compute a disc ID");
        assert_eq!(disc_id.len(), 28, "MusicBrainz disc IDs are 28 chars");
    }
}
