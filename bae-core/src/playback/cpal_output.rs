//! The system output through cpal, on desktop and iOS.
//!
//! Every stream is built on the system's default output device, and follows a
//! change of it:
//!
//! - **macOS.** cpal plays a stream on the default device through CoreAudio's
//!   default output unit, which moves to the new default by itself, at the same
//!   position; cpal reports the move as `DeviceChanged`.
//! - **Windows.** cpal opens a stream on the default device through the
//!   system's default render endpoint, and reports a change of the default as
//!   `StreamInvalidated`. The stream's owner then builds a new one on the
//!   current default, playing on from the same position.
//! - **iOS.** cpal's stream is a RemoteIO unit, which plays on the audio
//!   session's current route; the system moves the route itself when
//!   headphones, a Bluetooth device or AirPlay come or go. cpal reports the
//!   route changes that need a new stream (a category change, an override, a
//!   route configuration change, media services reset) as `StreamInvalidated`.
//! - **Linux.** cpal opens ALSA's `default` device, which on a PipeWire or
//!   PulseAudio system is the sound server's ALSA plugin, and the stream it
//!   creates names no sink. The sound server moves such a stream to the new
//!   default sink itself: WirePlumber relinks streams without a target when
//!   `default.audio.sink` changes, and PulseAudio 14 and later move the streams
//!   on the old default sink to the new one. With ALSA alone, `default` is fixed
//!   configuration rather than a device that changes.
//!
//! On every platform a stream whose device went away reports
//! `DeviceNotAvailable`. Both reports reach the stream's owner as an
//! invalidation ([`AudioStream::take_invalidation`]): a new stream plays on the
//! current default, or fails to build when there is none.

use super::audio_output::{
    AudioDrain, AudioError, AudioEventSender, AudioLockMissLog, AudioOutput, AudioOutputControls,
    AudioStream,
};
use crate::playback::source::PlaybackSource;
use cpal::traits::{DeviceTrait, HostTrait};
use cpal::{BufferSize, ErrorKind, StreamConfig};
use std::sync::mpsc;
use std::sync::Arc;
use std::sync::Mutex;
use tracing::{debug, error, info, warn};

/// A cpal stream, and the reports its error callback made that the device
/// stopped playing it.
struct CpalStream {
    /// Declared first so it drops first: dropping it stops cpal's threads,
    /// which hold the sending side of `invalidations`.
    stream: cpal::Stream,
    invalidations: mpsc::Receiver<String>,
}

impl AudioStream for CpalStream {
    fn play(&self) -> Result<(), AudioError> {
        cpal::traits::StreamTrait::play(&self.stream).map_err(|error| match error.kind() {
            ErrorKind::DeviceNotAvailable | ErrorKind::DeviceBusy => AudioError::DeviceUnavailable,
            _ => AudioError::Unexpected(format!("starting the stream: {error}")),
        })
    }

    fn take_invalidation(&self) -> Option<String> {
        self.invalidations.try_recv().ok()
    }
}

/// The error callback of a stream: what cpal reports about it, from its audio
/// thread or from the threads that watch the device. A report that the stream
/// can't go on as built — it must be rebuilt, or its device went away — goes to
/// `invalidations`, for the stream's owner to act on.
fn stream_error_handler(
    invalidations: mpsc::Sender<String>,
) -> impl FnMut(cpal::Error) + Send + 'static {
    move |error| match error.kind() {
        ErrorKind::StreamInvalidated | ErrorKind::DeviceNotAvailable => {
            if invalidations.send(error.to_string()).is_err() {
                // The owner dropped this stream's receiver while tearing the
                // stream down, so the stream is going away anyway.
                debug!("the output stream was invalidated as it was torn down: {error}");
            }
        }
        ErrorKind::DeviceChanged => {
            info!("the output stream moved to the new default device: {error}");
        }
        ErrorKind::Xrun => warn!("the output stream ran short of samples: {error}"),
        kind => error!(?kind, "Streaming audio error: {error}"),
    }
}

/// Audio output using the system audio device via CPAL.
pub struct CpalAudioOutput {
    controls: AudioOutputControls,
}

impl CpalAudioOutput {
    /// Build a cpal output. Verifies a default output device exists up front so
    /// construction fails fast when there's no audio hardware. Which device the
    /// samples land on is resolved per stream build.
    pub fn new() -> Result<Self, AudioError> {
        let host = cpal::default_host();
        host.default_output_device()
            .ok_or(AudioError::DeviceUnavailable)?;

        Ok(Self {
            controls: AudioOutputControls::new(1.0),
        })
    }
}

impl AudioOutput for CpalAudioOutput {
    fn create_stream(
        &mut self,
        source: Arc<Mutex<PlaybackSource>>,
        source_sample_rate: u32,
        source_channels: u32,
        audio_events: AudioEventSender,
        position_update_interval_ms: u32,
    ) -> Result<Box<dyn AudioStream>, AudioError> {
        // Resolve the default output device at each build. On macOS cpal sets
        // the stream's sample rate on the device it resolved as the default, so
        // a device kept from an earlier build would set the rate of whatever was
        // default then, not of the device that plays.
        let host = cpal::default_host();
        let device = host
            .default_output_device()
            .ok_or(AudioError::DeviceUnavailable)?;
        let stream_config = StreamConfig {
            channels: source_channels as u16,
            sample_rate: source_sample_rate,
            buffer_size: BufferSize::Default,
        };

        info!(
            device = ?device.description().map(|description| description.name().to_owned()),
            sample_rate = source_sample_rate,
            channels = source_channels,
            "Building audio output stream"
        );

        let mut drain = AudioDrain::new(
            self.controls.clone(),
            source,
            audio_events,
            position_update_interval_ms,
        );
        let mut lock_miss = AudioLockMissLog::new();
        let (invalidated, invalidations) = mpsc::channel();

        let stream = device
            .build_output_stream(
                stream_config,
                move |data: &mut [f32], _: &cpal::OutputCallbackInfo| {
                    let _ = drain.drain_iteration(data, true, true, Some(&mut lock_miss));
                },
                stream_error_handler(invalidated),
                None,
            )
            .map_err(|error| match error.kind() {
                ErrorKind::DeviceNotAvailable | ErrorKind::DeviceBusy => {
                    AudioError::DeviceUnavailable
                }
                ErrorKind::UnsupportedConfig => AudioError::UnsupportedFormat {
                    sample_rate: source_sample_rate,
                    channels: source_channels,
                },
                _ => AudioError::Unexpected(format!("building the stream: {error}")),
            })?;

        Ok(Box::new(CpalStream {
            stream,
            invalidations,
        }))
    }

    fn controls(&self) -> &AudioOutputControls {
        &self.controls
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A stream cpal says must be rebuilt, or whose device went away, is
    /// reported to its owner, which builds a new one on the current default.
    /// A move cpal made itself, a glitch, or any other error is only logged:
    /// rebuilding for those would interrupt a stream that still plays.
    /// Sabotage: drop `DeviceNotAvailable` from the reported kinds, and a stream
    /// whose device was unplugged is never replaced.
    #[test]
    fn only_a_stream_that_cant_go_on_is_reported_to_its_owner() {
        let (invalidated, invalidations) = mpsc::channel();
        let mut handle = stream_error_handler(invalidated);

        for kind in [
            ErrorKind::DeviceChanged,
            ErrorKind::Xrun,
            ErrorKind::BackendError,
        ] {
            handle(kind.into());
        }
        assert!(invalidations.try_recv().is_err());

        handle(cpal::Error::with_message(
            ErrorKind::StreamInvalidated,
            "default device changed",
        ));
        handle(cpal::Error::with_message(
            ErrorKind::DeviceNotAvailable,
            "device disconnected",
        ));
        assert_eq!(
            invalidations.try_iter().collect::<Vec<_>>(),
            ["default device changed", "device disconnected"]
        );
    }
}
