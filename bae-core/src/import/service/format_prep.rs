//! Probe-and-build the audio-format and content-type records for an import.
//!
//! Resolves each discovered file's `ContentType`, derives per-track
//! `DbAudioFormat` rows (CUE-backed tracks share their image's analysis;
//! standalone files are probed), and computes CUE track byte windows.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use tracing::warn;

use crate::db::{DbAudioFormat, DbAudioSegment, DbAudioSegmentRole};
use crate::import::types::{CueAudioAnalysis, CueFlacAnalysis, TrackFile};
use crate::util::content_type::ContentType;
use crate::util::content_type_hint::ContentTypeHint;

use super::ImportService;

fn admitted_audio_content_type(content_type: &ContentType) -> bool {
    matches!(
        content_type,
        ContentType::Flac
            | ContentType::Mp3
            | ContentType::Ape
            | ContentType::Alac
            | ContentType::Aac
            | ContentType::Pcm
            | ContentType::Opus
            | ContentType::Vorbis
            | ContentType::WavPack
            | ContentType::Dsd
    )
}

fn ensure_probe_audio_format(
    path: &Path,
    probe: &crate::audio_codec::ProbeResult,
) -> Result<(), String> {
    if probe.sample_rate == 0 || probe.channels == 0 {
        return Err(format!(
            "unusable audio format in {}: sample_rate={}, channels={}",
            path.display(),
            probe.sample_rate,
            probe.channels
        ));
    }
    if !admitted_audio_content_type(&probe.content_type) {
        return Err(format!(
            "Unsupported audio codec {} in {}",
            probe.content_type.display_name(),
            path.display()
        ));
    }
    Ok(())
}

/// Resolve the probe-verified `ContentType` for a discovered file.
///
/// Audio files are probed (only the decoder knows which codec the container
/// holds — `.m4a` could be ALAC or AAC, and the extension alone can't tell).
/// Non-audio files get their `ContentType` derived from the extension hint:
/// that's an honest mapping for images (an extension on image bytes predicts
/// the codec), text, and PDF. Anything the hint can't classify becomes
/// `OctetStream`, which flows through the DB as-is.
pub(super) fn resolve_file_content_type(path: &Path) -> Result<ContentType, String> {
    let ext = path
        .extension()
        .and_then(|e| e.to_str())
        .ok_or_else(|| format!("File has no extension: {:?}", path))?;
    let hint = ContentTypeHint::from_extension(ext);

    if hint.is_audio() {
        let path_str = path
            .to_str()
            .ok_or_else(|| format!("Invalid path: {}", path.display()))?;
        let probe = crate::audio_codec::probe_audio_from_path(path_str)
            .ok_or_else(|| format!("Failed to probe audio file: {}", path.display()))?;
        ensure_probe_audio_format(path, &probe)?;
        return Ok(probe.content_type);
    }

    Ok(match hint {
        ContentTypeHint::Jpeg => ContentType::Jpeg,
        ContentTypeHint::Png => ContentType::Png,
        ContentTypeHint::Gif => ContentType::Gif,
        ContentTypeHint::Webp => ContentType::Webp,
        ContentTypeHint::Bmp => ContentType::Bmp,
        ContentTypeHint::Svg => ContentType::Svg,
        ContentTypeHint::PlainText => ContentType::PlainText,
        ContentTypeHint::Pdf => ContentType::Pdf,
        // Audio hints were handled above; anything else is unknown binary.
        _ => ContentType::OctetStream,
    })
}

struct BuiltAudioFormat {
    format: DbAudioFormat,
    segments: Vec<DbAudioSegment>,
}

fn cue_track_by_playable_index(
    cue_pair: &CueFlacAnalysis,
    cue_index: usize,
) -> Result<&crate::cue_flac::CueTrack, String> {
    cue_pair
        .cue_sheet
        .playable_tracks()
        .nth(cue_index)
        .ok_or_else(|| format!("CUE playable track index {cue_index} out of bounds"))
}

fn cue_file_analysis<'a>(
    cue_pair: &'a CueFlacAnalysis,
    file_reference: &str,
) -> Result<&'a CueAudioAnalysis, String> {
    cue_pair
        .audio_files
        .iter()
        .find(|file| file.file_reference == file_reference)
        .map(|file| &file.analysis)
        .ok_or_else(|| format!("CUE references unprobed audio file: {file_reference}"))
}

fn cue_file_path<'a>(
    cue_pair: &'a CueFlacAnalysis,
    file_reference: &str,
) -> Result<&'a Path, String> {
    cue_pair
        .audio_files
        .iter()
        .find(|file| file.file_reference == file_reference)
        .map(|file| file.path.as_path())
        .ok_or_else(|| format!("CUE references unmapped audio file: {file_reference}"))
}

/// Build track-level audio format metadata for a CUE-backed track.
fn cue_backed_audio_format(
    db_track_id: &str,
    cue_pair: &CueFlacAnalysis,
    cue_index: usize,
    id: String,
    now: chrono::DateTime<chrono::Utc>,
) -> Result<DbAudioFormat, String> {
    let cue_track = cue_track_by_playable_index(cue_pair, cue_index)?;
    let cue_path = cue_file_path(cue_pair, &cue_track.file_reference)?;

    let fmt = cue_analysis_format(
        cue_file_analysis(cue_pair, &cue_track.file_reference)?,
        cue_path,
    )?;
    let audio_pregap_ms = cue_track
        .pregap_duration_ms()
        .filter(|&ms| ms > 0)
        .map(|ms| ms as i64);
    let generated_pregap_ms = cue_track
        .generated_pregap_duration_ms()
        .filter(|&ms| ms > 0)
        .map(|ms| ms as i64);
    let audio_pregap_samples = cue_track
        .pregap_cue_frames
        .map(|pregap| cue_track.start_cue_frames.saturating_sub(pregap))
        .filter(|&frames| frames > 0)
        .map(|frames| (frames * fmt.sample_rate as u64 / 75) as i64);
    let generated_pregap_samples = cue_track
        .generated_pregap_frames
        .filter(|&frames| frames > 0)
        .map(|frames| (frames * fmt.sample_rate as u64 / 75) as i64);

    Ok(DbAudioFormat::new(
        db_track_id,
        fmt.content_type,
        fmt.sample_rate as i64,
        fmt.bits_per_sample,
        fmt.channels as i64,
        id,
        now,
    )
    .with_pregap(audio_pregap_ms)
    .with_generated_pregap(generated_pregap_ms)
    .with_pregap_samples(audio_pregap_samples)
    .with_generated_pregap_samples(generated_pregap_samples))
}

/// Build an audio format for a per-track file (FLAC, MP3, APE, etc.) via FFmpeg probe.
fn standalone_probed_audio_format(
    db_track_id: &str,
    file_path: &Path,
    id: String,
    now: chrono::DateTime<chrono::Utc>,
) -> Result<crate::db::DbAudioFormat, String> {
    let path_str = file_path
        .to_str()
        .ok_or_else(|| format!("Invalid path: {}", file_path.display()))?;
    let probe = crate::audio_codec::probe_audio_from_path(path_str)
        .ok_or_else(|| format!("Failed to probe audio file: {}", file_path.display()))?;
    ensure_probe_audio_format(file_path, &probe)?;

    // A per-track file is its own whole-file source; the sample window lives in
    // its `audio_format_segments` row.
    Ok(DbAudioFormat::new(
        db_track_id,
        probe.content_type,
        probe.sample_rate as i64,
        probe.bits_per_sample.map(|b| b as i64),
        probe.channels as i64,
        id,
        now,
    ))
}

/// The audio format descriptor of a CUE-backed file, read once from its analysis
/// and shared by every caller that needs any of these fields, so the per-codec
/// extraction lives in one match instead of being repeated per field.
struct CueAnalysisFormat {
    content_type: ContentType,
    sample_rate: u32,
    bits_per_sample: Option<i64>,
    channels: u32,
}

fn cue_analysis_format(
    analysis: &CueAudioAnalysis,
    path: &Path,
) -> Result<CueAnalysisFormat, String> {
    let probe = &analysis.probe;
    ensure_probe_audio_format(path, probe)?;
    Ok(CueAnalysisFormat {
        content_type: probe.content_type.clone(),
        sample_rate: probe.sample_rate,
        bits_per_sample: probe.bits_per_sample.map(|b| b as i64),
        channels: probe.channels,
    })
}

/// Byte each CUE track's audio *starts* on within the shared file: the byte the
/// demuxer lands on when seeking to that track's `start_sample`, found via
/// `seek_landing_bytes` (one open per file, no decode). `starts[i]` is track
/// `i`'s start byte, index-aligned to the CUE tracks.
///
/// This one list gives both boundaries of every track, because a CUE track's
/// byte range is `[start[i], start[i + 1])`: track `i` ends exactly where track
/// `i + 1`'s audio begins, so its read-ahead ceiling is the next track's start
/// byte. The last track has no next start and runs to EOF. The first track starts
/// at byte 0 (nothing to prefetch), so the caller stores `None` for it. `None`
/// here means the offsets couldn't be read (non-UTF-8 path or a failed seek),
/// distinct from a real result; the caller then stores no start or end byte for
/// that file's tracks and they keep the whole-file read-ahead span.
fn cue_file_landing_bytes(
    file_path: &Path,
    cue_pair: &CueFlacAnalysis,
) -> Option<HashMap<u64, u64>> {
    let analysis = cue_pair
        .audio_files
        .iter()
        .find(|file| file.path == file_path)?;
    let sample_rate = match cue_analysis_format(&analysis.analysis, file_path) {
        Ok(fmt) => fmt.sample_rate,
        Err(error) => {
            warn!("Skipping CUE byte landing for unusable audio format: {error}");
            return None;
        }
    };
    let start_samples: Vec<u64> = cue_pair
        .cue_sheet
        .playable_tracks()
        .flat_map(|track| track.indexes.iter())
        .filter(|index| index.file_reference == analysis.file_reference)
        .filter(|index| matches!(index.number, 0 | 1))
        .map(|index| index.frames * sample_rate as u64 / 75)
        .collect();
    let Some(path) = file_path.to_str() else {
        warn!("cue_track_start_bytes: non-UTF-8 path, cannot seek byte offsets: {file_path:?}");
        return None;
    };
    let landings = crate::audio_codec::seek_landing_bytes(path, &start_samples)?;
    Some(start_samples.into_iter().zip(landings).collect())
}

fn cue_segment_end_frames(
    cue_pair: &CueFlacAnalysis,
    cue_index: usize,
    file_reference: &str,
) -> Option<u64> {
    cue_pair
        .cue_sheet
        .playable_tracks()
        .skip(cue_index + 1)
        .flat_map(|track| track.indexes.iter())
        .find(|index| index.file_reference == file_reference && matches!(index.number, 0 | 1))
        .map(|index| index.frames)
}

impl ImportService {
    fn audio_segment(
        audio_format_id: &str,
        segment_index: i64,
        role: DbAudioSegmentRole,
        file_id: &str,
        start_sample: u64,
        end_sample: Option<u64>,
        start_byte: Option<u64>,
        end_byte: Option<u64>,
        ids: &dyn coven::IdProvider,
        now: chrono::DateTime<chrono::Utc>,
    ) -> DbAudioSegment {
        DbAudioSegment {
            id: ids.new_id(),
            audio_format_id: audio_format_id.to_string(),
            segment_index,
            role,
            file_id: file_id.to_string(),
            start_sample: start_sample as i64,
            end_sample: end_sample.map(|sample| sample as i64),
            start_byte: start_byte.map(|byte| byte as i64),
            end_byte: end_byte.map(|byte| byte as i64),
            created_at: now,
        }
    }

    fn cue_segment_byte(
        byte_landings_by_file: &HashMap<PathBuf, Option<HashMap<u64, u64>>>,
        file_path: &Path,
        sample: u64,
    ) -> Option<u64> {
        if sample == 0 {
            return None;
        }
        byte_landings_by_file
            .get(file_path)
            .and_then(|map| map.as_ref())
            .and_then(|map| map.get(&sample))
            .copied()
    }

    fn cue_segments(
        audio_format_id: &str,
        cue_pair: &CueFlacAnalysis,
        cue_index: usize,
        file_ids: &HashMap<PathBuf, String>,
        byte_landings_by_file: &HashMap<PathBuf, Option<HashMap<u64, u64>>>,
        ids: &dyn coven::IdProvider,
        now: chrono::DateTime<chrono::Utc>,
    ) -> Result<Vec<DbAudioSegment>, String> {
        let cue_track = cue_track_by_playable_index(cue_pair, cue_index)?;
        let main_path = cue_file_path(cue_pair, &cue_track.file_reference)?;
        let fmt = cue_analysis_format(
            cue_file_analysis(cue_pair, &cue_track.file_reference)?,
            main_path,
        )?;
        let sample_rate = fmt.sample_rate as u64;
        let mut segments = Vec::new();

        if let crate::cue_flac::CuePregap::Audio(index) = &cue_track.pregap {
            let pregap_path = cue_file_path(cue_pair, &index.file_reference)?;
            let pregap_file_id = file_ids.get(pregap_path).ok_or_else(|| {
                format!("No DbFile registered for CUE pregap source {pregap_path:?}")
            })?;
            let start_sample = index.frames * sample_rate / 75;
            let end_frames = if index.file_reference == cue_track.file_reference {
                Some(cue_track.start_cue_frames)
            } else {
                cue_segment_end_frames(cue_pair, cue_index, &index.file_reference)
            };
            let end_sample = end_frames.map(|frames| frames * sample_rate / 75);
            let start_byte =
                Self::cue_segment_byte(byte_landings_by_file, pregap_path, start_sample);
            let end_byte = end_sample.and_then(|sample| {
                Self::cue_segment_byte(byte_landings_by_file, pregap_path, sample)
            });
            segments.push(Self::audio_segment(
                audio_format_id,
                segments.len() as i64,
                DbAudioSegmentRole::AudioPregap,
                pregap_file_id,
                start_sample,
                end_sample,
                start_byte,
                end_byte,
                ids,
                now,
            ));
        }

        let main_file_id = file_ids
            .get(main_path)
            .ok_or_else(|| format!("No DbFile registered for CUE source {main_path:?}"))?;
        let start_sample = cue_track.start_cue_frames * sample_rate / 75;
        let end_sample = cue_segment_end_frames(cue_pair, cue_index, &cue_track.file_reference)
            .map(|frames| frames * sample_rate / 75);
        let start_byte = Self::cue_segment_byte(byte_landings_by_file, main_path, start_sample);
        let end_byte = end_sample
            .and_then(|sample| Self::cue_segment_byte(byte_landings_by_file, main_path, sample));
        segments.push(Self::audio_segment(
            audio_format_id,
            segments.len() as i64,
            DbAudioSegmentRole::Main,
            main_file_id,
            start_sample,
            end_sample,
            start_byte,
            end_byte,
            ids,
            now,
        ));

        Ok(segments)
    }

    /// Build audio format records for all tracks. CUE-backed tracks already hold
    /// their shared analysis + index; standalone tracks are probed here.
    pub(super) fn build_audio_formats(
        tracks_to_files: &[TrackFile],
        file_ids: &HashMap<PathBuf, String>,
        clock: &dyn coven::Clock,
        ids: &dyn coven::IdProvider,
    ) -> Result<BuiltAudioFormats, String> {
        let now = clock.now();
        let mut audio_formats = Vec::with_capacity(tracks_to_files.len());
        let mut audio_segments = Vec::new();
        let mut cue_landings_by_file: HashMap<PathBuf, Option<HashMap<u64, u64>>> = HashMap::new();

        for track_file in tracks_to_files {
            let built = match track_file {
                TrackFile::CueBacked {
                    db_track,
                    cue_pair,
                    cue_index,
                    ..
                } => {
                    let af = cue_backed_audio_format(
                        &db_track.id,
                        cue_pair,
                        *cue_index,
                        ids.new_id(),
                        now,
                    )?;
                    for audio_file in &cue_pair.audio_files {
                        cue_landings_by_file
                            .entry(audio_file.path.clone())
                            .or_insert_with(|| cue_file_landing_bytes(&audio_file.path, cue_pair));
                    }
                    let segments = Self::cue_segments(
                        &af.id,
                        cue_pair,
                        *cue_index,
                        file_ids,
                        &cue_landings_by_file,
                        ids,
                        now,
                    )?;
                    BuiltAudioFormat {
                        format: af,
                        segments,
                    }
                }
                TrackFile::Standalone {
                    db_track,
                    file_path,
                } => {
                    let file_id = file_ids.get(file_path).ok_or_else(|| {
                        format!("No DbFile registered for track source {file_path:?}")
                    })?;
                    let af =
                        standalone_probed_audio_format(&db_track.id, file_path, ids.new_id(), now)?;
                    let segment = Self::audio_segment(
                        &af.id,
                        0,
                        DbAudioSegmentRole::Main,
                        file_id,
                        0,
                        None,
                        None,
                        None,
                        ids,
                        now,
                    );
                    BuiltAudioFormat {
                        format: af,
                        segments: vec![segment],
                    }
                }
            };

            audio_segments.extend(built.segments);
            audio_formats.push(built.format);
        }

        Ok(BuiltAudioFormats {
            audio_formats,
            audio_segments,
        })
    }
}

pub(super) struct BuiltAudioFormats {
    pub audio_formats: Vec<DbAudioFormat>,
    pub audio_segments: Vec<DbAudioSegment>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::audio_codec::ProbeResult;
    use crate::import::types::{
        CueAnalyzedAudioFile, CueAudioAnalysis, CueFlacAnalysis, TrackFile,
    };
    use std::sync::Arc;

    #[test]
    fn admitted_audio_content_type_gates_unlisted_codecs() {
        // Dispatch check: an admitted codec passes the gate.
        assert!(admitted_audio_content_type(&ContentType::Flac));
        // The gate's job — reject anything not on the import allowlist.
        assert!(!admitted_audio_content_type(&ContentType::Other(
            "codec:AV_CODEC_ID_SPEEX".to_string()
        )));
        assert!(!admitted_audio_content_type(&ContentType::Other(
            "audio/x-ms-wma".to_string()
        )));
    }

    #[test]
    fn resolve_file_content_type_maps_non_audio_by_extension() {
        // Non-audio files map straight from the extension hint — no probe, so
        // the paths need not exist.
        assert_eq!(
            resolve_file_content_type(Path::new("/x/cover.jpg")).unwrap(),
            ContentType::Jpeg
        );
        assert_eq!(
            resolve_file_content_type(Path::new("/x/back.png")).unwrap(),
            ContentType::Png
        );
        assert_eq!(
            resolve_file_content_type(Path::new("/x/notes.txt")).unwrap(),
            ContentType::PlainText
        );
        assert_eq!(
            resolve_file_content_type(Path::new("/x/booklet.pdf")).unwrap(),
            ContentType::Pdf
        );
        // An extension the hint can't classify becomes opaque binary.
        assert_eq!(
            resolve_file_content_type(Path::new("/x/data.bin")).unwrap(),
            ContentType::OctetStream
        );
    }

    #[test]
    fn resolve_file_content_type_errors_without_extension() {
        let error = resolve_file_content_type(Path::new("/x/README")).unwrap_err();
        assert!(error.contains("no extension"), "got: {error}");
    }

    fn test_clock() -> coven::FixedClock {
        coven::FixedClock(
            chrono::DateTime::parse_from_rfc3339("2024-01-01T00:00:00Z")
                .unwrap()
                .with_timezone(&chrono::Utc),
        )
    }

    /// A CUE track whose INDEX 00 sits after the previous track's INDEX 01 is a
    /// real audio pregap (a hidden intro / count-in), so the pregap bytes come
    /// from the container itself. `cue_segments` must emit an `AudioPregap`
    /// segment spanning `[INDEX 00, INDEX 01)` ahead of the `Main` segment —
    /// the branch the existing segment test never reaches, because its INDEX 00
    /// values are all bogus (before the prior track) and get rejected to
    /// `CuePregap::None`.
    #[test]
    fn cue_backed_segments_emit_audio_pregap_window() {
        let temp = tempfile::tempdir().expect("tempdir");
        let cue_path = temp.path().join("Album.cue");
        let audio_path = temp.path().join("test.ape");
        std::fs::write(
            &cue_path,
            r#"PERFORMER "Artist Name"
TITLE "Album Title"
FILE "test.ape" WAVE
  TRACK 01 AUDIO
    TITLE "Track One"
    INDEX 01 00:00:00
  TRACK 02 AUDIO
    TITLE "Track Two"
    INDEX 00 04:00:00
    INDEX 01 05:00:00
  TRACK 03 AUDIO
    TITLE "Track Three"
    INDEX 01 10:00:00
"#,
        )
        .expect("write cue");

        let cue_sheet =
            crate::cue_flac::CueFlacProcessor::parse_cue_sheet(&cue_path).expect("parse cue");
        let sample_rate = 44_100u64;
        let cue_pair = Arc::new(CueFlacAnalysis {
            cue_sheet,
            audio_files: vec![CueAnalyzedAudioFile {
                file_reference: "test.ape".to_string(),
                path: audio_path.clone(),
                analysis: CueAudioAnalysis {
                    probe: ProbeResult {
                        content_type: ContentType::Ape,
                        duration: std::time::Duration::from_secs(12 * 60),
                        sample_rate: sample_rate as u32,
                        bits_per_sample: Some(16),
                        channels: 2,
                    },
                },
            }],
        });

        let tracks_to_files: Vec<_> = cue_pair
            .cue_sheet
            .playable_tracks()
            .enumerate()
            .map(|(index, track)| TrackFile::CueBacked {
                db_track: crate::db::DbTrack::new_test(
                    "release-id",
                    &format!("track-{index}"),
                    track.title.as_deref().unwrap_or("Track Title"),
                    Some(track.number as i32),
                ),
                file_path: audio_path.clone(),
                cue_pair: Arc::clone(&cue_pair),
                cue_index: index,
            })
            .collect();
        let file_ids = HashMap::from([(audio_path, "file-id".to_string())]);
        let ids = coven::SequentialIdProvider::new("audio");

        let built =
            ImportService::build_audio_formats(&tracks_to_files, &file_ids, &test_clock(), &ids)
                .expect("build audio formats");

        // Exactly one track (track 2) has a real audio pregap.
        let pregaps: Vec<_> = built
            .audio_segments
            .iter()
            .filter(|segment| segment.role == DbAudioSegmentRole::AudioPregap)
            .collect();
        assert_eq!(pregaps.len(), 1, "only track 2 has an audio pregap");
        let pregap = pregaps[0];
        // INDEX 00 (4:00) .. INDEX 01 (5:00), converted frames -> samples.
        assert_eq!(pregap.segment_index, 0, "pregap precedes the main segment");
        assert_eq!(pregap.file_id, "file-id");
        assert_eq!(pregap.start_sample, (4 * 60 * 75) * sample_rate as i64 / 75);
        assert_eq!(
            pregap.end_sample,
            Some((5 * 60 * 75) * sample_rate as i64 / 75)
        );

        // Track 2's main segment starts where the pregap ends (INDEX 01) and
        // runs to track 3's INDEX 01.
        let main = built
            .audio_segments
            .iter()
            .find(|segment| {
                segment.audio_format_id == pregap.audio_format_id
                    && segment.role == DbAudioSegmentRole::Main
            })
            .expect("track 2 main segment");
        assert_eq!(main.start_sample, (5 * 60 * 75) * sample_rate as i64 / 75);
        assert_eq!(
            main.end_sample,
            Some((10 * 60 * 75) * sample_rate as i64 / 75)
        );
    }

    /// The standalone (non-CUE) arm probes the file for its format and emits one
    /// whole-file `Main` segment: sample window open (`start_sample` 0,
    /// `end_sample` None) and no byte window, since a per-track file is its own
    /// source.
    #[test]
    fn build_audio_formats_probes_standalone_track() {
        crate::audio_codec::init();
        let path = PathBuf::from(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/fixtures/flac/01 Test Track 1.flac"
        ));
        let track = TrackFile::Standalone {
            db_track: crate::db::DbTrack::new_test("release-id", "track-0", "Track Title", Some(1)),
            file_path: path.clone(),
        };
        let file_ids = HashMap::from([(path, "file-0".to_string())]);
        let ids = coven::SequentialIdProvider::new("af");

        let built = ImportService::build_audio_formats(&[track], &file_ids, &test_clock(), &ids)
            .expect("build audio formats");

        assert_eq!(built.audio_formats.len(), 1);
        assert_eq!(built.audio_formats[0].track_id, "track-0");
        assert_eq!(built.audio_formats[0].content_type, ContentType::Flac);

        assert_eq!(built.audio_segments.len(), 1);
        let segment = &built.audio_segments[0];
        assert_eq!(segment.role, DbAudioSegmentRole::Main);
        assert_eq!(segment.segment_index, 0);
        assert_eq!(segment.file_id, "file-0");
        assert_eq!(segment.start_sample, 0);
        assert_eq!(segment.end_sample, None, "whole-file source has no end");
        assert_eq!(segment.start_byte, None);
        assert_eq!(segment.end_byte, None);
    }

    fn probe_result(sample_rate: u32, channels: u32) -> ProbeResult {
        ProbeResult {
            content_type: ContentType::Flac,
            duration: std::time::Duration::from_secs(1),
            sample_rate,
            bits_per_sample: Some(16),
            channels,
        }
    }

    #[test]
    fn probe_audio_format_rejects_zero_channels() {
        let path = Path::new("track.flac");

        let error = ensure_probe_audio_format(path, &probe_result(44_100, 0))
            .expect_err("zero channels should be rejected");

        assert!(error.contains("unusable audio format"));
    }

    #[test]
    fn probe_audio_format_rejects_zero_sample_rate() {
        let path = Path::new("track.flac");

        let error = ensure_probe_audio_format(path, &probe_result(0, 2))
            .expect_err("zero sample rate should be rejected");

        assert!(error.contains("unusable audio format"));
    }

    #[test]
    fn cue_backed_audio_format_rejects_zero_channels() {
        let temp = tempfile::tempdir().expect("tempdir");
        let cue_path = temp.path().join("Album.cue");
        let audio_path = temp.path().join("test.flac");
        std::fs::write(
            &cue_path,
            r#"PERFORMER "Artist Name"
TITLE "Album Title"
FILE "test.flac" WAVE
  TRACK 01 AUDIO
    TITLE "Track One"
    INDEX 01 00:00:00
"#,
        )
        .expect("write cue");
        let cue_sheet =
            crate::cue_flac::CueFlacProcessor::parse_cue_sheet(&cue_path).expect("parse cue");
        let cue_pair = CueFlacAnalysis {
            cue_sheet,
            audio_files: vec![CueAnalyzedAudioFile {
                file_reference: "test.flac".to_string(),
                path: audio_path,
                analysis: CueAudioAnalysis {
                    probe: probe_result(44_100, 0),
                },
            }],
        };
        let now = chrono::DateTime::parse_from_rfc3339("2024-01-01T00:00:00Z")
            .unwrap()
            .with_timezone(&chrono::Utc);

        let error =
            cue_backed_audio_format("track-id", &cue_pair, 0, "audio-format-id".to_string(), now)
                .expect_err("zero-channel CUE audio format should fail");

        assert!(error.contains("unusable audio format"));
    }

    #[test]
    fn cue_backed_segments_ignore_rejected_index00_boundaries() {
        let temp = tempfile::tempdir().expect("tempdir");
        let cue_path = temp.path().join("Album.cue");
        let audio_path = temp.path().join("test.ape");
        std::fs::write(
            &cue_path,
            r#"PERFORMER "Artist Name"
TITLE "Album Title"
FILE "test.ape" WAVE
  TRACK 01 AUDIO
    TITLE "Track One"
    INDEX 00 00:00:00
    INDEX 01 00:00:32
  TRACK 02 AUDIO
    TITLE "Track Two"
    INDEX 01 05:05:00
  TRACK 03 AUDIO
    TITLE "Track Three"
    INDEX 00 05:05:00
    INDEX 01 08:31:20
  TRACK 04 AUDIO
    TITLE "Track Four"
    INDEX 00 05:05:00
    INDEX 01 11:01:30
"#,
        )
        .expect("write cue");

        let cue_sheet =
            crate::cue_flac::CueFlacProcessor::parse_cue_sheet(&cue_path).expect("parse cue");
        let cue_pair = Arc::new(CueFlacAnalysis {
            cue_sheet,
            audio_files: vec![CueAnalyzedAudioFile {
                file_reference: "test.ape".to_string(),
                path: audio_path.clone(),
                analysis: CueAudioAnalysis {
                    probe: ProbeResult {
                        content_type: ContentType::Ape,
                        duration: std::time::Duration::from_secs(12 * 60),
                        sample_rate: 75,
                        bits_per_sample: Some(16),
                        channels: 2,
                    },
                },
            }],
        });

        let tracks_to_files: Vec<_> = cue_pair
            .cue_sheet
            .playable_tracks()
            .enumerate()
            .map(|(index, track)| TrackFile::CueBacked {
                db_track: crate::db::DbTrack::new_test(
                    "release-id",
                    &format!("track-{index}"),
                    track.title.as_deref().unwrap_or("Track Title"),
                    Some(track.number as i32),
                ),
                file_path: audio_path.clone(),
                cue_pair: Arc::clone(&cue_pair),
                cue_index: index,
            })
            .collect();
        let file_ids = HashMap::from([(audio_path, "file-id".to_string())]);
        let clock = coven::FixedClock(
            chrono::DateTime::parse_from_rfc3339("2024-01-01T00:00:00Z")
                .unwrap()
                .with_timezone(&chrono::Utc),
        );
        let ids = coven::SequentialIdProvider::new("audio");

        let built = ImportService::build_audio_formats(&tracks_to_files, &file_ids, &clock, &ids)
            .expect("build audio formats");
        let main_segments: Vec<_> = built
            .audio_segments
            .iter()
            .filter(|segment| segment.role == DbAudioSegmentRole::Main)
            .collect();

        assert_eq!(main_segments.len(), 4);
        for segment in &main_segments {
            assert!(
                segment
                    .end_sample
                    .is_none_or(|end_sample| end_sample > segment.start_sample),
                "main segment must have a positive sample window: {segment:?}",
            );
        }
        assert_eq!(main_segments[1].end_sample, Some((8 * 60 + 31) * 75 + 20),);
        assert_eq!(main_segments[2].end_sample, Some((11 * 60 + 1) * 75 + 30),);
    }
}
