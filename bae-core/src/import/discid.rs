use crate::cue_flac::CueSheet;
use crate::util::content_type_hint::ContentTypeHint;
use std::path::{Path, PathBuf};
use thiserror::Error;
use tracing::{debug, trace, warn};
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
fn probe_duration_seconds(audio_path: &Path) -> Result<f64, MetadataDetectionError> {
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
    Ok(probe.duration.as_secs_f64())
}
fn parse_log_toc_row(line: &str) -> Option<(i32, i32)> {
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

    Some((start_sector, end_sector))
}

/// Extract raw `(start_sector, end_sector)` pairs from an EAC/XLD LOG TOC table.
/// Format: "       10  | 37:42.72 |  4:14.43 |    169722    |   188814"
fn extract_log_toc_sectors(log_content: &str) -> Result<Vec<(i32, i32)>, MetadataDetectionError> {
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
        if let Some((start_sector, end_sector)) = toc_row {
            track_sectors.push((start_sector, end_sector));
            trace!(
                "  Track {} sectors: start={}, end={}",
                track_sectors.len(),
                start_sector,
                end_sector
            );
        }
    }
    if track_sectors.is_empty() {
        warn!("Could not find any TOC rows in LOG file");
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
        return Err(MetadataDetectionError::Io(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            "No TOC rows found in LOG file",
        )));
    }

    Ok(track_sectors)
}

fn discid_from_raw_offsets(
    method_label: &str,
    raw_track_sectors: &[i32],
    raw_leadout_sector: i32,
    leadout_source: &str,
) -> Result<String, MetadataDetectionError> {
    let track_offsets: Vec<i32> = raw_track_sectors
        .iter()
        .map(|sector| sector + 150)
        .collect();
    let lead_out_sectors = raw_leadout_sector + 150;
    debug!(
        "{method_label} raw track start sectors before adding 150: {:?}",
        raw_track_sectors
    );
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

    let disc = discid::DiscId::put(1, &offsets).map_err(|e| {
        MetadataDetectionError::Io(std::io::Error::other(format!(
            "Failed to calculate DiscID: {}",
            e
        )))
    })?;
    let mb_discid_str = disc.id();
    debug!("MusicBrainz DiscID calculated: {}", mb_discid_str);
    Ok(mb_discid_str.to_string())
}

/// Calculate MusicBrainz DiscID from LOG file alone
/// This is the most efficient method as it doesn't require CUE or audio files
pub fn calculate_mb_discid_from_log(log_path: &Path) -> Result<String, MetadataDetectionError> {
    debug!("Calculating MusicBrainz DiscID from LOG: {:?}", log_path);
    trace!("Reading LOG file: {:?}", log_path);
    let log_content = crate::text_encoding::read_text_file(log_path)?.text;

    trace!("LOG file decoded, length: {} chars", log_content.len());
    let toc_sectors = extract_log_toc_sectors(&log_content)?;
    let raw_track_sectors: Vec<i32> = toc_sectors.iter().map(|(start, _)| *start).collect();
    let raw_leadout_sector = toc_sectors
        .last()
        .expect("LOG TOC parser returned at least one row")
        .1
        + 1;
    debug!("Found {} track(s) in LOG file", raw_track_sectors.len());
    discid_from_raw_offsets(
        "LOG",
        &raw_track_sectors,
        raw_leadout_sector,
        "from LOG lead-out",
    )
}
/// Calculate MusicBrainz DiscID from a parsed CUE sheet and its FLAC file.
/// Track offsets come from the CUE; the lead-out is derived from probe duration.
pub fn calculate_mb_discid_from_cue_flac(
    sheet: &CueSheet,
    flac_path: &Path,
) -> Result<String, MetadataDetectionError> {
    calculate_mb_discid_from_cue_audio(sheet, flac_path, "CUE/FLAC", "FLAC")
}

fn calculate_mb_discid_from_cue_audio(
    sheet: &CueSheet,
    audio_path: &Path,
    method_label: &str,
    duration_label: &str,
) -> Result<String, MetadataDetectionError> {
    debug!(
        "Calculating MusicBrainz DiscID from {method_label}, audio: {:?}",
        audio_path
    );
    let duration_seconds = probe_duration_seconds(audio_path)?;
    trace!("{duration_label} duration: {:.2} seconds", duration_seconds);
    calculate_mb_discid_from_cue_duration(sheet, duration_seconds, method_label)
}

/// Calculate MusicBrainz DiscID from a parsed CUE sheet and any audio file
/// FFmpeg can probe for duration. Container-agnostic: works for ALAC/AAC
/// `.m4a`, APE, MP3, WAV, OGG, and anything else FFmpeg recognises. Track
/// offsets come from the CUE; the lead-out is derived from probe duration.
///
pub fn calculate_mb_discid_from_cue_probe(
    sheet: &CueSheet,
    audio_path: &Path,
) -> Result<String, MetadataDetectionError> {
    calculate_mb_discid_from_cue_audio(sheet, audio_path, "CUE/probe", "Audio")
}

/// Shared MusicBrainz DiscID calculation: given a parsed CUE sheet and the
/// container's total duration in seconds, build the offsets array (lead-out
/// first, then track starts, all offsets include the 150-sector pregap) and
/// hand it to the `discid` crate. Used by both `_from_cue_flac` and
/// `_from_cue_probe`.
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
    let raw_leadout_sector = (duration_seconds * 75.0).round() as i32;
    debug!("Found {} track(s) in CUE file", raw_track_sectors.len());
    discid_from_raw_offsets(
        method_label,
        &raw_track_sectors,
        raw_leadout_sector,
        "from container duration",
    )
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
        ContentTypeHint::Mp3
        | ContentTypeHint::Ape
        | ContentTypeHint::Mp4Container
        | ContentTypeHint::WavContainer
        | ContentTypeHint::AiffContainer
        | ContentTypeHint::WavPack
        | ContentTypeHint::DsdContainer => calculate_mb_discid_from_cue_probe(sheet, audio_path),
        other @ (ContentTypeHint::Jpeg
        | ContentTypeHint::Png
        | ContentTypeHint::Gif
        | ContentTypeHint::Webp
        | ContentTypeHint::Bmp
        | ContentTypeHint::Svg
        | ContentTypeHint::OggContainer
        | ContentTypeHint::OpusContainer
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
            if let Some(id) = discid_from_cue_audio(&pair.cue_sheet, &pair.audio_file.path) {
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
        let toc_sectors = extract_log_toc_sectors(&log_content).expect("LOG TOC should parse");
        let last_end_sector = toc_sectors
            .last()
            .expect("LOG TOC should include at least one track")
            .1;
        let raw_sector = last_end_sector + 1;
        let final_offset = raw_sector + 150;
        assert_eq!(
            final_offset, 188965,
            "Expected lead-out to be 188965 (188814 + 1 + 150)",
        );
        assert_eq!(
            raw_sector, 188815,
            "Expected raw lead-out sector to be 188815 (188814 + 1)",
        );
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
    /// wrappers produce the same disc ID.
    #[test]
    fn test_cue_duration_discid_matches_across_codecs() {
        use crate::cue_flac::{CuePregap, CueSheet, CueTrack};

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
                    pregap: CuePregap::None,
                    pregap_cue_frames: None,
                    generated_pregap_frames: None,
                    end_cue_frames: Some(3 * 60 * 75),
                },
                CueTrack {
                    number: 2,
                    title: Some("Track 02".to_string()),
                    performer: None,
                    isrc: None,
                    file_reference: "Album.flac".to_string(),
                    start_cue_frames: 3 * 60 * 75,
                    pregap: CuePregap::None,
                    pregap_cue_frames: None,
                    generated_pregap_frames: None,
                    end_cue_frames: Some(6 * 60 * 75),
                },
                CueTrack {
                    number: 3,
                    title: Some("Track 03".to_string()),
                    performer: None,
                    isrc: None,
                    file_reference: "Album.flac".to_string(),
                    start_cue_frames: 6 * 60 * 75,
                    pregap: CuePregap::None,
                    pregap_cue_frames: None,
                    generated_pregap_frames: None,
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
