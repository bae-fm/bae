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
use crate::playback::data_source::{AudioDataReader, LocalReader};
use crate::playback::sparse_buffer::create_sparse_buffer;
use crate::playback::SharedSparseBuffer;

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
/// when the decode produced enough of the expected window to be usable).
struct TrackOutcome {
    measured: Option<(ebur128::EbuR128, f64, f64)>,
    broken: Option<String>,
}

/// Where one loudness pass reports its scan: the candidate row's phase percent,
/// like every other running phase. The decode calls in every ~0.1s of audio, but
/// the percent is only sent when its whole number moves, so the row's retained
/// snapshot rebuilds at most a hundred times over the pass instead of once per
/// call.
#[derive(Clone)]
struct LoudnessProgress {
    event_tx: broadcast::Sender<crate::import::handle::ImportEvent>,
    candidate_key: String,
    release_id: String,
    import_id: String,
    /// Shared by every track's sink, so the percent is the whole candidate's and
    /// crossing a track boundary does not re-report where it already stands.
    last_percent: Arc<std::sync::atomic::AtomicU8>,
}

impl LoudnessProgress {
    fn new(
        event_tx: &broadcast::Sender<crate::import::handle::ImportEvent>,
        candidate_key: &str,
        release_id: &str,
        import_id: &str,
    ) -> Self {
        Self {
            event_tx: event_tx.clone(),
            candidate_key: candidate_key.to_string(),
            release_id: release_id.to_string(),
            import_id: import_id.to_string(),
            last_percent: Arc::new(std::sync::atomic::AtomicU8::new(0)),
        }
    }

    /// Report the pass at `fraction` of its frames. `fraction` is `None` when
    /// some track provides no frame denominator; there is then no percent to
    /// advance and the row's bar stands where it was.
    fn report(&self, fraction: Option<f32>) {
        let Some(fraction) = fraction else {
            return;
        };
        let percent = (fraction * 100.0).round().clamp(0.0, 100.0) as u8;
        if self
            .last_percent
            .swap(percent, std::sync::atomic::Ordering::Relaxed)
            == percent
        {
            return;
        }
        send_event(
            &self.event_tx,
            crate::import::handle::ImportEvent::ImportProgress {
                candidate_key: self.candidate_key.clone(),
                progress: crate::import::types::ImportProgress::Progress {
                    id: self.release_id.clone(),
                    percent,
                    phase: crate::import::types::ImportPhase::MeasuringLoudness,
                    import_id: self.import_id.clone(),
                },
            },
        );
    }
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

/// Streams one track's decode into a [`crate::loudness::LoudnessMeter`], reporting
/// the pass's progress as it advances, throttled to ~0.1s of audio per report.
/// A failed `add_chunk` is recorded and the meter dropped (later chunks are
/// ignored); `into_result` surfaces the failure to the caller.
struct LoudnessProgressSink {
    state: Option<MeterState>,
    error: Option<String>,
    /// This track's total frame count, used for decode verification and as its
    /// share of overall progress. `None` when neither the sample window nor a
    /// track duration is known; that makes overall progress indeterminate.
    total_frames: Option<u64>,
    done_frames: u64,
    /// Expected frames in tracks completed before this one, and across the
    /// whole candidate. The whole-candidate value is absent if any track's
    /// frame count cannot be established.
    frames_done_before: u64,
    scan_total_frames: Option<u64>,
    /// Fatal FFmpeg errors reported by the decoder after the stream ends (0 for a
    /// clean decode). Accumulated across every segment; a non-zero count flags
    /// the track as broken for import decode-verify.
    decode_error_count: u32,
    /// Invalid compressed packets the verifying decode discarded while
    /// continuing through the stream. They are acceptable only when the frame
    /// count proves the requested window was substantially decoded.
    discarded_packet_count: u32,
    frames_since_emit: u64,
    progress: LoudnessProgress,
}

impl LoudnessProgressSink {
    /// Overall scan `fraction` (0..1) when the current track's frame count is
    /// known. Without that denominator the progress is indeterminate.
    fn emit(&self) {
        let frames_done = match self.scan_total_frames {
            Some(_) => {
                let track_total = self
                    .total_frames
                    .expect("a known scan total requires every track total");
                let current_done = self.done_frames.min(track_total);
                self.frames_done_before.saturating_add(current_done)
            }
            None => self.frames_done_before,
        };
        let fraction = progress_fraction(frames_done, self.scan_total_frames);
        self.progress.report(fraction);
    }

    /// Why this track's decode looks broken, if it does: a fatal FFmpeg error, or
    /// output frames far short of the expected window (a valid header over a
    /// truncated body — the decode stops early). Invalid packets are acceptable
    /// only when the decoded frame count proves the window is substantially
    /// complete. Advisory unless `verify_decode_on_import` is on.
    fn broken_reason(&self) -> Option<String> {
        if self.decode_error_count > 0 {
            return Some(format!("{} fatal decode error(s)", self.decode_error_count));
        }
        // A duration-derived `total_frames` (a whole-file / last track with no
        // end_sample) is approximate, hence the 5% slack: a truncated body decodes
        // a fraction of its window, far past that, while boundary rounding on the
        // expected count stays well under it.
        if let Some(total) = self.total_frames {
            let complete_floor = total - total / 20;
            if total > 0 && self.done_frames < complete_floor {
                return Some(format!(
                    "decoded {} of {} expected frames (truncated body)",
                    self.done_frames, total
                ));
            }
        } else if self.discarded_packet_count > 0 {
            return Some(format!(
                "{} invalid packet(s) with no expected frame count",
                self.discarded_packet_count
            ));
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

fn progress_fraction(frames_done: u64, total_frames: Option<u64>) -> Option<f32> {
    total_frames.and_then(|total| (total > 0).then(|| frames_done.min(total) as f32 / total as f32))
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
        self.decode_error_count = self.decode_error_count.saturating_add(count);
    }

    fn set_discarded_packet_count(&mut self, count: u32) {
        self.discarded_packet_count = self.discarded_packet_count.saturating_add(count);
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

/// Measure each track's loudness + true peak and the album's combined loudness,
/// attaching the per-track measurements to `audio_formats` and returning the
/// album-level `(loudness_lufs, peak_linear)`.
///
/// Each track's window is decoded and measured on a blocking thread (FFmpeg
/// decode is blocking CPU work). A file's compressed bytes stream through one
/// shared `SparseStreamingBuffer` across its tracks (the tracks of a CUE
/// image); the fill keeps a window ahead of the decode and evicts behind it,
/// and decoding one track window at a time bounds transient PCM memory to a
/// single track.
///
/// A track whose decode/measure fails, or that is too quiet to have a usable
/// loudness, keeps NULL loudness/peak and still imports — the skip is logged with
/// its file path, and a measurement failure never aborts the import.
///
/// The full decode doubles as verification: a fatal decode error, or output
/// frames far short of the expected window (a truncated body under a valid
/// header), marks the track broken. Broken tracks are always logged and returned;
/// the caller decides whether to fail the import (per `verify_decode_on_import`).
pub(super) async fn measure_loudness(
    event_tx: &broadcast::Sender<crate::import::handle::ImportEvent>,
    audio_formats: &mut [crate::db::DbAudioFormat],
    audio_segments: &[crate::db::DbAudioSegment],
    file_ids: &HashMap<PathBuf, String>,
    tracks_to_files: &[TrackFile],
    candidate_key: &str,
    release_id: &str,
    import_id: &str,
) -> LoudnessResult {
    use ebur128::EbuR128;

    // Stream once per file and share across its tracks (every CUE track of one
    // image reads the same sparse buffer; the fill evicts what the decodes have
    // passed, so only a window of compressed bytes stays resident). A file that
    // can't be stat'd (or has a non-UTF-8 path the reader can't carry) yields
    // `None`, so its tracks are skipped rather than measured against missing
    // bytes; a read failure mid-stream cancels the buffer and surfaces as that
    // track's decode failure.
    let path_by_file_id: HashMap<String, PathBuf> = file_ids
        .iter()
        .map(|(path, id)| (id.clone(), path.clone()))
        .collect();
    let mut file_buffers: HashMap<String, Option<SharedSparseBuffer>> = HashMap::new();
    for (file_id, path) in &path_by_file_id {
        file_buffers.entry(file_id.clone()).or_insert_with(|| {
            let size = match std::fs::metadata(path) {
                Ok(metadata) => metadata.len(),
                Err(e) => {
                    warn!("loudness: cannot stat {path:?} to measure: {e}; its tracks stay unmeasured");
                    return None;
                }
            };
            let Some(path_str) = path.to_str() else {
                warn!("loudness: non-UTF-8 path {path:?}; its tracks stay unmeasured");
                return None;
            };
            let buffer = create_sparse_buffer(size);
            let error_path = path.clone();
            Box::new(LocalReader::new(path_str)).start_reading(
                buffer.clone(),
                Box::new(move |error| {
                    warn!("loudness: streaming {error_path:?} failed: {error}");
                }),
            );
            Some(buffer)
        });
    }

    // The bar uses actual frame work rather than equal track slices: a candidate
    // is determinate only when every track provides a usable sample-window or
    // duration denominator.
    let progress = LoudnessProgress::new(event_tx, candidate_key, release_id, import_id);
    let track_total_frames: Vec<Option<u64>> = audio_formats
        .iter()
        .zip(tracks_to_files)
        .map(|(audio_format, track_file)| {
            let sample_rate = audio_format.sample_rate as u64;
            audio_segments
                .iter()
                .filter(|segment| segment.audio_format_id == audio_format.id)
                .try_fold(0u64, |total, segment| {
                    segment.end_sample.map(|end| {
                        total.saturating_add(
                            (end as u64).saturating_sub(segment.start_sample as u64),
                        )
                    })
                })
                .or_else(|| {
                    track_file
                        .db_track()
                        .duration_ms
                        .filter(|&ms| ms > 0 && sample_rate > 0)
                        .map(|ms| ms as u64 * sample_rate / 1000)
                })
        })
        .collect();
    let scan_total_frames = track_total_frames
        .iter()
        .copied()
        .try_fold(0u64, |total, track_total| {
            track_total.map(|frames| total.saturating_add(frames))
        });
    let mut frames_done_before = 0u64;

    progress.report(progress_fraction(0, scan_total_frames));

    // Decode + measure ONE track at a time: each decode runs on a blocking thread
    // but is awaited before the next starts, so the machine never runs N
    // concurrent decodes — one core's worth of work, and the bar advances as
    // frames are decoded. `audio_formats` and `tracks_to_files`
    // are index-aligned (the formats are built from the same tracks), so `idx`
    // keys both.
    let mut meters: Vec<EbuR128> = Vec::new();
    let mut track_peaks: Vec<f64> = Vec::new();
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
            if let Some(track_total) = track_total_frames[idx] {
                frames_done_before = frames_done_before.saturating_add(track_total);
            }
            let fraction = progress_fraction(frames_done_before, scan_total_frames);
            progress.report(fraction);
            continue;
        }
        // Frames in this track's window: the sample window when known, else
        // duration × sample rate. With neither, the whole bar is indeterminate.
        let total_frames = track_total_frames[idx];
        let mut decode_segments = Vec::new();
        let mut missing_segment = false;
        for segment in &segments {
            let Some(buffer) = file_buffers.get(&segment.file_id).and_then(|b| b.clone()) else {
                warn!(
                    "loudness: cannot read segment source file {} for track {}; track stays unmeasured",
                    segment.file_id,
                    idx + 1
                );
                missing_segment = true;
                break;
            };
            decode_segments.push((
                buffer,
                segment.start_sample as u64,
                segment.end_sample.map(|sample| sample as u64),
            ));
        }
        if missing_segment {
            if let Some(track_total) = total_frames {
                frames_done_before = frames_done_before.saturating_add(track_total);
            }
            let fraction = progress_fraction(frames_done_before, scan_total_frames);
            progress.report(fraction);
            continue;
        }
        // Cloned into the blocking task so the sink can report progress on the
        // import event channel straight from the worker thread.
        let task_progress = progress.clone();
        let outcome = tokio::task::spawn_blocking(move || {
            let mut sink = LoudnessProgressSink {
                state: None,
                error: None,
                total_frames,
                done_frames: 0,
                frames_done_before,
                scan_total_frames,
                decode_error_count: 0,
                discarded_packet_count: 0,
                frames_since_emit: 0,
                progress: task_progress,
            };
            // A decode that fails outright is broken, but so is one that returns
            // Ok over fatal errors or a truncated body — `broken_reason` reads the
            // error count and frame shortfall the sink captured.
            let never_cancelled = Arc::new(std::sync::atomic::AtomicBool::new(false));
            for (buffer, start_sample, end_sample) in decode_segments {
                if let Err(e) = crate::audio_codec::decode_audio_to_verifying_sink(
                    buffer,
                    Some(start_sample),
                    end_sample,
                    &mut sink,
                    never_cancelled.clone(),
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
            if sink.discarded_packet_count > 0 {
                warn!(
                    "loudness: discarded {} invalid packet(s) for track {}; decoded {} of {:?} expected frames",
                    sink.discarded_packet_count,
                    idx + 1,
                    sink.done_frames,
                    sink.total_frames,
                );
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
        if let Some(track_total) = total_frames {
            frames_done_before = frames_done_before.saturating_add(track_total);
        }
        let fraction = progress_fraction(frames_done_before, scan_total_frames);
        progress.report(fraction);
    }

    let album_loudness = crate::loudness::album_loudness(&meters);
    let album_peak = crate::loudness::album_peak(&track_peaks);
    LoudnessResult {
        album_loudness_lufs: album_loudness,
        album_peak_linear: album_peak,
        broken: broken_tracks,
    }
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
            frames_done_before: 0,
            scan_total_frames: total,
            decode_error_count: errors,
            discarded_packet_count: 0,
            frames_since_emit: 0,
            progress: LoudnessProgress::new(&event_tx, "test", "release-1", "import-1"),
        }
    }

    #[test]
    fn measured_frames_control_progress_value_and_determinacy() {
        let (event_tx, mut rx) = broadcast::channel(16);
        let emit = |total_frames, done_frames, frames_done_before, scan_total_frames| {
            LoudnessProgressSink {
                state: None,
                error: None,
                total_frames,
                done_frames,
                frames_done_before,
                scan_total_frames,
                decode_error_count: 0,
                discarded_packet_count: 0,
                frames_since_emit: 0,
                progress: LoudnessProgress::new(&event_tx, "test", "release-1", "import-1"),
            }
            .emit();
        };

        emit(Some(900), 450, 100, Some(1_000));
        emit(None, 44_100, 100, None);

        let mut percents = Vec::new();
        while let Ok(event) = rx.try_recv() {
            match event {
                crate::import::handle::ImportEvent::ImportProgress {
                    progress: crate::import::types::ImportProgress::Progress { percent, phase, .. },
                    ..
                } => {
                    assert_eq!(phase, crate::import::types::ImportPhase::MeasuringLoudness);
                    percents.push(percent);
                }
                other => panic!("unexpected event: {other:?}"),
            }
        }

        // The determinate scan is 55% through; the indeterminate one has no
        // percent to report, so the row's percent stands where it was.
        assert_eq!(percents, vec![55]);
    }

    /// The broken signature: a gross frame shortfall (a truncated body under a
    /// valid header) or any fatal decode error. A full-window decode, including
    /// one that discarded an invalid packet, is not broken.
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

        let mut complete_with_discard = sink_with(Some(1000), 1000, 0);
        crate::audio_codec::DecodedSink::set_discarded_packet_count(&mut complete_with_discard, 1);
        crate::audio_codec::DecodedSink::set_discarded_packet_count(&mut complete_with_discard, 1);
        assert_eq!(complete_with_discard.discarded_packet_count, 2);
        assert!(
            complete_with_discard.broken_reason().is_none(),
            "a discarded packet is recoverable when frame coverage is complete"
        );

        let mut unknown_with_discard = sink_with(None, 0, 0);
        crate::audio_codec::DecodedSink::set_discarded_packet_count(&mut unknown_with_discard, 1);
        assert!(
            unknown_with_discard.broken_reason().is_some(),
            "a discarded packet needs an expected frame count to prove recovery"
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

        // Pre-filled buffers: the decode never waits on a fill, so the test
        // exercises the window/verify logic, not streaming.
        let buffer_of = |bytes: &[u8]| {
            let buffer = create_sparse_buffer(bytes.len() as u64);
            buffer.append_at(0, bytes);
            buffer
        };
        let never = || Arc::new(std::sync::atomic::AtomicBool::new(false));

        let mut sink = sink_with(Some(total), 0, 0);
        let ok = crate::audio_codec::decode_audio_to_verifying_sink(
            buffer_of(&clean),
            Some(start),
            Some(end),
            &mut sink,
            never(),
        );
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
        let res = crate::audio_codec::decode_audio_to_verifying_sink(
            buffer_of(truncated),
            Some(start),
            Some(end),
            &mut sink,
            never(),
        );
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
            source_audio: crate::import::folder_scanner::ScannedAudio {
                content_type: crate::util::content_type::ContentType::Flac,
                duration_ms: 1_000,
                format: crate::album_detail::AudioFormat {
                    codec: "FLAC".to_string(),
                    sample_rate_hz: 44_100,
                    bits_per_sample: Some(16),
                    bitrate_kbps: None,
                    channels: 2,
                },
            },
        }
    }

    /// The whole-percent moves one loudness pass reported, in order.
    fn loudness_percents(
        rx: &mut broadcast::Receiver<crate::import::handle::ImportEvent>,
    ) -> Vec<u8> {
        let mut percents = Vec::new();
        while let Ok(event) = rx.try_recv() {
            if let crate::import::handle::ImportEvent::ImportProgress {
                progress: crate::import::types::ImportProgress::Progress { percent, phase, .. },
                ..
            } = event
            {
                assert_eq!(phase, crate::import::types::ImportPhase::MeasuringLoudness);
                percents.push(percent);
            }
        }
        percents
    }

    fn cue_flac_fixture(name: &str) -> PathBuf {
        PathBuf::from(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/fixtures/cue_flac"
        ))
        .join(name)
    }

    /// A track whose source file can't be read stays unmeasured and never fails
    /// the import.
    #[tokio::test]
    async fn measure_loudness_skips_unreadable_source() {
        let (event_tx, mut rx) = broadcast::channel(16);
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
            "release-1",
            "import-1",
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

        // Its absent duration leaves the pass indeterminate, so there is no
        // percent to report and the row's bar stands where it was.
        assert_eq!(
            loudness_percents(&mut rx),
            Vec::<u8>::new(),
            "an indeterminate pass reports no percent",
        );
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
            "release-1",
            "import-1",
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
            "release-1",
            "import-1",
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

    #[tokio::test]
    async fn measure_loudness_progress_weights_tracks_by_frames() {
        crate::audio_codec::init();
        let (event_tx, mut rx) = broadcast::channel(32);
        let path = cue_flac_fixture("03 Test Artist - Track Three (Brown Noise).flac");
        let mut audio_formats = vec![
            audio_format("track-0", "af-0"),
            audio_format("track-1", "af-1"),
        ];
        let mut short = whole_file_main_segment("af-0", "file-0");
        short.end_sample = Some(2_205);
        let mut long = whole_file_main_segment("af-1", "file-0");
        long.end_sample = Some(6_615);
        let audio_segments = vec![short, long];
        let file_ids = HashMap::from([(path.clone(), "file-0".to_string())]);
        let tracks = vec![
            standalone_track("track-0", &path),
            standalone_track("track-1", &path),
        ];

        measure_loudness(
            &event_tx,
            &mut audio_formats,
            &audio_segments,
            &file_ids,
            &tracks,
            "cand",
            "release-1",
            "import-1",
        )
        .await;

        // The first track holds 2,205 of the 8,820 measured frames, so the pass
        // stands at 25% when it finishes — a quarter, not the half an
        // equal-track-slices bar would report.
        assert!(
            loudness_percents(&mut rx).contains(&25),
            "the first of two tracks completing is a quarter of the frame work",
        );
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
            "release-1",
            "import-1",
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
