//! Loudness measurement for an import's tracks.
//!
//! Decodes each track's sample window on a blocking thread, streams it through
//! an EBU R128 meter, and reports the per-track loudness/true-peak plus the
//! album aggregate. Progress ticks ride the import event channel passed in by
//! the service, so the analyzer needs no reference back to it.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

use tracing::{debug, warn};

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
/// like every other running phase. The decode calls in every ~0.1s of audio, but
/// the percent is only sent when its whole number moves, so the row's retained
/// snapshot rebuilds at most a hundred times over the pass instead of once per
/// call.
#[derive(Clone)]
struct LoudnessProgress {
    event_tx: crate::import::handle::ImportEventBus,
    candidate_key: String,
    release_id: String,
    import_id: String,
    /// Shared by every track's sink, so the percent is the whole candidate's and
    /// crossing a track boundary does not re-report where it already stands.
    last_percent: Arc<std::sync::atomic::AtomicU8>,
}

impl LoudnessProgress {
    fn new(
        event_tx: &crate::import::handle::ImportEventBus,
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

    fn report_initial(&self, total_frames: Option<u64>) {
        self.event_tx.send(crate::import::handle::ImportEvent::ImportProgress {
                candidate_key: self.candidate_key.clone(),
                progress: crate::import::types::ImportProgress::Progress {
                    id: self.release_id.clone(),
                    percent: total_frames.is_some_and(|total| total > 0).then_some(0),
                    phase: crate::import::types::ImportPhase::MeasuringLoudness,
                    import_id: self.import_id.clone(),
                },
            },
        );
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
        self.event_tx.send(crate::import::handle::ImportEvent::ImportProgress {
                candidate_key: self.candidate_key.clone(),
                progress: crate::import::types::ImportProgress::Progress {
                    id: self.release_id.clone(),
                    percent: Some(percent),
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

/// Open a stream over one source file for the tracks that read it. The fill
/// task owns the open handle and closes it once the returned buffer is
/// dropped. A read failure fails the buffer with the read's error, which the
/// decode reading it returns as its outcome.
fn open_source_stream(path: &std::path::Path, size: u64) -> SharedSparseBuffer {
    let buffer = create_sparse_buffer(size);
    let error_path = path.to_path_buf();
    Box::new(LocalReader::new(path)).start_reading(
        buffer.clone(),
        Box::new(move |error| {
            warn!("loudness: streaming {error_path:?} failed: {error}");
        }),
    );
    buffer
}

/// Measure each track's loudness + true peak and the album's combined loudness,
/// attaching the per-track measurements to `audio_formats` and returning the
/// album-level `(loudness_lufs, peak_linear)`.
///
/// Each track's window is decoded and measured on a blocking thread (FFmpeg
/// decode is blocking CPU work). A file's compressed bytes stream through one
/// `SparseStreamingBuffer` for the run of tracks that read it (the tracks of a
/// CUE image), opened when that run starts and closed when it ends; the fill
/// keeps a window ahead of the decode and evicts behind it, and decoding one
/// track window at a time bounds transient PCM memory to a single track.
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
/// A source that can't be read is neither: the pass stops at the first read
/// failure and fails the import with [`crate::import::ImportError::SourceRead`]
/// carrying the read's own error. Nothing has been committed yet, so importing
/// again once the source is readable is the whole remedy; carrying on would
/// commit tracks that were never measured or verified.
pub(super) async fn measure_loudness(
    event_tx: &crate::import::handle::ImportEventBus,
    audio_formats: &mut [crate::db::DbAudioFormat],
    audio_segments: &[crate::db::DbAudioSegment],
    file_ids: &HashMap<PathBuf, String>,
    source_file_sizes: &HashMap<PathBuf, u64>,
    tracks_to_files: &[TrackFile],
    candidate_key: &str,
    release_id: &str,
    import_id: &str,
) -> Result<LoudnessResult, crate::import::ImportError> {
    use ebur128::EbuR128;

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
    // The streams of exactly the track being measured, by file id, so the files
    // held open never exceed one track's segments however many tracks the
    // candidate has. A file opens when the first track that reads it is
    // measured and closes when the pass reaches a track that does not read it.
    // Consecutive tracks of one CUE image share its stream, so the image opens
    // once for the run of its tracks.
    let mut open_streams: HashMap<String, SharedSparseBuffer> = HashMap::new();

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
    let mut frames_done_before = 0u64;

    progress.report_initial(scan_total_frames);

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
        open_streams.retain(|file_id, _| {
            segments
                .iter()
                .any(|segment| &segment.file_id == file_id)
        });
        let mut decode_segments = Vec::new();
        let mut missing_segment = false;
        for segment in &segments {
            let stream = match open_streams.get(&segment.file_id) {
                Some(buffer) => Some(buffer.clone()),
                None => sources
                    .get(&segment.file_id)
                    .map(|(path, size)| open_source_stream(path, *size))
                    .inspect(|buffer| {
                        open_streams.insert(segment.file_id.clone(), buffer.clone());
                    }),
            };
            let Some(buffer) = stream else {
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
                match crate::audio_codec::decode_audio_to_verifying_sink(
                    buffer,
                    Some(start_sample),
                    end_sample,
                    &mut sink,
                    never_cancelled.clone(),
                ) {
                    Ok(()) => {}
                    Err(crate::audio_codec::DecodeError::SourceRead(error)) => {
                        return TrackOutcome::Unreadable(error);
                    }
                    Err(e) => {
                        warn!(
                            "loudness: decode failed for track {}: {e}; track stays unmeasured",
                            idx + 1
                        );
                        return TrackOutcome::Decoded {
                            measured: None,
                            broken: Some(format!("decode failed: {e}")),
                        };
                    }
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
            TrackOutcome::Decoded { measured, broken }
        })
        .await;

        match outcome {
            Ok(TrackOutcome::Unreadable(error)) => {
                return Err(crate::import::ImportError::SourceRead {
                    track: format!("{} (track {})", tf.db_track.title, idx + 1),
                    error,
                });
            }
            Ok(TrackOutcome::Decoded { measured, broken }) => {
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
                        tf.db_track.title,
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
