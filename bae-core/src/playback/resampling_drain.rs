//! Pulls a playback source through an [`AudioDrain`] at the track's rate and
//! hands out samples at the rate and channel count an output's device takes,
//! converting with FFmpeg's resampler when the two differ. The AirPlay output
//! converts to the 44.1 kHz stereo of the realtime stream bae opens to a
//! receiver; the cpal output converts on a Mac whose output device can't run
//! at the track's rate, and passes the samples through everywhere else.

use crate::playback::audio_output::{
    AudioDrain, AudioLockMissLog, AudioOutputControls, AudioState, DrainStatus,
};
use bae_audio::Resampler;

/// The most source frames pulled at once. An output asks for one device
/// buffer at a time, a few hundred to a few thousand frames; a larger request
/// is filled over several pulls.
const MAX_PULL_FRAMES: u64 = 4_096;

/// A drain whose output is at the device's rate and channel count.
pub(crate) struct ResamplingDrain {
    drain: AudioDrain,
    /// A clone of the output's controls, to tell a paused drain from a drained
    /// one.
    controls: AudioOutputControls,
    source_rate: u32,
    source_channels: usize,
    output_rate: u32,
    output_channels: usize,
    /// `None` when the source is already at the output's rate and channel
    /// count, so its samples pass through.
    resampler: Option<Resampler>,
    /// Samples at the output's rate not yet handed out.
    converted: Vec<f32>,
    /// Source samples pulled from the drain, before conversion.
    pulled: Vec<f32>,
    /// Whether the resampler's tail has been flushed for the current drain.
    flushed: bool,
    /// Set once a conversion fails; from then on the drain yields nothing.
    failed: bool,
    lock_miss: Option<AudioLockMissLog>,
    /// What the output plays to, for the failure report.
    output_name: &'static str,
}

impl ResamplingDrain {
    /// A drain from a source at `source_rate` and `source_channels` to an
    /// output at `output_rate` and `output_channels`, or why no converter
    /// between the two could be set up. `lock_miss`, when given, logs the audio
    /// thread failing to take the source's lock.
    pub(crate) fn new(
        drain: AudioDrain,
        controls: AudioOutputControls,
        (source_rate, source_channels): (u32, u32),
        (output_rate, output_channels): (u32, u32),
        lock_miss: Option<AudioLockMissLog>,
        output_name: &'static str,
    ) -> Result<Self, String> {
        let source_channels = source_channels.max(1);
        let output_channels = output_channels.max(1);
        let resampler = if (source_rate, source_channels) == (output_rate, output_channels) {
            None
        } else {
            Some(Resampler::new(
                source_rate,
                source_channels,
                output_rate,
                output_channels,
            )?)
        };
        Ok(Self {
            drain,
            controls,
            source_rate,
            source_channels: source_channels as usize,
            output_rate,
            output_channels: output_channels as usize,
            resampler,
            converted: Vec::new(),
            pulled: Vec::new(),
            flushed: false,
            failed: false,
            lock_miss,
            output_name,
        })
    }

    /// Fill `out` with the next interleaved samples at the output's rate,
    /// pulling only as much of the source as that takes, and return how many
    /// samples were written: always whole frames, fewer than `out.len()` when
    /// the source is paused, starved or drained. The rest of `out` is left as
    /// it was.
    pub(crate) fn read(&mut self, out: &mut [f32]) -> usize {
        while self.converted.len() < out.len() {
            let missing_frames = (out.len() - self.converted.len()).div_ceil(self.output_channels);
            let source_frames = (missing_frames as u64 * u64::from(self.source_rate))
                .div_ceil(u64::from(self.output_rate))
                .clamp(1, MAX_PULL_FRAMES);
            if matches!(self.fill(source_frames as usize), Pull::Idle) {
                break;
            }
        }
        let available = out.len().min(self.converted.len());
        let take = available - available % self.output_channels;
        out[..take].copy_from_slice(&self.converted[..take]);
        self.converted.drain(..take);
        take
    }

    /// Pull up to `frames` source frames and append them, converted, to
    /// `converted`.
    fn fill(&mut self, frames: usize) -> Pull {
        if self.failed {
            return Pull::Idle;
        }
        self.pulled.resize(frames * self.source_channels, 0.0);
        match self
            .drain
            .drain_iteration(&mut self.pulled, true, false, self.lock_miss.as_mut())
        {
            DrainStatus::Samples { read } => {
                self.flushed = false;
                let input = &self.pulled[..read];
                match &mut self.resampler {
                    Some(resampler) => {
                        if let Err(error) = resampler.convert(input, &mut self.converted) {
                            return self.conversion_failed("a chunk", error);
                        }
                    }
                    None => self.converted.extend_from_slice(input),
                }
                Pull::Produced
            }
            // A drained source (completion latched → Stopped) flushes the
            // resampler's tail once; a paused source just waits.
            DrainStatus::Idle => {
                if !self.flushed && self.controls.get_state() == AudioState::Stopped {
                    self.flushed = true;
                    if let Some(resampler) = &mut self.resampler {
                        let before = self.converted.len();
                        if let Err(error) = resampler.flush(&mut self.converted) {
                            return self.conversion_failed("the end of the track", error);
                        }
                        if self.converted.len() > before {
                            return Pull::Produced;
                        }
                    }
                }
                Pull::Idle
            }
        }
    }

    /// Stop yielding after a conversion failed, and tell playback why.
    fn conversion_failed(&mut self, what: &str, error: String) -> Pull {
        self.failed = true;
        self.converted.clear();
        self.drain.report_output_failure(format!(
            "converting {what} for {} failed: {error}",
            self.output_name
        ));
        Pull::Idle
    }
}

/// The outcome of one [`ResamplingDrain::fill`].
enum Pull {
    Produced,
    Idle,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::playback::audio_output::audio_event_channel;
    use crate::playback::source::{PlaybackSource, TrackFmt};
    use bae_audio::track_stream::create_track_stream_pair_with_capacity;
    use std::sync::{Arc, Mutex};

    fn track_fmt() -> TrackFmt {
        TrackFmt {
            track_id: "t".to_string(),
            timeline: crate::playback::TrackTimeline::new(
                Some(std::time::Duration::from_secs(1)),
                std::time::Duration::ZERO,
            ),
            starts_at: crate::playback::StreamPosition::START,
            replay_gain_linear: 1.0,
        }
    }

    /// A playing drain over a finished source holding `samples` at `rate`
    /// stereo.
    fn drain_over(rate: u32, samples: &[f32]) -> (AudioDrain, AudioOutputControls) {
        let (mut sink, stream, _ready) =
            create_track_stream_pair_with_capacity(rate, 2, samples.len());
        sink.push_samples(samples);
        sink.mark_finished();
        let source = Arc::new(Mutex::new(PlaybackSource::new(stream, track_fmt())));
        let controls = AudioOutputControls::new(1.0);
        controls.set_state(AudioState::Playing);
        let (tx, _rx) = audio_event_channel();
        (AudioDrain::new(controls.clone(), source, tx, 100), controls)
    }

    /// Read the whole source in device buffers of `buffer_frames`.
    fn play_out(drain: &mut ResamplingDrain, buffer_frames: usize) -> Vec<f32> {
        let mut heard = Vec::new();
        let mut buffer = vec![0.0; buffer_frames * 2];
        for _ in 0..10_000 {
            let written = drain.read(&mut buffer);
            if written == 0 {
                return heard;
            }
            heard.extend_from_slice(&buffer[..written]);
        }
        panic!("the source didn't drain within 10 000 buffers");
    }

    /// At the device's own rate the samples pass through untouched, whatever
    /// the device's buffer size.
    #[test]
    fn a_source_at_the_output_rate_passes_through() {
        let samples: Vec<f32> = (0..4_000).map(|i| i as f32 / 4_000.0).collect();
        let (drain, controls) = drain_over(44_100, &samples);
        let mut drain =
            ResamplingDrain::new(drain, controls, (44_100, 2), (44_100, 2), None, "the test")
                .unwrap();

        assert_eq!(play_out(&mut drain, 333), samples);
    }

    /// A 192 kHz track on a device at 96 kHz comes out at 96 kHz: half the
    /// frames, and a tone keeps its pitch, so it neither plays fast nor
    /// loses its end. Sabotage: pass the samples through, and twice the frames
    /// come out.
    #[test]
    fn a_source_at_another_rate_is_converted_to_the_output_rate() {
        let frames = 19_200;
        let tone = |rate: f32, frame: usize| {
            (2.0 * std::f32::consts::PI * 1_000.0 * frame as f32 / rate).sin() * 0.5
        };
        let samples: Vec<f32> = (0..frames)
            .flat_map(|frame| {
                let sample = tone(192_000.0, frame);
                [sample, sample]
            })
            .collect();
        let (drain, controls) = drain_over(192_000, &samples);
        let mut drain =
            ResamplingDrain::new(drain, controls, (192_000, 2), (96_000, 2), None, "the test")
                .unwrap();

        let heard = play_out(&mut drain, 512);

        let heard_frames = heard.len() / 2;
        assert!(
            (heard_frames as i64 - 9_600).abs() < 64,
            "19 200 frames at 192 kHz are about 9 600 at 96 kHz, not {heard_frames}"
        );
        // Past the converter's start-up, the output is the same 1 kHz tone
        // sampled at 96 kHz.
        for frame in 1_000..8_000 {
            let expected = tone(96_000.0, frame);
            let actual = heard[frame * 2];
            assert!(
                (actual - expected).abs() < 0.05,
                "frame {frame}: {actual} vs {expected}"
            );
        }
    }
}
