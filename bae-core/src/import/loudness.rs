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
    /// Authoritative source bit depth (NULL for lossy, where the decoded
    /// container depth is used instead).
    source_bits: Option<u32>,
    state: Option<MeterState>,
    error: Option<String>,
    /// This track's total frame count, used to fill its bar segment. `None` when
    /// neither the sample window nor a track duration is known — the segment then
    /// only advances at the post-track tick.
    total_frames: Option<u64>,
    done_frames: u64,
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
    fn on_format(&mut self, sample_rate: u32, channels: u32, bits_per_sample: u32) {
        let sample_bits = self.source_bits.unwrap_or(bits_per_sample);
        match crate::loudness::LoudnessMeter::new(channels, sample_rate, sample_bits) {
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
pub(super) async fn measure_loudness(
    event_tx: &broadcast::Sender<crate::import::handle::ImportEvent>,
    audio_formats: &mut [crate::db::DbAudioFormat],
    tracks_to_files: &[TrackFile],
    candidate_key: &str,
) -> (Option<f64>, Option<f64>) {
    use ebur128::EbuR128;

    // Source bytes per file, read once and shared across that file's tracks
    // (every CUE track of one image points at the same bytes). An unreadable
    // file yields `None`, so its tracks are skipped (logged) rather than
    // measured against missing bytes.
    let mut file_bytes: HashMap<PathBuf, Option<Arc<Vec<u8>>>> = HashMap::new();
    for tf in tracks_to_files {
        let path = tf.file_path().to_path_buf();
        file_bytes.entry(path.clone()).or_insert_with(|| {
            match std::fs::read(&path) {
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
    for (idx, tf) in tracks_to_files.iter().enumerate() {
        let path = tf.file_path().to_path_buf();
        let Some(bytes) = file_bytes.get(&path).and_then(|b| b.clone()) else {
            tracks_done += 1;
            let fraction = tracks_done as f32 / tracks_total as f32;
            emit_loudness_progress(event_tx, candidate_key, tracks_done, tracks_total, fraction);
            continue;
        };
        let start_sample = audio_formats[idx].start_sample as u64;
        let end_sample = audio_formats[idx].end_sample.map(|s| s as u64);
        // The source's value range bit depth: the decoder hands the meter i32
        // samples scaled to the source's bits (16-bit values for 16-bit FLAC,
        // 24-bit for 24-bit), so the meter must shift them up to full i32. The
        // stored `bits_per_sample` is the authoritative source depth (NULL for
        // lossy codecs, where the decoded container depth is used instead).
        let source_bits = audio_formats[idx].bits_per_sample.map(|b| b as u32);
        // Frames in this track's window, to fill its bar segment as the decode
        // streams: the sample window when known, else the track duration ×
        // sample rate. Absent both, the segment only steps at the post-track
        // tick.
        let sample_rate = audio_formats[idx].sample_rate as u64;
        let total_frames = match end_sample {
            Some(end) => Some(end.saturating_sub(start_sample)),
            None => tf
                .db_track()
                .duration_ms
                .filter(|&ms| ms > 0 && sample_rate > 0)
                .map(|ms| ms as u64 * sample_rate / 1000),
        };
        // Cloned into the blocking task so the sink can emit progress on the
        // import event channel directly from the worker thread.
        // `idx`/`tracks_total` place this track's scan in the bar.
        let task_event_tx = event_tx.clone();
        let key = candidate_key.to_string();
        let measured = tokio::task::spawn_blocking(move || {
            let mut sink = LoudnessProgressSink {
                source_bits,
                state: None,
                error: None,
                total_frames,
                done_frames: 0,
                frames_since_emit: 0,
                event_tx: task_event_tx,
                candidate_key: key,
                idx: idx as u32,
                tracks_total,
            };
            if let Err(e) = crate::audio_codec::decode_audio_to_sink(
                &bytes,
                Some(start_sample),
                end_sample,
                &mut sink,
            ) {
                warn!("loudness: decode failed for {path:?}: {e}; track stays unmeasured");
                return None;
            }
            match sink.into_result() {
                Ok((meter, Some(m))) => Some((meter, m.loudness_lufs, m.peak_linear)),
                Ok((_, None)) => {
                    debug!("loudness: {path:?} has no usable loudness (silent); unmeasured");
                    None
                }
                Err(e) => {
                    warn!("loudness: measure failed for {path:?}: {e}; track stays unmeasured");
                    None
                }
            }
        })
        .await;

        match measured {
            Ok(Some((meter, loudness_lufs, peak_linear))) => {
                audio_formats[idx].track_loudness_lufs = Some(loudness_lufs);
                audio_formats[idx].track_peak_linear = Some(peak_linear);
                meters.push(meter);
                track_peaks.push(peak_linear);
            }
            Ok(None) => {}
            Err(e) => warn!("loudness: measurement task panicked: {e}; track stays unmeasured"),
        }
        tracks_done += 1;
        let fraction = tracks_done as f32 / tracks_total as f32;
        emit_loudness_progress(event_tx, candidate_key, tracks_done, tracks_total, fraction);
    }

    let album_loudness = crate::loudness::album_loudness(&meters);
    let album_peak = crate::loudness::album_peak(&track_peaks);
    (album_loudness, album_peak)
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
