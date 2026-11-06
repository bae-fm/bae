//! Pulls a playback source through an [`AudioDrain`] at the track's rate and
//! hands out samples at the rate and channel count an output's device takes,
//! converting with FFmpeg's resampler when the two differ. The AirPlay output
//! converts to the 44.1 kHz stereo of the realtime stream bae opens to a
//! receiver; the cpal output converts on a Mac whose output device can't run
//! at the track's rate, and passes the samples through everywhere else.

use crate::audio_output::{
    AudioDrain, AudioLockMissLog, AudioOutputControls, AudioState, DrainStatus, SourceLock,
};
use bae_audio::Resampler;
#[cfg(not(target_os = "android"))]
use bae_util::task_failure::Bug;

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
    /// between the two could be set up. `lock_miss`, given for the real-time
    /// callback, makes the drain skip a buffer rather than wait for the
    /// source's lock, and logs each miss; without it the drain's thread waits
    /// for the lock.
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
            Some(Resampler::new(source_rate, source_channels, output_rate, output_channels)?)
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
        let lock = match &mut self.lock_miss {
            Some(lock_miss) => SourceLock::Try(lock_miss),
            None => SourceLock::Wait,
        };
        match self.drain.drain_iteration(&mut self.pulled, true, lock) {
            DrainStatus::Samples { read, .. } => {
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
        self.fail(format!("converting {what} for {} failed: {error}", self.output_name));
        Pull::Idle
    }

    /// Stop yielding after a [`Self::read`] panicked, and tell playback why.
    /// The panic's caller catches it at the output's own callback boundary:
    /// cpal's, which Android's output does not use.
    #[cfg(not(target_os = "android"))]
    pub(crate) fn panicked(&mut self, failed: Bug) {
        self.fail(format!("feeding {}: {failed}", self.output_name));
    }

    fn fail(&mut self, detail: String) {
        self.failed = true;
        self.converted.clear();
        self.drain.report_output_failure(detail);
    }
}

/// The outcome of one [`ResamplingDrain::fill`].
enum Pull {
    Produced,
    Idle,
}

#[cfg(test)]
#[path = "resampling_drain_tests.rs"]
mod tests;
