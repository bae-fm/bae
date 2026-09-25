//! Loudness measurement for an import's tracks.
//!
//! Decodes each track's sample window on a blocking thread, streams it through
//! an EBU R128 meter, and reports the per-track loudness/true-peak plus the
//! album aggregate. Progress ticks ride the import event channel passed in by
//! the service, so the analyzer needs no reference back to it.

use std::collections::HashMap;
use std::num::NonZeroUsize;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

use tracing::{debug, warn};

use crate::import::types::TrackFile;
use crate::playback::data_source::LocalReader;
use crate::playback::stream_pipeline::{SegmentDecodeParams, StreamDecodeParams};
use crate::playback::track_sources::{run_tracks_over_sources, SourceStream};
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

/// One track's decode outcome from the blocking measure task.
enum TrackOutcome {
    /// The source was read through: the measurement (absent when
    /// unmeasured/silent/failed) and the broken-decode reason (absent when the
    /// decode produced enough of the expected window to be usable).
    Decoded {
        measured: Option<(ebur128::EbuR128, f64, f64)>,
        broken: Option<String>,
    },
    /// A read of the source failed, so the decode never saw the track's bytes.
    /// Says nothing about the audio.
    Unreadable(Arc<crate::playback::PlaybackError>),
}

/// Where one loudness pass reports its scan: the candidate row's phase percent,
/// like every other running phase. Tracks measured at once all advance one
/// shared frame count. The decode calls in every ~0.1s of audio, but the
/// percent is only sent when its whole number rises, so the row's retained
/// snapshot rebuilds at most a hundred times over the pass instead of once per
/// call, and never moves backward when tracks finish out of order.
#[derive(Clone)]
struct LoudnessProgress {
    event_tx: crate::import::handle::ImportEventBus,
    candidate_key: String,
    release_id: String,
    import_id: String,
    /// Expected frames across the whole candidate, absent when any track's
    /// frame count cannot be established; the bar is then indeterminate.
    scan_total_frames: Option<u64>,
    /// Frames measured (or skipped) so far across every track.
    frames_done: Arc<AtomicU64>,
    /// The highest percent sent so far, held while a higher one is sent so
    /// tracks finishing on several threads cannot send percents out of order.
    last_percent: Arc<std::sync::Mutex<u8>>,
}

impl LoudnessProgress {
    fn new(
        event_tx: &crate::import::handle::ImportEventBus,
        candidate_key: &str,
        release_id: &str,
        import_id: &str,
        scan_total_frames: Option<u64>,
    ) -> Self {
        Self {
            event_tx: event_tx.clone(),
            candidate_key: candidate_key.to_string(),
            release_id: release_id.to_string(),
            import_id: import_id.to_string(),
            scan_total_frames,
            frames_done: Arc::new(AtomicU64::new(0)),
            last_percent: Arc::new(std::sync::Mutex::new(0)),
        }
    }

    fn report_initial(&self) {
        self.send(
            self.scan_total_frames
                .is_some_and(|total| total > 0)
                .then_some(0),
        );
    }

    /// Count `frames` more of the scan as done and report where it stands.
    /// Nothing is reported when the bar is indeterminate.
    fn advance(&self, frames: u64) {
        let Some(total) = self.scan_total_frames else {
            return;
        };
        let done = self.frames_done.fetch_add(frames, Ordering::Relaxed) + frames;
        let Some(fraction) = progress_fraction(done, Some(total)) else {
            return;
        };
        let percent = (fraction * 100.0).round().clamp(0.0, 100.0) as u8;
        let mut last_percent = self
            .last_percent
            .lock()
            .expect("loudness progress mutex poisoned");
        if *last_percent >= percent {
            return;
        }
        *last_percent = percent;
        self.send(Some(percent));
    }

    fn send(&self, percent: Option<u8>) {
        self.event_tx.send(crate::import::handle::ImportEvent::ImportProgress {
            candidate_key: self.candidate_key.clone(),
            progress: crate::import::types::ImportProgress::Progress {
                id: self.release_id.clone(),
                percent,
                phase: crate::import::types::ImportPhase::MeasuringLoudness,
                import_id: self.import_id.clone(),
            },
        });
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
    /// This track's frames already counted into the pass's progress.
    reported_frames: u64,
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
    fn new(total_frames: Option<u64>, progress: LoudnessProgress) -> Self {
        Self {
            state: None,
            error: None,
            total_frames,
            done_frames: 0,
            reported_frames: 0,
            decode_error_count: 0,
            discarded_packet_count: 0,
            frames_since_emit: 0,
            progress,
        }
    }

    /// Count this track's newly measured frames, capped at its expected total,
    /// into the pass's progress. Without that total the bar is indeterminate
    /// and nothing is counted.
    fn emit(&mut self) {
        let Some(total) = self.total_frames else {
            return;
        };
        let counted = self.done_frames.min(total);
        let newly = counted.saturating_sub(self.reported_frames);
        self.reported_frames = counted;
        self.progress.advance(newly);
    }

    /// Count the rest of this track's expected frames as done, however much of
    /// it the decode produced.
    fn finish_progress(&mut self) {
        if let Some(total) = self.total_frames {
            let rest = total.saturating_sub(self.reported_frames);
            self.reported_frames = total;
            self.progress.advance(rest);
        }
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

    fn add_decode_error_count(&mut self, count: u32) {
        self.decode_error_count = self.decode_error_count.saturating_add(count);
    }

    fn add_discarded_packet_count(&mut self, count: u32) {
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

/// One track the pass measures: where its audio lives and how many frames it
/// is expected to yield.
struct MeasuredTrack {
    /// Its index in the pass's track list.
    index: usize,
    title: String,
    /// Frames in its window: the sample window when known, else duration ×
    /// sample rate. With neither, the whole bar is indeterminate.
    total_frames: Option<u64>,
    /// Its segments in play order: the file each reads, by file id, and its
    /// window there.
    segments: Vec<(String, crate::db::SegmentSpan)>,
    /// Whether the demuxer may jump to a segment's recorded start byte (not
    /// APE, which sample-seeks its index).
    byte_seekable: bool,
}

/// Decode and meter one track's segments from their open streams, counting
/// its frames into the pass's progress as they are measured.
fn measure_track(
    track: &MeasuredTrack,
    streams: &HashMap<String, SharedSparseBuffer>,
    progress: LoudnessProgress,
) -> TrackOutcome {
    let mut sink = LoudnessProgressSink::new(track.total_frames, progress);
    let decoded = decode_track(track, streams, &mut sink);
    sink.finish_progress();
    match decoded {
        Err(DecodeStop::Unreadable(error)) => TrackOutcome::Unreadable(error),
        Err(DecodeStop::Failed(reason)) => TrackOutcome::Decoded {
            measured: None,
            broken: Some(reason),
        },
        Ok(()) => {
            // A decode that fails outright is broken, but so is one that
            // returns Ok over fatal errors or a truncated body — `broken_reason`
            // reads the error count and frame shortfall the sink captured.
            let broken = sink.broken_reason();
            let measured = match sink.into_result() {
                Ok((meter, Some(m))) => Some((meter, m.loudness_lufs, m.peak_linear)),
                Ok((_, None)) => {
                    debug!(
                        "loudness: track {} has no usable loudness (silent); unmeasured",
                        track.index + 1
                    );
                    None
                }
                Err(e) => {
                    warn!(
                        "loudness: measure failed for track {}: {e}; track stays unmeasured",
                        track.index + 1
                    );
                    None
                }
            };
            TrackOutcome::Decoded { measured, broken }
        }
    }
}

/// Why a track's decode could not go on.
enum DecodeStop {
    /// A read of the source failed.
    Unreadable(Arc<crate::playback::PlaybackError>),
    /// The decode itself failed, for the reason given.
    Failed(String),
}

/// Stream every segment of `track` through `sink`, seeking each the way
/// playback does.
fn decode_track(
    track: &MeasuredTrack,
    streams: &HashMap<String, SharedSparseBuffer>,
    sink: &mut LoudnessProgressSink,
) -> Result<(), DecodeStop> {
    let segments = track
        .segments
        .iter()
        .map(|(file_id, span)| {
            let buffer = streams
                .get(file_id)
                .expect("every segment's file is open for its track")
                .clone();
            SegmentDecodeParams::new(buffer, *span, 0)
        })
        .collect();
    let decode = StreamDecodeParams::new(segments, track.byte_seekable, 0, 0);
    let never_cancelled = Arc::new(std::sync::atomic::AtomicBool::new(false));
    match decode.verify_to_sink(sink, never_cancelled) {
        Ok(()) => Ok(()),
        Err(crate::audio_codec::DecodeError::SourceRead(error)) => {
            Err(DecodeStop::Unreadable(error))
        }
        Err(e) => {
            warn!(
                "loudness: decode failed for track {}: {e}; track stays unmeasured",
                track.index + 1
            );
            Err(DecodeStop::Failed(format!("decode failed: {e}")))
        }
    }
}

/// Measure each track's loudness + true peak and the album's combined loudness,
/// attaching the per-track measurements to `audio_formats` and returning the
/// album-level `(loudness_lufs, peak_linear)`.
///
/// Up to `parallelism` tracks are decoded and metered at once, each on a
/// blocking thread (FFmpeg decode is blocking CPU work). A source file is open
/// only while tracks reading it run (see [`run_tracks_over_sources`]): tracks
/// of one CUE image share its stream, each through its own reader, and the fill
/// keeps a window ahead of each decode and evicts behind them. At most
/// `parallelism` source files are open at once however many tracks there are.
///
/// A track whose decode/measure fails, or that is too quiet to have a usable
/// loudness, keeps NULL loudness/peak and still imports — the skip is logged with
/// its file path, and a measurement failure never aborts the import.
///
/// The full decode doubles as verification: a fatal decode error, or output
/// frames far short of the expected window (a truncated body under a valid
/// header), marks the track broken. Broken tracks are always logged and returned;
/// the caller decides whether to fail the import (per `verify_decode_on_import`).
///
/// A source that can't be read is neither: the pass stops admitting tracks at
/// the first read failure and fails the import with
/// [`crate::import::ImportError::SourceRead`] carrying the read's own error.
/// Nothing has been committed yet, so importing again once the source is
/// readable is the whole remedy; carrying on would commit tracks that were
/// never measured or verified.
#[allow(clippy::too_many_arguments)]
pub(super) async fn measure_loudness(
    event_tx: &crate::import::handle::ImportEventBus,
    parallelism: NonZeroUsize,
    audio_formats: &mut [crate::db::DbAudioFormat],
    audio_segments: &[crate::db::DbAudioSegment],
    file_ids: &HashMap<PathBuf, String>,
    source_file_sizes: &HashMap<PathBuf, u64>,
    tracks_to_files: &[TrackFile],
    candidate_key: &str,
    release_id: &str,
    import_id: &str,
) -> Result<LoudnessResult, crate::import::ImportError> {
    // Every source's validated size is established before any is opened, so a
    // missing size fails the pass before a single file is read.
    let mut sources: HashMap<String, (PathBuf, u64)> = HashMap::new();
    for (path, file_id) in file_ids {
        let size =
            *source_file_sizes
                .get(path)
                .ok_or_else(|| crate::import::ImportError::Internal {
                    detail: format!("validated source size is missing for {}", path.display()),
                })?;
        sources.insert(file_id.clone(), (path.clone(), size));
    }

    // Each track's segments in play order. `audio_formats` and
    // `tracks_to_files` are index-aligned (the formats are built from the same
    // tracks), so an index keys all three.
    let track_segments: Vec<Vec<&crate::db::DbAudioSegment>> = audio_formats
        .iter()
        .map(|audio_format| {
            let mut segments: Vec<_> = audio_segments
                .iter()
                .filter(|segment| segment.audio_format_id == audio_format.id)
                .collect();
            segments.sort_by_key(|segment| segment.segment_index);
            segments
        })
        .collect();

    // The bar uses actual frame work rather than equal track slices: a candidate
    // is determinate only when every track provides a usable sample-window or
    // duration denominator.
    let track_total_frames: Vec<Option<u64>> = audio_formats
        .iter()
        .zip(tracks_to_files)
        .zip(&track_segments)
        .map(|((audio_format, track_file), segments)| {
            let sample_rate = audio_format.sample_rate as u64;
            segments
                .iter()
                .try_fold(0u64, |total, segment| {
                    segment.end_sample.map(|end| {
                        total.saturating_add(
                            (end as u64).saturating_sub(segment.start_sample as u64),
                        )
                    })
                })
                .or_else(|| {
                    track_file
                        .db_track
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
    let progress = LoudnessProgress::new(
        event_tx,
        candidate_key,
        release_id,
        import_id,
        scan_total_frames,
    );
    progress.report_initial();

    // A track with no segments, or one reading a file the import holds no
    // source for, stays unmeasured; its frames count as done.
    let mut measured_tracks = Vec::new();
    for (index, segments) in track_segments.iter().enumerate() {
        if segments.is_empty() {
            warn!(
                "loudness: audio format {} has no segments; track stays unmeasured",
                audio_formats[index].id
            );
            progress.advance(track_total_frames[index].unwrap_or(0));
            continue;
        }
        if let Some(segment) = segments
            .iter()
            .find(|segment| !sources.contains_key(&segment.file_id))
        {
            warn!(
                "loudness: cannot read segment source file {} for track {}; track stays unmeasured",
                segment.file_id,
                index + 1
            );
            progress.advance(track_total_frames[index].unwrap_or(0));
            continue;
        }
        measured_tracks.push(MeasuredTrack {
            index,
            title: tracks_to_files[index].db_track.title.clone(),
            total_frames: track_total_frames[index],
            segments: segments
                .iter()
                .map(|segment| (segment.file_id.clone(), segment.span()))
                .collect(),
            byte_seekable: audio_formats[index].content_type
                != crate::util::content_type::ContentType::Ape,
        });
    }

    let outcomes = run_tracks_over_sources(
        measured_tracks,
        parallelism,
        |track: &MeasuredTrack| {
            track
                .segments
                .iter()
                .map(|(file_id, _)| file_id.clone())
                .collect()
        },
        |file_id: &String| {
            let (path, size) = &sources[file_id];
            let error_path = path.clone();
            SourceStream::start(
                Box::new(LocalReader::new(path)),
                *size,
                Box::new(move |error| {
                    warn!("loudness: streaming {error_path:?} failed: {error}");
                }),
            )
        },
        |track, streams| {
            let progress = progress.clone();
            async move {
                let index = track.index;
                let label = format!("{} (track {})", track.title, index + 1);
                let outcome =
                    tokio::task::spawn_blocking(move || measure_track(&track, &streams, progress))
                        .await;
                match outcome {
                    Ok(TrackOutcome::Unreadable(error)) => {
                        Err(crate::import::ImportError::SourceRead {
                            track: label,
                            error,
                        })
                    }
                    Ok(outcome) => Ok((index, Some(outcome))),
                    Err(e) => {
                        warn!("loudness: measurement task panicked: {e}; track stays unmeasured");
                        Ok((index, None))
                    }
                }
            }
        },
    )
    .await?;

    let mut meters: Vec<ebur128::EbuR128> = Vec::new();
    let mut track_peaks: Vec<f64> = Vec::new();
    let mut broken_tracks: Vec<String> = Vec::new();
    for (index, outcome) in outcomes {
        let Some(TrackOutcome::Decoded { measured, broken }) = outcome else {
            continue;
        };
        if let Some((meter, loudness_lufs, peak_linear)) = measured {
            audio_formats[index].track_loudness_lufs = Some(loudness_lufs);
            audio_formats[index].track_peak_linear = Some(peak_linear);
            meters.push(meter);
            track_peaks.push(peak_linear);
        }
        if let Some(reason) = broken {
            warn!(
                "import verify: track source for track {} looks broken: {reason}",
                index + 1
            );
            broken_tracks.push(format!(
                "{} (track {}): {reason}",
                tracks_to_files[index].db_track.title,
                index + 1
            ));
        }
    }

    let album_loudness = crate::loudness::album_loudness(&meters);
    let album_peak = crate::loudness::album_peak(&track_peaks);
    Ok(LoudnessResult {
        album_loudness_lufs: album_loudness,
        album_peak_linear: album_peak,
        broken: broken_tracks,
    })
}

#[cfg(test)]
#[path = "loudness_recovery_tests.rs"]
mod recovery_tests;

#[cfg(test)]
#[path = "loudness_tests.rs"]
mod tests;
