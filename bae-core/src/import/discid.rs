use crate::cue_flac::CueSheet;
use crate::import::folder_scanner::resolve_cue_audio_paths;
use std::path::{Path, PathBuf};
use thiserror::Error;
use tracing::{debug, trace, warn};

const CD_PREGAP_SECTORS: i32 = 150;

#[derive(Debug, Error)]
pub enum MetadataDetectionError {
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
}

fn invalid_discid_data(message: impl Into<String>) -> MetadataDetectionError {
    MetadataDetectionError::Io(std::io::Error::new(
        std::io::ErrorKind::InvalidData,
        message.into(),
    ))
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
        .map(|sector| {
            sector.checked_add(CD_PREGAP_SECTORS).ok_or_else(|| {
                invalid_discid_data("track start sector out of range for DiscID calculation")
            })
        })
        .collect::<Result<_, _>>()?;
    let lead_out_sectors = raw_leadout_sector
        .checked_add(CD_PREGAP_SECTORS)
        .ok_or_else(|| {
            invalid_discid_data("lead-out sector out of range for DiscID calculation")
        })?;
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

    let mb_discid_str = super::discid_hash::musicbrainz_discid(&offsets)
        .map_err(|e| invalid_discid_data(format!("Failed to calculate DiscID: {e}")))?;
    debug!("MusicBrainz DiscID calculated: {}", mb_discid_str);
    Ok(mb_discid_str)
}

/// MusicBrainz DiscID from a LOG file alone — the most direct method, since the
/// sector offsets are in the log and neither the CUE nor the audio is needed.
pub fn calculate_mb_discid_from_log(log_path: &Path) -> Result<String, MetadataDetectionError> {
    debug!("Calculating MusicBrainz DiscID from LOG: {:?}", log_path);
    trace!("Reading LOG file: {:?}", log_path);
    let log_content = crate::text_encoding::read_text_file(log_path)?.text;

    trace!("LOG file decoded, length: {} chars", log_content.len());
    let toc_sectors = extract_log_toc_sectors(&log_content)?;
    let raw_track_sectors: Vec<i32> = toc_sectors.iter().map(|(start, _)| *start).collect();
    let last_end_sector = toc_sectors
        .last()
        .expect("LOG TOC parser returned at least one row")
        .1;
    let raw_leadout_sector = last_end_sector
        .checked_add(1)
        .ok_or_else(|| invalid_discid_data("lead-out sector out of range in LOG TOC"))?;
    debug!("Found {} track(s) in LOG file", raw_track_sectors.len());
    discid_from_raw_offsets(
        "LOG",
        &raw_track_sectors,
        raw_leadout_sector,
        "from LOG lead-out",
    )
}
/// From a parsed CUE sheet and the scan's retained container duration, build the
/// offsets array (lead-out first, then track starts, every offset including the
/// 150-sector pregap) and hand it to the DiscID calculator.
fn calculate_mb_discid_from_cue_duration(
    sheet: &CueSheet,
    duration_seconds: f64,
    method_label: &str,
) -> Result<String, MetadataDetectionError> {
    if sheet.playable_tracks().next().is_none() {
        return Err(MetadataDetectionError::Io(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            "CUE has no playable audio tracks",
        )));
    }
    let raw_track_sectors: Vec<i32> = sheet
        .playable_tracks()
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

/// A MusicBrainz DiscID from pre-resolved LOG/CUE/audio paths. LOG files come
/// first — most accurate, since the EAC or XLD log carries the sector offsets
/// directly — then CUE+audio pairs. `None` when nothing produces a DiscID;
/// failures along the way log at `debug!` so the chain shows up in traces.
pub fn compute_discid_from_paths(
    log_paths: &[PathBuf],
    cue_paths: &[PathBuf],
    audio_files: &[(PathBuf, u64)],
) -> Option<String> {
    for log_path in log_paths {
        match calculate_mb_discid_from_log(log_path) {
            Ok(id) => return Some(id),
            Err(e) => debug!("DiscID from LOG failed for {:?}: {}", log_path, e),
        }
    }

    let audio_paths = audio_files
        .iter()
        .map(|(path, _)| path.clone())
        .collect::<Vec<_>>();
    for cue_path in cue_paths {
        let sheet = match crate::cue_flac::parse_cue_sheet(cue_path) {
            Ok(s) => s,
            Err(e) => {
                debug!("Skipping unparseable CUE {:?}: {}", cue_path, e);
                continue;
            }
        };
        let Some(resolved) = resolve_cue_audio_paths(cue_path, &sheet, &audio_paths) else {
            debug!("Skipping CUE with no matching audio file: {:?}", cue_path);
            continue;
        };
        let [(_, audio_path)] = resolved.as_slice() else {
            debug!("Skipping multi-file CUE for DiscID: {:?}", cue_path);
            continue;
        };
        let duration_ms = audio_files
            .iter()
            .find_map(|(path, duration_ms)| (path == *audio_path).then_some(*duration_ms))
            .expect("a matched CUE audio path came from the retained-duration list");
        if let Some(id) = discid_from_cue_duration(&sheet, duration_ms, audio_path) {
            return Some(id);
        }
    }

    None
}

/// A MusicBrainz DiscID from an already-parsed CUE sheet and the duration the
/// authoritative scan retained for its physical audio file.
fn discid_from_cue_duration(
    sheet: &CueSheet,
    duration_ms: u64,
    audio_path: &Path,
) -> Option<String> {
    trace!(
        "Retained audio duration for {:?}: {} ms",
        audio_path,
        duration_ms
    );
    match calculate_mb_discid_from_cue_duration(
        sheet,
        duration_ms as f64 / 1000.0,
        "CUE/scanned audio",
    ) {
        Ok(id) => Some(id),
        Err(e) => {
            debug!("DiscID from CUE+audio failed for {:?}: {}", audio_path, e);
            None
        }
    }
}

/// A MusicBrainz DiscID from already-categorized files, reusing the track sheets
/// the folder scan parsed — no re-read, no re-parse. LOG first (most accurate),
/// then the sheets that are bound to their audio. A folder whose sheet is
/// unbound can still identify itself from its log.
/// A disc ID and the file it was derived from — the rip log, or the sheet that
/// carves the tracks. The file rides along so a surface can put the disc ID on
/// the row for that file rather than beside the release.
pub struct ComputedDiscId {
    pub disc_id: String,
    /// The candidate-relative path of the LOG or CUE it came from.
    pub source_file: String,
}

pub fn compute_discid_from_categorized(
    categorized: &crate::import::folder_scanner::CategorizedFiles,
) -> Option<ComputedDiscId> {
    for doc in categorized.documents() {
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
            Ok(id) => {
                return Some(ComputedDiscId {
                    disc_id: id,
                    source_file: doc.relative_path.clone(),
                })
            }
            Err(e) => debug!("DiscID from LOG failed for {:?}: {}", doc.path, e),
        }
    }

    // Only the sheets that carve: one the user took out of the tracklist
    // describes a disc this folder is no longer presenting.
    for bound in categorized.carving_sheets() {
        let [resolved] = bound.audio_files.as_slice() else {
            debug!("Skipping multi-file CUE for DiscID: {:?}", bound.file.path);
            continue;
        };
        let source_audio = resolved
            .1
            .source_audio
            .as_ref()
            .expect("a categorized audio file retains its scan facts");
        if let Some(id) =
            discid_from_cue_duration(bound.sheet, source_audio.duration_ms, &resolved.1.path)
        {
            return Some(ComputedDiscId {
                disc_id: id,
                source_file: bound.file.relative_path.clone(),
            });
        }
    }

    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    /// Resolve a checked-in test fixture by path relative to the crate root, so
    /// tests don't depend on the process working directory.
    fn fixture(rel: &str) -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures")
            .join(rel)
    }

    fn assert_invalid_data(result: Result<String, MetadataDetectionError>) {
        let error = result.expect_err("out-of-range sector should return an error");
        match error {
            MetadataDetectionError::Io(error) => {
                assert_eq!(error.kind(), std::io::ErrorKind::InvalidData);
            }
        }
    }

    fn write_log_with_toc(end_sector: i32) -> tempfile::NamedTempFile {
        let file = tempfile::NamedTempFile::new().expect("test LOG file should be created");
        let content = format!(
            "TOC of the extracted CD\n\
             \n\
             Track | Start | Length | Start sector | End sector\n\
             ---------------------------------------------------\n\
             1 | 0:00.00 | 1:00.00 | 0 | {end_sector}\n\
             \n\
             Range status and errors\n"
        );
        std::fs::write(file.path(), content).expect("test LOG file should be written");
        file
    }

    #[test]
    fn discid_rejects_track_start_sector_that_overflows_pregap_offset() {
        assert_invalid_data(discid_from_raw_offsets(
            "LOG",
            &[i32::MAX],
            100,
            "from test lead-out",
        ));
    }

    #[test]
    fn discid_rejects_leadout_sector_that_overflows_pregap_offset() {
        assert_invalid_data(discid_from_raw_offsets(
            "LOG",
            &[0],
            i32::MAX,
            "from test lead-out",
        ));
    }

    #[test]
    fn discid_from_log_rejects_end_sector_that_overflows_leadout_derivation() {
        let log_file = write_log_with_toc(i32::MAX);

        assert_invalid_data(calculate_mb_discid_from_log(log_file.path()));
    }

    #[test]
    fn test_extract_leadout_from_log() {
        let log_content = crate::text_encoding::read_text_file(&fixture("test_album.log"))
            .expect("LOG fixture should be readable")
            .text;
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
        let discid = calculate_mb_discid_from_log(&fixture("test_album.log"))
            .expect("disc ID should compute from the LOG fixture");
        assert_eq!(discid.len(), 28, "DiscID should be 28 characters");
        assert!(
            discid
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_'),
            "DiscID should contain only alphanumeric characters, dashes, and underscores",
        );
    }

    /// The shared duration-based disc ID path is container-agnostic: given a
    /// CUE sheet and a container duration in seconds, the FLAC and probe
    /// wrappers produce the same disc ID.
    #[test]
    fn test_cue_duration_discid_matches_across_codecs() {
        use crate::cue_flac::{CueIndex, CuePregap, CueSheet, CueTrack, CueTrackMode};

        // Three tracks, 75 CUE frames/sec → minute 0, minute 3, minute 6.
        let sheet = CueSheet {
            title: Some("Album Title".to_string()),
            performer: Some("Artist Name".to_string()),
            catalog: None,
            date: None,
            tracks: vec![
                CueTrack {
                    number: 1,
                    mode: CueTrackMode::Audio,
                    title: Some("Track 01".to_string()),
                    performer: None,
                    indexes: vec![CueIndex {
                        number: 1,
                        frames: 0,
                        file_reference: "Album.flac".to_string(),
                    }],
                    file_reference: "Album.flac".to_string(),
                    start_cue_frames: 0,
                    pregap: CuePregap::None,
                    end_cue_frames: Some(3 * 60 * 75),
                },
                CueTrack {
                    number: 2,
                    mode: CueTrackMode::Audio,
                    title: Some("Track 02".to_string()),
                    performer: None,
                    indexes: vec![CueIndex {
                        number: 1,
                        frames: 3 * 60 * 75,
                        file_reference: "Album.flac".to_string(),
                    }],
                    file_reference: "Album.flac".to_string(),
                    start_cue_frames: 3 * 60 * 75,
                    pregap: CuePregap::None,
                    end_cue_frames: Some(6 * 60 * 75),
                },
                CueTrack {
                    number: 3,
                    mode: CueTrackMode::Audio,
                    title: Some("Track 03".to_string()),
                    performer: None,
                    indexes: vec![CueIndex {
                        number: 1,
                        frames: 6 * 60 * 75,
                        file_reference: "Album.flac".to_string(),
                    }],
                    file_reference: "Album.flac".to_string(),
                    start_cue_frames: 6 * 60 * 75,
                    pregap: CuePregap::None,
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
    fn cue_duration_discid_ignores_non_audio_tracks() {
        use crate::cue_flac::{CueIndex, CuePregap, CueSheet, CueTrack, CueTrackMode};

        fn track(number: u32, mode: CueTrackMode, start_cue_frames: u64) -> CueTrack {
            CueTrack {
                number,
                mode,
                title: Some(format!("Track {number:02}")),
                performer: None,
                indexes: vec![CueIndex {
                    number: 1,
                    frames: start_cue_frames,
                    file_reference: "disc-image.bin".to_string(),
                }],
                file_reference: "disc-image.bin".to_string(),
                start_cue_frames,
                pregap: CuePregap::None,
                end_cue_frames: None,
            }
        }

        let audio_tracks = vec![
            track(1, CueTrackMode::Audio, 0),
            track(2, CueTrackMode::Audio, 3 * 60 * 75),
        ];
        let audio_only = CueSheet {
            title: Some("Album Title".to_string()),
            performer: Some("Artist Name".to_string()),
            catalog: None,
            date: None,
            tracks: audio_tracks.clone(),
        };
        let with_data_track = CueSheet {
            title: Some("Album Title".to_string()),
            performer: Some("Artist Name".to_string()),
            catalog: None,
            date: None,
            tracks: audio_tracks
                .into_iter()
                .chain(std::iter::once(track(
                    3,
                    CueTrackMode::Other("MODE1/2352".to_string()),
                    6 * 60 * 75,
                )))
                .collect(),
        };

        let duration_seconds = 6.0 * 60.0;
        let audio_only_id =
            calculate_mb_discid_from_cue_duration(&audio_only, duration_seconds, "CUE/probe")
                .expect("audio-only CUE should compute a disc ID");
        let with_data_track_id =
            calculate_mb_discid_from_cue_duration(&with_data_track, duration_seconds, "CUE/probe")
                .expect("CUE with a data track should compute a disc ID");

        assert_eq!(with_data_track_id, audio_only_id);
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
        let fixture_dir = fixture("cue_ape");
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
            crate::import::folder_scanner::collect_release_candidate_files_with_scope(
                folder,
                crate::import::ReleaseFileScope::Recursive,
                &crate::import::folder_scanner::StoredCandidateEdits::none(),
            )
            .unwrap();
        let audio_path = folder.join("Test Album.ape");
        let opens_before = crate::audio_codec::probe_opens_for(&audio_path);
        let computed = compute_discid_from_categorized(&categorized)
            .expect("CUE+APE pair must compute a disc ID");
        assert_eq!(
            crate::audio_codec::probe_opens_for(&audio_path),
            opens_before,
            "DiscID must reuse the duration retained by the folder scan"
        );
        assert_eq!(
            computed.disc_id.len(),
            28,
            "MusicBrainz disc IDs are 28 chars"
        );
        assert!(
            computed.source_file.ends_with(".cue"),
            "the sheet it was carved from rides with it, got {:?}",
            computed.source_file
        );
    }

    /// A single-FILE rip with `.cue` + `.mp3` produces a disc ID — the dispatcher
    /// routes MP3 through the FFmpeg-probe path.
    ///
    /// Drives `compute_discid_from_paths` with constructed paths rather than
    /// `compute_discid_from_categorized`, which would go through the folder
    /// scanner's CUE+audio pair detection — a separate concern for MP3.
    #[test]
    fn test_compute_discid_routes_cue_mp3() {
        use tempfile::TempDir;

        crate::audio_codec::init();
        let tmp = TempDir::new().unwrap();
        let folder = tmp.path();
        let mp3_path = folder.join("Test Album.mp3");
        // 9s silent stereo MP3 — short enough to keep the test fast, long
        // enough to span the CUE's three 3-second tracks.
        let samples = vec![0i32; 44_100 * 9 * 2];
        let mp3_bytes = crate::audio_codec::encode_i32(
            crate::audio_codec::EncodeFormat::Mp3 { bitrate_kbps: 320 },
            &samples,
            44_100,
            2,
        )
        .expect("encode mp3");
        std::fs::write(&mp3_path, mp3_bytes).unwrap();

        let cue_body = "PERFORMER \"Artist Name\"\n\
                        TITLE \"Album Title\"\n\
                        FILE \"Test Album.mp3\" WAVE\n  \
                        TRACK 01 AUDIO\n    INDEX 01 00:00:00\n  \
                        TRACK 02 AUDIO\n    INDEX 01 00:03:00\n  \
                        TRACK 03 AUDIO\n    INDEX 01 00:06:00\n";
        let cue_path = folder.join("Test Album.cue");
        std::fs::write(&cue_path, cue_body).unwrap();

        let disc_id = compute_discid_from_paths(&[], &[cue_path], &[(mp3_path, 9_000)])
            .expect("CUE+MP3 pair must compute a disc ID");
        assert_eq!(disc_id.len(), 28, "MusicBrainz disc IDs are 28 chars");
    }

    #[test]
    fn parse_log_toc_row_accepts_valid_and_rejects_malformed() {
        // A well-formed EAC/XLD TOC row: track | start | length | start | end.
        assert_eq!(
            parse_log_toc_row("   1 | 0:00.00 | 1:00.00 |      0 |  188814"),
            Some((0, 188814))
        );
        // Fewer than five pipe-separated columns.
        assert_eq!(parse_log_toc_row("1 | 0:00.00 | 1:00.00 | 0"), None);
        // Track number outside 1..=99.
        assert_eq!(parse_log_toc_row("0 | 0:00.00 | 1:00.00 | 0 | 100"), None);
        assert_eq!(parse_log_toc_row("100 | 0:00.00 | 1:00.00 | 0 | 100"), None);
        // Non-numeric track / sector cells.
        assert_eq!(parse_log_toc_row("x | 0:00.00 | 1:00.00 | 0 | 100"), None);
        assert_eq!(parse_log_toc_row("1 | 0:00.00 | 1:00.00 | a | 100"), None);
        // Negative start sector, or non-positive end sector.
        assert_eq!(parse_log_toc_row("1 | 0:00.00 | 1:00.00 | -1 | 100"), None);
        assert_eq!(parse_log_toc_row("1 | 0:00.00 | 1:00.00 | 0 | 0"), None);
    }

    #[test]
    fn extract_log_toc_sectors_errors_without_toc_rows() {
        let log = "Some header line\nRandom prose\nNo TOC table anywhere\n";
        let err = extract_log_toc_sectors(log).expect_err("a LOG with no TOC rows must error");
        match err {
            MetadataDetectionError::Io(e) => {
                assert_eq!(e.kind(), std::io::ErrorKind::InvalidData);
            }
        }
    }

    #[test]
    fn extract_log_toc_sectors_parses_headerless_table() {
        // A bare TOC table with no "TOC of the extracted CD" header still parses:
        // the first parseable row opens the section.
        let log = "   1 | 0:00.00 | 1:00.00 | 0 | 100\n   2 | 1:00.00 | 1:00.00 | 100 | 200\n";
        let rows = extract_log_toc_sectors(log).expect("headerless TOC table should parse");
        assert_eq!(rows, vec![(0, 100), (100, 200)]);
    }

    #[test]
    fn extract_log_toc_sectors_skips_unparseable_rows() {
        // A header, a malformed row (track 0), then a valid row: the bad row is
        // dropped rather than aborting the parse.
        let log = "TOC of the extracted CD\n\
                   Track | Start | Length | Start sector | End sector\n\
                   ---------------------------------------------------\n\
                   0 | 0:00.00 | 1:00.00 | 0 | 100\n\
                   1 | 0:00.00 | 1:00.00 | 10 | 200\n\
                   \n\
                   Range status and errors\n";
        let rows = extract_log_toc_sectors(log).expect("the valid row should parse");
        assert_eq!(rows, vec![(10, 200)], "the invalid track-0 row is dropped");
    }

    fn audio_cue_track(number: u32, file_reference: &str) -> crate::cue_flac::CueTrack {
        use crate::cue_flac::{CueIndex, CuePregap, CueTrack, CueTrackMode};
        CueTrack {
            number,
            mode: CueTrackMode::Audio,
            title: Some(format!("Track {number:02}")),
            performer: None,
            indexes: vec![CueIndex {
                number: 1,
                frames: 0,
                file_reference: file_reference.to_string(),
            }],
            file_reference: file_reference.to_string(),
            start_cue_frames: 0,
            pregap: CuePregap::None,
            end_cue_frames: None,
        }
    }

    fn cue_sheet_with(tracks: Vec<crate::cue_flac::CueTrack>) -> CueSheet {
        CueSheet {
            title: Some("Album Title".to_string()),
            performer: Some("Artist Name".to_string()),
            catalog: None,
            date: None,
            tracks,
        }
    }

    /// A single-FILE CUE matches the unique same-stem audio beside the sheet.
    #[test]
    fn resolve_cue_audio_paths_matches_by_stem() {
        let sheet = cue_sheet_with(vec![
            audio_cue_track(1, "Album Image.flac"),
            audio_cue_track(2, "Album Image.flac"),
        ]);
        let audio_files = vec![
            PathBuf::from("/rip/Some Other.mp3"),
            PathBuf::from("/rip/Album Image.flac"),
        ];

        let matched = resolve_cue_audio_paths(Path::new("/rip/Album.cue"), &sheet, &audio_files)
            .expect("the unique same-stem audio should resolve");
        assert_eq!(matched[0].1, &PathBuf::from("/rip/Album Image.flac"));
    }

    /// No audio file shares the FILE reference's stem → no match.
    #[test]
    fn resolve_cue_audio_paths_no_stem_match_is_none() {
        let sheet = cue_sheet_with(vec![audio_cue_track(1, "Album Image.flac")]);
        let audio_files = vec![PathBuf::from("/rip/Different Name.flac")];

        assert!(
            resolve_cue_audio_paths(Path::new("/rip/Album.cue"), &sheet, &audio_files).is_none()
        );
    }

    #[test]
    fn resolve_cue_audio_paths_ambiguous_same_stem_is_none() {
        let sheet = cue_sheet_with(vec![audio_cue_track(1, "Album Image.wav")]);
        let audio_files = vec![
            PathBuf::from("/rip/Album Image.flac"),
            PathBuf::from("/rip/Album Image.ape"),
        ];

        assert!(
            resolve_cue_audio_paths(Path::new("/rip/Album.cue"), &sheet, &audio_files).is_none()
        );
    }

    #[test]
    fn resolve_cue_audio_paths_exact_path_wins_over_same_stem_audio() {
        let sheet = cue_sheet_with(vec![audio_cue_track(1, "Album Image.flac")]);
        let audio_files = vec![
            PathBuf::from("/rip/Album Image.ape"),
            PathBuf::from("/rip/Album Image.flac"),
        ];

        assert_eq!(
            resolve_cue_audio_paths(Path::new("/rip/Album.cue"), &sheet, &audio_files)
                .expect("the exact path should resolve")[0]
                .1,
            &PathBuf::from("/rip/Album Image.flac"),
        );
    }

    #[test]
    fn resolve_cue_audio_paths_other_directory_is_none() {
        let sheet = cue_sheet_with(vec![audio_cue_track(1, "Album Image.wav")]);
        let audio_files = vec![PathBuf::from("/rip/audio/Album Image.flac")];

        assert!(
            resolve_cue_audio_paths(Path::new("/rip/Album.cue"), &sheet, &audio_files).is_none()
        );
    }

    #[test]
    /// Same-stem fallback follows the reference's directory, not only the
    /// directory containing the CUE.
    fn resolve_cue_audio_paths_matches_by_stem_in_referenced_subdirectory() {
        let sheet = cue_sheet_with(vec![audio_cue_track(1, "audio/Album Image.wav")]);
        let audio_files = vec![PathBuf::from("/rip/audio/Album Image.flac")];

        let matched = resolve_cue_audio_paths(Path::new("/rip/Album.cue"), &sheet, &audio_files)
            .expect("same-stem audio beside the referenced path should resolve");
        assert_eq!(matched[0].1, &PathBuf::from("/rip/audio/Album Image.flac"));
    }

    /// Every reference in a multi-FILE CUE resolves independently.
    #[test]
    fn resolve_cue_audio_paths_resolves_multiple_files() {
        let sheet = cue_sheet_with(vec![
            audio_cue_track(1, "Track 01.flac"),
            audio_cue_track(2, "Track 02.flac"),
        ]);
        let audio_files = vec![
            PathBuf::from("/rip/Track 01.flac"),
            PathBuf::from("/rip/Track 02.flac"),
        ];

        let matched = resolve_cue_audio_paths(Path::new("/rip/Album.cue"), &sheet, &audio_files)
            .expect("both referenced audio files should resolve");
        assert_eq!(
            matched
                .into_iter()
                .map(|(_, path)| path.as_path())
                .collect::<Vec<_>>(),
            vec![
                Path::new("/rip/Track 01.flac"),
                Path::new("/rip/Track 02.flac"),
            ]
        );
    }
}
