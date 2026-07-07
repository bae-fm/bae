//! Loudness measurement for an import's tracks.
//!
//! Decodes each track's sample window on a blocking thread, streams it through
//! an EBU R128 meter, and reports the per-track loudness/true-peak plus the
//! album aggregate. Progress ticks ride the import event channel passed in by
//! the service, so the analyzer needs no reference back to it.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

use tokio::sync::broadcast;
use tracing::{debug, warn};

use crate::import::handle::send_event;
use crate::import::types::TrackFile;

/// The album loudness/peak plus the tracks whose decode looked broken (import
/// decode-verify). `broken` carries a human-readable line per broken track (its
/// path, track number, and reason); the caller fails the import on it when
/// `verify_decode_on_import` is on.
pub(super) struct LoudnessResult {
    pub album_loudness_lufs: Option<f64>,
    pub album_peak_linear: Option<f64>,
    pub broken: Vec<String>,
}

/// One track's decode outcome from the blocking measure task: the measurement
/// (absent when unmeasured/silent/failed) and the broken-decode reason (absent
/// when the decode produced the full window cleanly).
struct TrackOutcome {
    measured: Option<(ebur128::EbuR128, f64, f64)>,
    broken: Option<String>,
}

/// The meter and the format-derived constants it needs, set together once the
/// decode probes the format. Held as one `Option` so "no format yet" is a single
/// absence, not several fields each separately nullable.
struct MeterState {
    meter: crate::loudness::LoudnessMeter,
    channels: u32,
    /// Emit a progress tick once this many frames have been measured since the
    /// last one (~0.1s of audio), so the bar creeps without an event per frame.
    emit_every_frames: u64,
}

/// Streams one track's decode into a [`crate::loudness::LoudnessMeter`] and emits
/// `ImportLoudnessProgress` as the scan advances, throttled to ~0.1s of audio per
/// tick. A failed `add_chunk` is recorded and the meter dropped (later chunks are
/// ignored); `into_result` surfaces the failure to the caller.
struct LoudnessProgressSink {
    state: Option<MeterState>,
    error: Option<String>,
    /// This track's total frame count, used to fill its bar segment. `None` when
    /// neither the sample window nor a track duration is known — the segment then
    /// only advances at the post-track tick.
    total_frames: Option<u64>,
    done_frames: u64,
    /// Fatal FFmpeg errors reported by the decoder after the stream ends (0 for a
    /// clean decode). Set once via `set_decode_error_count`; a non-zero count
    /// flags the track as broken for import decode-verify.
    decode_error_count: u32,
    frames_since_emit: u64,
    event_tx: broadcast::Sender<crate::import::handle::ImportEvent>,
    candidate_key: String,
    idx: u32,
    tracks_total: u32,
}

impl LoudnessProgressSink {
    /// Overall scan `fraction` (0..1): this track is the `idx`-th of
    /// `tracks_total` equal segments, filled by `done_frames / total_frames`.
    fn emit(&self) {
        let within = match self.total_frames {
            Some(total) if total > 0 => (self.done_frames as f32 / total as f32).min(1.0),
            _ => 0.0,
        };
        // `within` is already clamped to 0..1 above and `idx < tracks_total`, so
        // `fraction` is in 0..1 by construction — consumers render it as-is.
        let fraction = (self.idx as f32 + within) / self.tracks_total as f32;
        send_event(
            &self.event_tx,
            crate::import::handle::ImportEvent::ImportLoudnessProgress {
                candidate_key: self.candidate_key.clone(),
                tracks_done: self.idx,
                tracks_total: self.tracks_total,
                fraction,
            },
        );
    }

    /// Why this track's decode looks broken, if it does: a fatal FFmpeg error, or
    /// output frames far short of the expected window (a valid header over a
    /// truncated body — the decode stops early). `None` = the decode produced the
    /// full window cleanly. The caller asserts on this only when
    /// `verify_decode_on_import` is on; otherwise it's advisory (logged).
    fn broken_reason(&self) -> Option<String> {
        if self.decode_error_count > 0 {
            return Some(format!("{} fatal decode error(s)", self.decode_error_count));
        }
        // A duration-derived `total_frames` (a whole-file / last track with no
        // end_sample) is approximate, so require a gross shortfall: a truncated
        // body decodes a fraction of the window, far past this 5% slack, while
        // boundary rounding on the expected count stays well under it.
        if let Some(total) = self.total_frames {
            let complete_floor = total - total / 20;
            if total > 0 && self.done_frames < complete_floor {
                return Some(format!(
                    "decoded {} of {} expected frames (truncated body)",
                    self.done_frames, total
                ));
            }
        }
        None
    }

    /// Finish the meter, surfacing any stored decode/measure failure.
    fn into_result(
        self,
    ) -> Result<(ebur128::EbuR128, Option<crate::loudness::TrackLoudness>), String> {
        if let Some(e) = self.error {
            return Err(e);
        }
        let state = self
            .state
            .ok_or_else(|| "decode produced no audio format".to_string())?;
        state.meter.finish()
    }
}

impl crate::audio_codec::DecodedSink for LoudnessProgressSink {
    fn on_format(&mut self, sample_rate: u32, channels: u32) {
        match crate::loudness::LoudnessMeter::new(channels, sample_rate) {
            Ok(meter) => {
                self.state = Some(MeterState {
                    meter,
                    channels,
                    emit_every_frames: (sample_rate as u64 / 10).max(1),
                })
            }
            Err(e) => self.error = Some(e),
        }
    }

    fn set_decode_error_count(&mut self, count: u32) {
        self.decode_error_count = count;
    }

    fn on_samples(&mut self, samples: &[i32]) {
        // No meter: either creation failed (`error` is set and surfaced by
        // `into_result`) or a prior chunk failed and dropped it. Either way this
        // track is already accounted for; stop feeding.
        let Some(state) = self.state.as_mut() else {
            return;
        };
        let channels = state.channels;
        let emit_every_frames = state.emit_every_frames;
        if let Err(e) = state.meter.add_chunk(samples) {
            self.error = Some(e);
            self.state = None;
            return;
        }
        let frames = (samples.len() / channels.max(1) as usize) as u64;
        self.done_frames += frames;
        self.frames_since_emit += frames;
        if self.frames_since_emit >= emit_every_frames {
            self.frames_since_emit = 0;
            self.emit();
        }
    }
}

/// Measure each track's loudness + true peak and the album's combined
/// loudness, attaching the per-track measurements to `audio_formats` and
/// returning the album-level `(loudness_lufs, peak_linear)`.
///
/// Each track's window is decoded and measured on a blocking thread (FFmpeg
/// decode is blocking CPU work); the per-file source bytes are read once and
/// shared across that file's tracks (the tracks of a CUE image). Decoding
/// per track window — not the whole image at once — bounds transient PCM
/// memory to one track at a time.
///
/// A track whose decode/measure fails, or that is too quiet to have a usable
/// loudness, keeps NULL loudness/peak and still imports; the skip is logged
/// at `warn!`/`debug!` with the file path. A measurement failure never
/// aborts the import.
///
/// Per-track progress ticks (and the sub-track ticks from the sink) are
/// published on `event_tx`, the import event channel the service owns.
///
/// The full decode also verifies each track: a fatal decode error or output
/// frames far short of the expected window (a truncated body under a valid
/// header) marks the track broken. Broken tracks are always logged and returned;
/// the caller decides whether to fail the import (per `verify_decode_on_import`).
pub(super) async fn measure_loudness(
    event_tx: &broadcast::Sender<crate::import::handle::ImportEvent>,
    audio_formats: &mut [crate::db::DbAudioFormat],
    audio_segments: &[crate::db::DbAudioSegment],
    file_ids: &HashMap<PathBuf, String>,
    tracks_to_files: &[TrackFile],
    candidate_key: &str,
) -> LoudnessResult {
    use ebur128::EbuR128;

    // Source bytes per file, read once and shared across that file's tracks
    // (every CUE track of one image points at the same bytes). An unreadable
    // file yields `None`, so its tracks are skipped (logged) rather than
    // measured against missing bytes.
    let path_by_file_id: HashMap<String, PathBuf> = file_ids
        .iter()
        .map(|(path, id)| (id.clone(), path.clone()))
        .collect();
    let mut file_bytes: HashMap<String, Option<Arc<Vec<u8>>>> = HashMap::new();
    for (file_id, path) in &path_by_file_id {
        file_bytes.entry(file_id.clone()).or_insert_with(|| {
            match std::fs::read(path) {
                Ok(bytes) => Some(Arc::new(bytes)),
                Err(e) => {
                    warn!("loudness: cannot read {path:?} to measure: {e}; its tracks stay unmeasured");
                    None
                }
            }
        });
    }

    // Every track is one unit of progress. A track whose source file was
    // unreadable gets no decode task, so count it as done up front — the bar
    // still reaches N/N even when some tracks can't be measured.
    let tracks_total = audio_formats.len() as u32;
    let mut tracks_done: u32 = 0;

    // Start tick (already counting any unreadable-file skips), then a tick per
    // ~0.1s of audio measured (from inside the blocking task) so the
    // determinate bar creeps continuously through each track's scan.
    emit_loudness_progress(event_tx, candidate_key, tracks_done, tracks_total, 0.0);

    // Decode + measure ONE track at a time: each decode runs on a blocking
    // thread (off the async worker) but is awaited before the next starts, so
    // the machine never runs N concurrent decodes — one core's worth of work,
    // and the bar advances a track per completion instead of jumping at the
    // end. `audio_formats` and `tracks_to_files` are index-aligned (the
    // formats are built from the same tracks), so `idx` keys both.
    let mut meters: Vec<EbuR128> = Vec::new();
    let mut track_peaks: Vec<f64> = Vec::new();
    // Tracks whose decode looked broken (fatal errors / truncated body). Always
    // collected; the caller fails the import on these when verify is on.
    let mut broken_tracks: Vec<String> = Vec::new();
    for (idx, tf) in tracks_to_files.iter().enumerate() {
        let format_id = audio_formats[idx].id.clone();
        let mut segments: Vec<_> = audio_segments
            .iter()
            .filter(|segment| segment.audio_format_id == format_id)
            .collect();
        segments.sort_by_key(|segment| segment.segment_index);
        if segments.is_empty() {
            warn!(
                "loudness: audio format {} has no segments; track stays unmeasured",
                format_id
            );
            tracks_done += 1;
            let fraction = tracks_done as f32 / tracks_total as f32;
            emit_loudness_progress(event_tx, candidate_key, tracks_done, tracks_total, fraction);
            continue;
        }
        // Frames in this track's window, to fill its bar segment as the decode
        // streams: the sample window when known, else the track duration ×
        // sample rate. Absent both, the segment only steps at the post-track
        // tick.
        let sample_rate = audio_formats[idx].sample_rate as u64;
        let total_frames = segments
            .iter()
            .try_fold(0u64, |total, segment| {
                segment.end_sample.map(|end| {
                    total.saturating_add((end as u64).saturating_sub(segment.start_sample as u64))
                })
            })
            .or_else(|| {
                tf.db_track()
                    .duration_ms
                    .filter(|&ms| ms > 0 && sample_rate > 0)
                    .map(|ms| ms as u64 * sample_rate / 1000)
            });
        let mut decode_segments = Vec::new();
        let mut missing_segment = false;
        for segment in &segments {
            let Some(bytes) = file_bytes.get(&segment.file_id).and_then(|b| b.clone()) else {
                warn!(
                    "loudness: cannot read segment source file {} for track {}; track stays unmeasured",
                    segment.file_id,
                    idx + 1
                );
                missing_segment = true;
                break;
            };
            decode_segments.push((
                bytes,
                segment.start_sample as u64,
                segment.end_sample.map(|sample| sample as u64),
            ));
        }
        if missing_segment {
            tracks_done += 1;
            let fraction = tracks_done as f32 / tracks_total as f32;
            emit_loudness_progress(event_tx, candidate_key, tracks_done, tracks_total, fraction);
            continue;
        }
        // Cloned into the blocking task so the sink can emit progress on the
        // import event channel directly from the worker thread.
        // `idx`/`tracks_total` place this track's scan in the bar.
        let task_event_tx = event_tx.clone();
        let key = candidate_key.to_string();
        let outcome = tokio::task::spawn_blocking(move || {
            let mut sink = LoudnessProgressSink {
                state: None,
                error: None,
                total_frames,
                done_frames: 0,
                decode_error_count: 0,
                frames_since_emit: 0,
                event_tx: task_event_tx,
                candidate_key: key,
                idx: idx as u32,
                tracks_total,
            };
            // A decode that fails outright is broken; one that returns Ok can still
            // be broken (fatal errors / a truncated body) — `broken_reason` reads
            // the error count and frame shortfall the sink captured.
            for (bytes, start_sample, end_sample) in decode_segments {
                if let Err(e) = crate::audio_codec::decode_audio_to_sink(
                    &bytes,
                    Some(start_sample),
                    end_sample,
                    &mut sink,
                ) {
                    warn!(
                        "loudness: decode failed for track {}: {e}; track stays unmeasured",
                        idx + 1
                    );
                    return TrackOutcome {
                        measured: None,
                        broken: Some(format!("decode failed: {e}")),
                    };
                }
            }
            let broken = sink.broken_reason();
            let measured = match sink.into_result() {
                Ok((meter, Some(m))) => Some((meter, m.loudness_lufs, m.peak_linear)),
                Ok((_, None)) => {
                    debug!(
                        "loudness: track {} has no usable loudness (silent); unmeasured",
                        idx + 1
                    );
                    None
                }
                Err(e) => {
                    warn!(
                        "loudness: measure failed for track {}: {e}; track stays unmeasured",
                        idx + 1
                    );
                    None
                }
            };
            TrackOutcome { measured, broken }
        })
        .await;

        match outcome {
            Ok(TrackOutcome { measured, broken }) => {
                if let Some((meter, loudness_lufs, peak_linear)) = measured {
                    audio_formats[idx].track_loudness_lufs = Some(loudness_lufs);
                    audio_formats[idx].track_peak_linear = Some(peak_linear);
                    meters.push(meter);
                    track_peaks.push(peak_linear);
                }
                if let Some(reason) = broken {
                    warn!(
                        "import verify: track source for track {} looks broken: {reason}",
                        idx + 1
                    );
                    broken_tracks.push(format!(
                        "{} (track {}): {reason}",
                        tf.db_track().title,
                        idx + 1
                    ));
                }
            }
            Err(e) => warn!("loudness: measurement task panicked: {e}; track stays unmeasured"),
        }
        tracks_done += 1;
        let fraction = tracks_done as f32 / tracks_total as f32;
        emit_loudness_progress(event_tx, candidate_key, tracks_done, tracks_total, fraction);
    }

    let album_loudness = crate::loudness::album_loudness(&meters);
    let album_peak = crate::loudness::album_peak(&track_peaks);
    LoudnessResult {
        album_loudness_lufs: album_loudness,
        album_peak_linear: album_peak,
        broken: broken_tracks,
    }
}

/// Emit a loudness-measurement tick for the candidate's confirm pane. Routed
/// to a native leaf view (not the coarse candidate row), so the sub-track
/// cadence never churns the row. `fraction` is overall scan progress (0..1)
/// for the determinate bar; `tracks_done`/`tracks_total` label which track.
fn emit_loudness_progress(
    event_tx: &broadcast::Sender<crate::import::handle::ImportEvent>,
    candidate_key: &str,
    tracks_done: u32,
    tracks_total: u32,
    fraction: f32,
) {
    send_event(
        event_tx,
        crate::import::handle::ImportEvent::ImportLoudnessProgress {
            candidate_key: candidate_key.to_string(),
            tracks_done,
            tracks_total,
            fraction,
        },
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sink_with(total: Option<u64>, done: u64, errors: u32) -> LoudnessProgressSink {
        let (event_tx, _rx) = broadcast::channel(16);
        LoudnessProgressSink {
            state: None,
            error: None,
            total_frames: total,
            done_frames: done,
            decode_error_count: errors,
            frames_since_emit: 0,
            event_tx,
            candidate_key: "test".to_string(),
            idx: 0,
            tracks_total: 1,
        }
    }

    /// The broken signature: a gross frame shortfall (a truncated body under a
    /// valid header) or any fatal decode error. A full-window decode and small
    /// boundary rounding on a duration-derived expected count are not broken.
    #[test]
    fn broken_reason_flags_shortfall_and_errors_not_clean() {
        assert!(
            sink_with(Some(1000), 1000, 0).broken_reason().is_none(),
            "a full-window decode is clean"
        );
        assert!(
            sink_with(Some(1000), 970, 0).broken_reason().is_none(),
            "a 3% shortfall is boundary rounding, within slack"
        );
        assert!(
            sink_with(Some(1000), 100, 0).broken_reason().is_some(),
            "a 90% shortfall is a truncated body"
        );
        assert!(
            sink_with(Some(1000), 1000, 1).broken_reason().is_some(),
            "a fatal decode error is broken even with the full window"
        );
        assert!(
            sink_with(None, 0, 0).broken_reason().is_none(),
            "with no expected count, a shortfall can't be asserted"
        );
    }

    /// End-to-end over the real decoder: a window that falls in a truncated
    /// FLAC's missing tail is flagged broken (the decode errors out or produces
    /// far too few frames), while the same window of the intact fixture decodes
    /// clean. Rides the same `set_decode_error_count` + frame count the import
    /// verify uses.
    #[test]
    fn truncated_flac_decode_is_flagged_broken_intact_is_not() {
        crate::audio_codec::init();
        let path = concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/fixtures/cue_flac/Test Album.flac"
        );
        let clean = std::fs::read(path).expect("read fixture");
        let sr = 44100u64;
        // A one-second window deep in the file (25s, brown noise near the end).
        let start = 25 * sr;
        let end = 26 * sr;
        let total = end - start;

        let mut sink = sink_with(Some(total), 0, 0);
        let ok =
            crate::audio_codec::decode_audio_to_sink(&clean, Some(start), Some(end), &mut sink);
        assert!(ok.is_ok(), "intact decode should succeed: {ok:?}");
        assert!(
            sink.broken_reason().is_none(),
            "intact fixture window is not broken (decoded {} of {total})",
            sink.done_frames
        );

        // Truncated to 60% of its bytes: the 25s window is past the surviving
        // audio, so the decode errors out or yields far too few frames.
        let truncated = &clean[..clean.len() * 6 / 10];
        let mut sink = sink_with(Some(total), 0, 0);
        let res =
            crate::audio_codec::decode_audio_to_sink(truncated, Some(start), Some(end), &mut sink);
        assert!(
            res.is_err() || sink.broken_reason().is_some(),
            "truncated fixture window must be flagged broken (decode {res:?}, decoded {} of {total})",
            sink.done_frames,
        );
    }

    // ── measure_loudness ───────────────────────────────────────────

    fn now() -> chrono::DateTime<chrono::Utc> {
        chrono::DateTime::parse_from_rfc3339("2024-01-01T00:00:00Z")
            .unwrap()
            .with_timezone(&chrono::Utc)
    }

    fn audio_format(track_id: &str, id: &str) -> crate::db::DbAudioFormat {
        crate::db::DbAudioFormat::new(
            track_id,
            crate::util::content_type::ContentType::Flac,
            44_100,
            Some(16),
            2,
            id.to_string(),
            now(),
        )
    }

    fn whole_file_main_segment(format_id: &str, file_id: &str) -> crate::db::DbAudioSegment {
        crate::db::DbAudioSegment {
            id: format!("seg-{format_id}"),
            audio_format_id: format_id.to_string(),
            segment_index: 0,
            role: crate::db::DbAudioSegmentRole::Main,
            file_id: file_id.to_string(),
            start_sample: 0,
            end_sample: None,
            start_byte: None,
            end_byte: None,
            created_at: now(),
        }
    }

    fn standalone_track(track_id: &str, path: &std::path::Path) -> TrackFile {
        TrackFile::Standalone {
            db_track: crate::db::DbTrack::new_test("release-id", track_id, "Track Title", Some(1)),
            file_path: path.to_path_buf(),
        }
    }

    fn cue_flac_fixture(name: &str) -> PathBuf {
        PathBuf::from(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/fixtures/cue_flac"
        ))
        .join(name)
    }

    /// A track whose source file can't be read is counted done (the bar still
    /// reaches N/N) but stays unmeasured, and never fails the import.
    #[tokio::test]
    async fn measure_loudness_skips_unreadable_source() {
        let (event_tx, _rx) = broadcast::channel(16);
        let missing = PathBuf::from("/nonexistent/track.flac");
        let mut audio_formats = vec![audio_format("track-0", "af-0")];
        let audio_segments = vec![whole_file_main_segment("af-0", "file-0")];
        let file_ids = HashMap::from([(missing.clone(), "file-0".to_string())]);
        let tracks = vec![standalone_track("track-0", &missing)];

        let result = measure_loudness(
            &event_tx,
            &mut audio_formats,
            &audio_segments,
            &file_ids,
            &tracks,
            "cand",
        )
        .await;

        assert!(result.album_loudness_lufs.is_none());
        assert!(result.album_peak_linear.is_none());
        assert!(
            result.broken.is_empty(),
            "an unreadable source is not broken"
        );
        assert!(audio_formats[0].track_loudness_lufs.is_none());
        assert!(audio_formats[0].track_peak_linear.is_none());
    }

    /// A track whose audio format has no segments is skipped (nothing to
    /// decode) and stays unmeasured.
    #[tokio::test]
    async fn measure_loudness_skips_track_with_no_segments() {
        let (event_tx, _rx) = broadcast::channel(16);
        let mut audio_formats = vec![audio_format("track-0", "af-0")];
        let audio_segments: Vec<crate::db::DbAudioSegment> = Vec::new();
        let file_ids = HashMap::new();
        let tracks = vec![standalone_track("track-0", &PathBuf::from("/unused.flac"))];

        let result = measure_loudness(
            &event_tx,
            &mut audio_formats,
            &audio_segments,
            &file_ids,
            &tracks,
            "cand",
        )
        .await;

        assert!(result.album_loudness_lufs.is_none());
        assert!(audio_formats[0].track_loudness_lufs.is_none());
    }

    /// A real, non-silent decode yields per-track loudness/peak and an album
    /// aggregate over the measured tracks.
    #[tokio::test]
    async fn measure_loudness_computes_track_and_album_values() {
        crate::audio_codec::init();
        let (event_tx, _rx) = broadcast::channel(16);
        let path = cue_flac_fixture("03 Test Artist - Track Three (Brown Noise).flac");
        let mut audio_formats = vec![audio_format("track-0", "af-0")];
        let audio_segments = vec![whole_file_main_segment("af-0", "file-0")];
        let file_ids = HashMap::from([(path.clone(), "file-0".to_string())]);
        let tracks = vec![standalone_track("track-0", &path)];

        let result = measure_loudness(
            &event_tx,
            &mut audio_formats,
            &audio_segments,
            &file_ids,
            &tracks,
            "cand",
        )
        .await;

        assert!(
            audio_formats[0].track_loudness_lufs.is_some(),
            "brown-noise track has a measurable loudness"
        );
        assert!(audio_formats[0].track_peak_linear.is_some());
        assert!(
            result.album_loudness_lufs.is_some(),
            "one measured track yields an album aggregate"
        );
        assert!(result.album_peak_linear.is_some());
    }

    /// A window shorter than one EBU R128 gated block (400 ms) produces no
    /// usable loudness — the same "unmeasured" outcome the code labels silent
    /// (`into_result` returns `Ok((_, None))`). The track keeps NULL
    /// loudness/peak and contributes nothing to the album. (The available
    /// "silence" fixture measures above the −70 LUFS floor over its full length,
    /// so a sub-gate window is the deterministic way to reach this branch with a
    /// real decode.)
    #[tokio::test]
    async fn measure_loudness_leaves_ungated_track_unmeasured() {
        crate::audio_codec::init();
        let (event_tx, _rx) = broadcast::channel(16);
        let path = cue_flac_fixture("03 Test Artist - Track Three (Brown Noise).flac");
        let mut audio_formats = vec![audio_format("track-0", "af-0")];
        // ~50 ms at 44.1 kHz — far short of a 400 ms gated block.
        let mut segment = whole_file_main_segment("af-0", "file-0");
        segment.end_sample = Some(2_205);
        let audio_segments = vec![segment];
        let file_ids = HashMap::from([(path.clone(), "file-0".to_string())]);
        let tracks = vec![standalone_track("track-0", &path)];

        let result = measure_loudness(
            &event_tx,
            &mut audio_formats,
            &audio_segments,
            &file_ids,
            &tracks,
            "cand",
        )
        .await;

        assert!(
            audio_formats[0].track_loudness_lufs.is_none(),
            "a sub-gate window has no usable loudness and stays unmeasured"
        );
        assert!(
            result.album_loudness_lufs.is_none(),
            "no measured track means no album loudness"
        );
    }
}
