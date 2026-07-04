use crate::cue_flac::CueSheet;
use crate::db::DbTrack;
use crate::import::folder_scanner::{
    AudioContent, CategorizedFiles, ScannedCueFlacPair, ScannedFile,
};
use crate::import::types::{CueAudioAnalysis, CueFlacAnalysis, TrackFile};
use std::sync::Arc;
use tracing::{debug, info, warn};

/// Map tracks to their source audio files using the scan's categorised output.
///
/// Runs BEFORE database insertion. The scan has already detected CUE/FLAC
/// pairs, parsed their CUE sheets, and classified files — this function
/// never re-parses or re-detects.
///
/// For CUE-backed imports the caller's `Vec<DbTrack>` is consumed in order
/// and sliced per pair by the pair's parsed CUE track count. For per-track
/// imports each track maps 1:1 to an audio file. In both cases each DbTrack
/// is moved into a TrackFile variant and its `duration_ms` is populated
/// from the CUE sheet or a standalone-file probe.
pub fn map_tracks_to_files(
    tracks: Vec<DbTrack>,
    files: &CategorizedFiles,
) -> Result<Vec<TrackFile>, String> {
    info!("Mapping {} tracks using scan output", tracks.len());
    match &files.audio {
        AudioContent::CueFlacPairs { pairs, .. } => map_tracks_to_cue_flacs(tracks, pairs),
        AudioContent::TrackFiles { tracks: files, .. } => {
            map_tracks_to_individual_files(tracks, files)
        }
    }
}

fn map_tracks_to_cue_flacs(
    tracks: Vec<DbTrack>,
    pairs: &[ScannedCueFlacPair],
) -> Result<Vec<TrackFile>, String> {
    if pairs.is_empty() {
        return Err("CUE/FLAC import has no pairs".to_string());
    }

    // Natural-sort pairs by relative path: CD1, CD2, ... CD10, and Side A,
    // Side B, ... read in the order they ship on disk. Same convention the
    // scan records and the UI displays.
    let mut sorted: Vec<&ScannedCueFlacPair> = pairs.iter().collect();
    sorted.sort_by(|a, b| natord::compare(&a.cue_file.relative_path, &b.cue_file.relative_path));

    // Pre-parsed CUE sheets per pair. The folder scan populates these; a
    // `None` here means a pair reached the mapper with its CUE unparsed,
    // which is a bug — the mapper needs the parsed sheet to align tracks.
    let sheets: Vec<&CueSheet> = sorted
        .iter()
        .map(|p| {
            p.cue_sheet
                .as_ref()
                .ok_or_else(|| format!("CUE sheet not parsed for pair {:?}", p.cue_file.path))
        })
        .collect::<Result<_, _>>()?;

    let per_pair_counts: Vec<usize> = sheets.iter().map(|s| s.tracks.len()).collect();
    let total_cue_tracks: usize = per_pair_counts.iter().sum();
    if total_cue_tracks != tracks.len() {
        return Err(format!(
            "Track count mismatch: CUE pairs contain {} tracks in total but release has {} tracks",
            total_cue_tracks,
            tracks.len(),
        ));
    }

    let mut track_files = Vec::with_capacity(tracks.len());
    let mut remaining: std::collections::VecDeque<DbTrack> = tracks.into();
    for (pair, count) in sorted.iter().zip(per_pair_counts.iter()) {
        let slice: Vec<DbTrack> = remaining.drain(..*count).collect();
        track_files.extend(map_tracks_to_cue_flac(pair, slice)?);
    }
    info!(
        "Created {} CUE/FLAC mappings with validated metadata",
        track_files.len()
    );
    Ok(track_files)
}

/// Process a single CUE/FLAC pair: use the scan's already-parsed CUE sheet,
/// probe the audio container, and emit one `TrackFile::CueBacked` per track
/// — all sharing the same `Arc<CueFlacAnalysis>`. Each DbTrack is moved in;
/// its `duration_ms` is populated from the CUE sheet before the DbTrack
/// moves into the variant.
fn map_tracks_to_cue_flac(
    pair: &ScannedCueFlacPair,
    tracks: Vec<DbTrack>,
) -> Result<Vec<TrackFile>, String> {
    let cue_sheet = pair
        .cue_sheet
        .clone()
        .expect("map_tracks_to_cue_flacs already verified cue_sheet is Some");
    debug!(
        "Processing CUE/FLAC pair: {} + {} ({} tracks)",
        pair.audio_file.path.display(),
        pair.cue_file.path.display(),
        cue_sheet.tracks.len()
    );
    if cue_sheet.tracks.is_empty() {
        return Err(format!(
            "CUE sheet '{}' contains no tracks. Check CUE file format.",
            pair.cue_file.path.display(),
        ));
    }
    // Caller pre-sliced `tracks` to match the CUE's track count, so the
    // zip-by-index below is always exact.
    assert_eq!(
        cue_sheet.tracks.len(),
        tracks.len(),
        "caller must slice tracks to match per-pair CUE track count",
    );

    let analysis = analyze_cue_audio(&pair.audio_file.path)?;
    let pair_analysis = Arc::new(CueFlacAnalysis {
        cue_sheet,
        analysis,
    });

    let mut mappings = Vec::with_capacity(tracks.len());
    for (index, mut db_track) in tracks.into_iter().enumerate() {
        let cue_track = &pair_analysis.cue_sheet.tracks[index];
        // CUE sheets give us exact per-track timing. The final track has no
        // next-track boundary in the sheet, so its duration is derived from
        // the container's total duration minus its INDEX 01 start.
        db_track.duration_ms = cue_track.track_duration_ms().map(|d| d as i64).or_else(|| {
            container_duration_ms(&pair_analysis.analysis)
                .map(|total| total - cue_track.start_time_ms() as i64)
        });
        debug!(
            "Mapped CUE track {:?} to DB track '{}' (duration {:?}ms)",
            cue_track.title, db_track.title, db_track.duration_ms
        );
        mappings.push(TrackFile::CueBacked {
            db_track,
            file_path: pair.audio_file.path.clone(),
            cue_pair: Arc::clone(&pair_analysis),
            cue_index: index,
        });
    }
    Ok(mappings)
}

fn container_duration_ms(analysis: &CueAudioAnalysis) -> Option<i64> {
    Some(analysis.probe.duration.as_millis() as i64)
}

/// Extract duration from a standalone audio file.
fn extract_duration_from_file(file_path: &std::path::Path) -> Option<i64> {
    let Some(path_str) = file_path.to_str() else {
        warn!(
            "Cannot probe duration for non-UTF-8 path: {}",
            file_path.display()
        );
        return None;
    };
    // probe_audio_from_path logs its own failure reason; None here means it
    // couldn't be probed, so the track lands with no duration.
    let probe = crate::audio_codec::probe_audio_from_path(path_str)?;
    Some(probe.duration.as_millis() as i64)
}

/// Probe a CUE-backed container through FFmpeg.
pub(crate) fn analyze_cue_audio(audio_path: &std::path::Path) -> Result<CueAudioAnalysis, String> {
    let path_str = audio_path
        .to_str()
        .ok_or_else(|| format!("Non-UTF-8 audio path: {:?}", audio_path))?;
    let probe = crate::audio_codec::probe_audio_from_path(path_str)
        .ok_or_else(|| format!("Failed to probe CUE audio file: {:?}", audio_path))?;
    match probe.content_type {
        crate::util::content_type::ContentType::Flac
        | crate::util::content_type::ContentType::Ape
        | crate::util::content_type::ContentType::Alac => Ok(CueAudioAnalysis { probe }),
        other => Err(format!(
            "CUE audio expects FLAC, APE, or ALAC, got {} in {:?}",
            other.display_name(),
            audio_path
        )),
    }
}

fn map_tracks_to_individual_files(
    tracks: Vec<DbTrack>,
    audio_files: &[ScannedFile],
) -> Result<Vec<TrackFile>, String> {
    if audio_files.is_empty() {
        return Err("No audio files found in discovered files".to_string());
    }
    if audio_files.len() != tracks.len() {
        return Err(format!(
            "Track count mismatch: found {} audio files but have {} tracks",
            audio_files.len(),
            tracks.len(),
        ));
    }
    // Audio order within a release is already the scan's natural sort
    // (relative_path). The mapper preserves that order when zipping to
    // DbTracks.
    let mut mappings = Vec::with_capacity(tracks.len());
    for (mut db_track, audio_file) in tracks.into_iter().zip(audio_files.iter()) {
        db_track.duration_ms = extract_duration_from_file(&audio_file.path);
        mappings.push(TrackFile::Standalone {
            db_track,
            file_path: audio_file.path.clone(),
        });
    }
    info!("Mapped {} tracks to source files", mappings.len());
    Ok(mappings)
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;
    use std::collections::HashMap;
    use std::fs;

    /// Synthetic FLAC bytes valid enough to round-trip through the CUE/FLAC
    /// import analyzer and `file_validation::is_valid_flac`.
    ///
    /// 44.1 kHz / 2-channel / 16-bit STREAMINFO declaring 1 second of audio,
    /// padded with zeros above the truncation guard's threshold (file must be
    /// at least 10% of raw PCM, so ≥ 17_640 bytes — round up to 18_000).
    fn synthetic_flac_bytes() -> Vec<u8> {
        let sample_rate: u32 = 44_100;
        let channels: u32 = 2;
        let bps: u32 = 16;
        let total_samples: u64 = 44_100;

        let mut buf = Vec::new();
        buf.extend_from_slice(b"fLaC");

        // STREAMINFO block header: last-block=1, type=0, length=34.
        buf.push(0x80);
        buf.push(0x00);
        buf.push(0x00);
        buf.push(34);

        // STREAMINFO data: 34 bytes laid out as
        //   [0..2]   min block size
        //   [2..4]   max block size
        //   [4..7]   min frame size
        //   [7..10]  max frame size
        //   [10..13] sample rate (20 bits) | channels-1 (3) | bps-1 high bit
        //   [13]     bps-1 low 4 bits | total_samples high 4 bits
        //   [14..18] total_samples low 32 bits
        //   [18..34] MD5 signature
        buf.extend_from_slice(&[0x10, 0x00, 0x10, 0x00]); // 4096 / 4096
        buf.extend_from_slice(&[0u8; 6]); // min/max frame size unknown

        let ch_minus_1 = (channels - 1) & 0x07;
        let bps_minus_1 = (bps - 1) & 0x1F;
        let ts_high = ((total_samples >> 32) & 0x0F) as u8;

        buf.push((sample_rate >> 12) as u8);
        buf.push(((sample_rate >> 4) & 0xFF) as u8);
        buf.push(
            (((sample_rate & 0x0F) as u8) << 4)
                | ((ch_minus_1 as u8) << 1)
                | ((bps_minus_1 >> 4) as u8),
        );
        buf.push((((bps_minus_1 & 0x0F) as u8) << 4) | ts_high);
        buf.extend_from_slice(&((total_samples & 0xFFFF_FFFF) as u32).to_be_bytes());
        buf.extend_from_slice(&[0u8; 16]); // MD5

        debug_assert_eq!(buf.len(), 42);

        // Pad above the truncation guard's threshold for 1 s of 16-bit stereo
        // (raw = 44_100 * 2 * 2 = 176_400 bytes → ≥ 17_640 required).
        buf.resize(18_000, 0);
        buf
    }
    fn create_test_tracks(count: usize) -> Vec<DbTrack> {
        (0..count)
            .map(|i| {
                let now = Utc::now();
                DbTrack {
                    id: format!("track-{}", i),
                    release_id: "release-1".to_string(),
                    title: format!("Track {}", i + 1),
                    side: 1,
                    track_number: Some((i + 1) as i32),
                    duration_ms: None,
                    discogs_position: Some((i + 1).to_string()),
                    created_at: now,
                }
            })
            .collect()
    }
    fn create_test_tracks_for_disc(disc: i32, count: usize) -> Vec<DbTrack> {
        (0..count)
            .map(|i| {
                let now = Utc::now();
                DbTrack {
                    id: format!("track-d{}-{}", disc, i),
                    release_id: "release-1".to_string(),
                    title: format!("Disc {} Track {}", disc, i + 1),
                    side: disc,
                    track_number: Some((i + 1) as i32),
                    duration_ms: None,
                    discogs_position: Some(format!("{}-{}", disc, i + 1)),
                    created_at: now,
                }
            })
            .collect()
    }
    fn make_cue_sheet(disc_label: &str, track_count: usize) -> String {
        let mut s = String::from("PERFORMER \"Test Artist\"\n");
        s.push_str(&format!("TITLE \"{}\"\n", disc_label));
        s.push_str("FILE \"CDImage.flac\" WAVE\n");
        for i in 0..track_count {
            let track_num = i + 1;
            let minute = i * 3;
            s.push_str(&format!("  TRACK {:02} AUDIO\n", track_num));
            s.push_str(&format!("    TITLE \"Track {}\"\n", track_num));
            s.push_str(&format!("    INDEX 01 {:02}:00:00\n", minute));
        }
        s
    }
    use crate::import::folder_scanner::{
        collect_release_candidate_files, AudioContent, CategorizedFiles, ScannedFile,
    };
    use std::path::PathBuf;

    fn scanned(path: &str) -> ScannedFile {
        ScannedFile::new(
            PathBuf::from(path),
            path.trim_start_matches('/').to_string(),
            1024 * 1024,
        )
    }

    /// Build a `CategorizedFiles` carrying per-track audio and no CUE/FLAC
    /// pair — i.e. the `AudioContent::TrackFiles` branch. Used by tests that
    /// don't touch the filesystem.
    fn categorized_track_files(paths: Vec<&str>, format_label: &str) -> CategorizedFiles {
        CategorizedFiles {
            audio: AudioContent::TrackFiles {
                tracks: paths.into_iter().map(scanned).collect(),
                format_label: format_label.to_string(),
            },
            artwork: Vec::new(),
            documents: Vec::new(),
            unpaired_cue_sheets: Vec::new(),
        }
    }
    #[test]
    fn test_map_tracks_to_files_individual_files() {
        let tracks = create_test_tracks(3);
        let files = categorized_track_files(
            vec![
                "/album/01-track1.flac",
                "/album/02-track2.flac",
                "/album/03-track3.flac",
            ],
            "FLAC",
        );
        let mappings = map_tracks_to_files(tracks, &files).expect("mapping should succeed");
        assert_eq!(mappings.len(), 3);
        assert_eq!(mappings[0].db_track().id, "track-0");
        assert_eq!(
            mappings[0].file_path(),
            PathBuf::from("/album/01-track1.flac").as_path()
        );
        assert_eq!(mappings[1].db_track().id, "track-1");
        assert_eq!(
            mappings[1].file_path(),
            PathBuf::from("/album/02-track2.flac").as_path()
        );
        assert_eq!(mappings[2].db_track().id, "track-2");
        assert_eq!(
            mappings[2].file_path(),
            PathBuf::from("/album/03-track3.flac").as_path()
        );
        assert!(matches!(mappings[0], TrackFile::Standalone { .. }));
    }
    #[test]
    fn test_map_tracks_to_files_no_audio_files() {
        let tracks = create_test_tracks(2);
        let files = categorized_track_files(Vec::new(), "FLAC");
        let result = map_tracks_to_files(tracks, &files);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("No audio files found"));
    }
    #[test]
    fn test_map_tracks_to_files_more_tracks_than_files() {
        let tracks = create_test_tracks(5);
        let files = categorized_track_files(
            vec!["/album/01.flac", "/album/02.flac", "/album/03.flac"],
            "FLAC",
        );
        let result = map_tracks_to_files(tracks, &files);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Track count mismatch"));
    }
    #[test]
    fn test_map_tracks_to_files_more_files_than_tracks() {
        let tracks = create_test_tracks(2);
        let files = categorized_track_files(
            vec![
                "/album/01.flac",
                "/album/02.flac",
                "/album/03.flac",
                "/album/04.flac",
            ],
            "FLAC",
        );
        let result = map_tracks_to_files(tracks, &files);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Track count mismatch"));
    }
    #[test]
    fn test_map_tracks_to_files_multi_disc_cue_flac() {
        let tmp = tempfile::TempDir::new().expect("tempdir");
        let cd1_dir = tmp.path().join("CD1");
        let cd2_dir = tmp.path().join("CD2");
        fs::create_dir_all(&cd1_dir).expect("mkdir CD1");
        fs::create_dir_all(&cd2_dir).expect("mkdir CD2");
        let cd1_cue = cd1_dir.join("CDImage.cue");
        let cd1_flac = cd1_dir.join("CDImage.flac");
        let cd2_cue = cd2_dir.join("CDImage.cue");
        let cd2_flac = cd2_dir.join("CDImage.flac");
        fs::write(&cd1_cue, make_cue_sheet("Disc 1", 8)).expect("write cd1 cue");
        fs::write(&cd1_flac, synthetic_flac_bytes()).expect("write cd1 flac");
        fs::write(&cd2_cue, make_cue_sheet("Disc 2", 8)).expect("write cd2 cue");
        fs::write(&cd2_flac, synthetic_flac_bytes()).expect("write cd2 flac");
        let mut tracks = create_test_tracks_for_disc(1, 8);
        tracks.extend(create_test_tracks_for_disc(2, 8));
        let files = collect_release_candidate_files(tmp.path()).expect("scan should succeed");
        let track_files = map_tracks_to_files(tracks, &files)
            .expect("multi-disc CUE/FLAC mapping should succeed");
        assert_eq!(track_files.len(), 16);
        let mapped: HashMap<String, PathBuf> = track_files
            .iter()
            .map(|tf| (tf.db_track().id.clone(), tf.file_path().to_path_buf()))
            .collect();
        for i in 0..8 {
            let id = format!("track-d1-{}", i);
            assert_eq!(
                mapped.get(&id),
                Some(&cd1_flac),
                "disc 1 track {} should map to CD1/CDImage.flac",
                i,
            );
        }
        for i in 0..8 {
            let id = format!("track-d2-{}", i);
            assert_eq!(
                mapped.get(&id),
                Some(&cd2_flac),
                "disc 2 track {} should map to CD2/CDImage.flac",
                i,
            );
        }
        // Every CUE-backed track carries a shared analysis and its own cue_index.
        let mut d1_indices: Vec<usize> = track_files
            .iter()
            .filter_map(|tf| match tf {
                TrackFile::CueBacked {
                    file_path,
                    cue_index,
                    ..
                } if file_path == &cd1_flac => Some(*cue_index),
                _ => None,
            })
            .collect();
        d1_indices.sort();
        assert_eq!(d1_indices, (0..8).collect::<Vec<_>>());
        // Durations are populated at construction. Non-final tracks get the
        // gap to the next CUE INDEX 01 (180_000 ms with 3-minute gaps); the
        // final track falls through to `container_duration - start`, which
        // is negative against this 1 s synthetic FLAC but still `Some(_)`.
        // The mapping contract is "every track gets a duration," not a
        // specific value — that's what we assert.
        for tf in &track_files {
            assert!(
                tf.db_track().duration_ms.is_some(),
                "CUE-backed track '{}' should have a computed duration",
                tf.db_track().title,
            );
        }
    }
    #[tokio::test]
    async fn test_map_tracks_to_files_ten_disc_cue_flac_natural_sort() {
        let tmp = tempfile::TempDir::new().expect("tempdir");
        let mut disc_flacs = Vec::new();
        let mut tracks: Vec<DbTrack> = Vec::new();
        for disc in 1..=10i32 {
            let dir = tmp.path().join(format!("CD{}", disc));
            fs::create_dir_all(&dir).expect("mkdir");
            let cue = dir.join("CDImage.cue");
            let flac = dir.join("CDImage.flac");
            let track_count = disc as usize;
            fs::write(&cue, make_cue_sheet(&format!("Disc {}", disc), track_count))
                .expect("write cue");
            fs::write(&flac, synthetic_flac_bytes()).expect("write flac");
            tracks.extend(create_test_tracks_for_disc(disc, track_count));
            disc_flacs.push(flac);
        }
        let total_tracks: usize = (1..=10).sum();
        let files = collect_release_candidate_files(tmp.path()).expect("scan should succeed");
        let track_files = map_tracks_to_files(tracks, &files)
            .expect("10-disc CUE/FLAC mapping should succeed with natural sort");
        assert_eq!(track_files.len(), total_tracks);
        let mapped: HashMap<String, PathBuf> = track_files
            .iter()
            .map(|tf| (tf.db_track().id.clone(), tf.file_path().to_path_buf()))
            .collect();
        for (i, expected_flac) in disc_flacs.iter().enumerate() {
            let disc = (i + 1) as i32;
            for j in 0..(disc as usize) {
                let id = format!("track-d{}-{}", disc, j);
                assert_eq!(
                    mapped.get(&id),
                    Some(expected_flac),
                    "disc {} track {} should map to CD{}/CDImage.flac",
                    disc,
                    j,
                    disc,
                );
            }
        }
    }

    /// `CueAudioAnalysis` carries one FFmpeg probe for every CUE-backed
    /// container, and `container_duration_ms` reads its duration directly.
    #[test]
    fn test_cue_audio_analysis_probe_duration() {
        let analysis = CueAudioAnalysis {
            probe: crate::audio_codec::ProbeResult {
                content_type: crate::util::content_type::ContentType::Alac,
                duration: std::time::Duration::from_millis(100),
                sample_rate: 44100,
                channels: 2,
                bits_per_sample: Some(16),
            },
        };

        assert_eq!(container_duration_ms(&analysis), Some(100));
    }

    /// A 2xLP vinyl ripped as a single continuous CUE+FLAC pair. The release's
    /// tracklist spans four sides (A/B/C/D, -> `side` values 1..=4) but the
    /// rip is one pair. This is a legitimate vinyl rip shape that
    /// `map_tracks_to_files` must accept.
    #[test]
    fn test_map_tracks_to_files_cue_flac_multi_side_single_pair() {
        let tmp = tempfile::TempDir::new().expect("tempdir");
        // The CUE's FILE directive references "CDImage.flac" (see
        // `make_cue_sheet`), so the on-disk audio must use that name for
        // pair detection to bind them.
        let cue = tmp.path().join("CDImage.cue");
        let flac = tmp.path().join("CDImage.flac");
        fs::write(&cue, make_cue_sheet("Album Title", 9)).expect("write cue");
        fs::write(&flac, synthetic_flac_bytes()).expect("write flac");

        // Mirrors a typical 2xLP Discogs/MusicBrainz tracklist: two tracks on
        // side A, two on B, three on C, two on D.
        let side_plan = [1, 1, 2, 2, 3, 3, 3, 4, 4];
        let now = Utc::now();
        let tracks: Vec<DbTrack> = side_plan
            .iter()
            .enumerate()
            .map(|(i, &side)| DbTrack {
                id: format!("track-{}", i),
                release_id: "release-1".to_string(),
                title: format!("Track {}", i + 1),
                side,
                track_number: Some((i + 1) as i32),
                duration_ms: None,
                discogs_position: None,
                created_at: now,
            })
            .collect();

        let files = collect_release_candidate_files(tmp.path()).expect("scan should succeed");

        let track_files = map_tracks_to_files(tracks, &files)
            .expect("single-pair rip of a multi-side vinyl should map successfully");
        assert_eq!(track_files.len(), 9);
        for tf in &track_files {
            match tf {
                TrackFile::CueBacked { file_path, .. } => assert_eq!(file_path, &flac),
                _ => panic!("expected TrackFile::CueBacked"),
            }
        }
    }

    /// Guard against a `CategorizedFiles` with an unparsed CUE reaching the
    /// mapper: without the parsed sheet the mapper can't align tracks, so it
    /// must error rather than proceed.
    #[test]
    fn test_map_tracks_to_files_unparsed_cue_errors() {
        let tracks = create_test_tracks(3);
        let files = CategorizedFiles {
            audio: AudioContent::CueFlacPairs {
                pairs: vec![crate::import::folder_scanner::ScannedCueFlacPair {
                    cue_file: scanned("/album/Album.cue"),
                    audio_file: scanned("/album/Album.flac"),
                    cue_sheet: None,
                    total_size: 2048,
                }],
                format_label: "CUE+FLAC".to_string(),
            },
            artwork: Vec::new(),
            documents: Vec::new(),
            unpaired_cue_sheets: Vec::new(),
        };
        let result = map_tracks_to_files(tracks, &files);
        assert!(result.is_err());
        assert!(
            result.unwrap_err().contains("CUE sheet not parsed"),
            "Expected unparsed-CUE error",
        );
    }
}
