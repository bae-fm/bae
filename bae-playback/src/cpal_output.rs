//! The system output through cpal, on desktop and iOS.
//!
//! Every stream is built on the system's default output device, and follows a
//! change of it:
//!
//! - **macOS.** cpal plays a stream on the default device through CoreAudio's
//!   default output unit, which moves to the new default by itself and reports
//!   the move as `DeviceChanged`, but plays there at the new device's own rate.
//!   bae plays a track at its own rate (`output_rate`) and puts each device's
//!   format back (`device_formats`), so a move is an
//!   invalidation too: the stream is rebuilt on the new device, which cpal sets
//!   to the stream's rate, and the old device goes back to its format.
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
//! `DeviceNotAvailable`. These reports reach the stream's owner as an
//! invalidation ([`AudioStream::take_invalidation`]), and the output's
//! `InvalidationReport` tells the owner to ask: a new stream plays on the
//! current default, or fails to build when there is none.

use super::audio_output::{
    AudioDrain, AudioError, AudioEventSender, AudioLockMissLog, AudioOutput, AudioOutputControls,
    AudioStream, InvalidationReport,
};
#[cfg(target_os = "macos")]
use super::device_formats::{CoreAudioDeviceFormats, DeviceFormats, PlayerFormat, StreamFormat};
use super::resampling_drain::ResamplingDrain;
use crate::source::PlaybackSource;
use bae_util::task_failure::caught_sync;
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
    /// which hold the sending side of `invalidations`, and stops playing on
    /// the device before `_format` hands its lease back to the player.
    stream: cpal::Stream,
    invalidations: mpsc::Receiver<String>,
    #[cfg(target_os = "macos")]
    _format: StreamFormat<CoreAudioDeviceFormats>,
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
/// `invalidations`, for the stream's owner to act on, and `report` tells the
/// owner to look.
fn stream_error_handler(
    invalidations: mpsc::Sender<String>,
    report: InvalidationReport,
) -> impl FnMut(cpal::Error) + Send + 'static {
    move |error| match error.kind() {
        ErrorKind::StreamInvalidated | ErrorKind::DeviceNotAvailable => {
            report_invalidation(&invalidations, &report, error)
        }
        // The default output unit moved to the new device, which plays the
        // stream at its own rate: rebuild it there at the track's.
        #[cfg(target_os = "macos")]
        ErrorKind::DeviceChanged => report_invalidation(&invalidations, &report, error),
        #[cfg(not(target_os = "macos"))]
        ErrorKind::DeviceChanged => {
            info!("the output stream moved to the new default device: {error}");
        }
        ErrorKind::Xrun => warn!("the output stream ran short of samples: {error}"),
        kind => error!(?kind, "Streaming audio error: {error}"),
    }
}

fn report_invalidation(
    invalidations: &mpsc::Sender<String>,
    report: &InvalidationReport,
    error: cpal::Error,
) {
    if invalidations.send(error.to_string()).is_err() {
        // The owner dropped this stream's receiver while tearing the stream
        // down, so the stream is going away anyway.
        debug!("the output stream was invalidated as it was torn down: {error}");
        return;
    }
    report.report();
}

/// The rate to build the stream at for a track at `track_rate`. On macOS cpal
/// sets the output device to the stream's rate, so this is the track's own rate
/// when the device runs at it — nothing converts — and otherwise the supported
/// rate [`crate::output_rate::output_rate`] picks, which bae converts
/// the track to.
#[cfg(target_os = "macos")]
fn output_rate(device: &cpal::Device, track_rate: u32, channels: u32) -> Result<u32, AudioError> {
    let device_rates: Vec<std::ops::RangeInclusive<u32>> = device
        .supported_output_configs()
        .map_err(|error| match error.kind() {
            ErrorKind::DeviceNotAvailable | ErrorKind::DeviceBusy => AudioError::DeviceUnavailable,
            _ => AudioError::Unexpected(format!("reading the device's sample rates: {error}")),
        })?
        .map(|range| range.min_sample_rate()..=range.max_sample_rate())
        .collect();
    crate::output_rate::output_rate(track_rate, &device_rates)
        .ok_or(AudioError::UnsupportedFormat { sample_rate: track_rate, channels })
}

/// The rate to build the stream at for a track at `track_rate`: the track's
/// own. Off macOS the system converts a stream to the rate its device runs at:
/// WASAPI's shared mode on Windows (cpal opens it to convert any rate), the
/// sound server or ALSA's `plug` layer behind `default` on Linux, and the audio
/// session on iOS.
#[cfg(not(target_os = "macos"))]
fn output_rate(_device: &cpal::Device, track_rate: u32, _channels: u32) -> Result<u32, AudioError> {
    Ok(track_rate)
}

/// The default output device as cpal resolves it, with its CoreAudio id — the
/// device whose format cpal is about to change. The default is read before
/// and after cpal resolves it: when the two reads differ the default changed
/// under the build, and which device cpal resolved isn't known.
#[cfg(target_os = "macos")]
fn resolve_default_device(
    resolve: impl FnOnce() -> Result<cpal::Device, AudioError>,
) -> Result<(cpal::Device, objc2_core_audio::AudioDeviceID), AudioError> {
    let before = CoreAudioDeviceFormats::default_output_device();
    let device = resolve()?;
    let after = CoreAudioDeviceFormats::default_output_device();
    match (before, after) {
        (Some(before), Some(after)) if before == after => Ok((device, before)),
        (None, _) | (_, None) => Err(AudioError::DeviceUnavailable),
        (Some(before), Some(after)) => {
            warn!(
                before,
                after, "the default output device changed while a stream was being built"
            );
            Err(AudioError::DeviceUnavailable)
        }
    }
}

/// Audio output using the system audio device via CPAL.
pub struct CpalAudioOutput {
    /// Told when a stream's device reports the stream can't go on.
    report: InvalidationReport,
    /// The formats this output's streams, and every other output's on the
    /// same system device, set Mac output devices to.
    #[cfg(target_os = "macos")]
    device_formats: Arc<DeviceFormats<CoreAudioDeviceFormats>>,
    /// The lease of this output's last stream, kept until its playback ends.
    #[cfg(target_os = "macos")]
    player_format: PlayerFormat<CoreAudioDeviceFormats>,
}

impl CpalAudioOutput {
    /// Build a cpal output. Verifies a default output device exists up front so
    /// construction fails fast when there's no audio hardware. Which device the
    /// samples land on is resolved per stream build.
    pub(crate) fn new(
        report: InvalidationReport,
        #[cfg(target_os = "macos")] device_formats: Arc<DeviceFormats<CoreAudioDeviceFormats>>,
    ) -> Result<Self, AudioError> {
        let host = cpal::default_host();
        host.default_output_device().ok_or(AudioError::DeviceUnavailable)?;

        Ok(Self {
            report,
            #[cfg(target_os = "macos")]
            device_formats,
            #[cfg(target_os = "macos")]
            player_format: PlayerFormat::new(),
        })
    }
}

impl AudioOutput for CpalAudioOutput {
    fn create_stream(
        &mut self,
        controls: AudioOutputControls,
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
        let resolve =
            || cpal::default_host().default_output_device().ok_or(AudioError::DeviceUnavailable);
        #[cfg(target_os = "macos")]
        let (device, format_lease) = {
            let (device, device_id) = resolve_default_device(resolve)?;
            // Taken before cpal changes the device's format, so the end of
            // this playback puts it back.
            let lease = self.device_formats.lease(device_id).map_err(|error| {
                AudioError::Unexpected(format!("reading the output device's format: {error}"))
            })?;
            (device, lease)
        };
        #[cfg(not(target_os = "macos"))]
        let device = resolve()?;
        let output_rate = output_rate(&device, source_sample_rate, source_channels)?;
        let stream_config = StreamConfig {
            channels: source_channels as u16,
            sample_rate: output_rate,
            buffer_size: BufferSize::Default,
        };

        info!(
            device = ?device.description().map(|description| description.name().to_owned()),
            sample_rate = source_sample_rate,
            output_rate,
            channels = source_channels,
            "Building audio output stream"
        );

        let mut converter = ResamplingDrain::new(
            AudioDrain::new(controls.clone(), source, audio_events, position_update_interval_ms),
            controls,
            (source_sample_rate, source_channels),
            (output_rate, source_channels),
            Some(AudioLockMissLog::new()),
            "the output device",
        )
        .map_err(|error| {
            warn!(
                "no converter from {source_sample_rate} Hz to the output device's \
                 {output_rate} Hz: {error}"
            );
            AudioError::UnsupportedFormat {
                sample_rate: source_sample_rate,
                channels: source_channels,
            }
        })?;
        let (invalidated, invalidations) = mpsc::channel();

        let stream = device
            .build_output_stream(
                stream_config,
                move |data: &mut [f32], _: &cpal::OutputCallbackInfo| {
                    // A panic unwinding into the audio system's own frames
                    // would abort the process (CoreAudio) or end cpal's thread
                    // unseen (WASAPI, ALSA). It fails the output instead,
                    // which playback reports and stops on.
                    let written =
                        caught_sync("fill the output device's buffer", || converter.read(data))
                            .unwrap_or_else(|failed| {
                                converter.panicked(failed);
                                0
                            });
                    data[written..].fill(0.0);
                },
                stream_error_handler(invalidated, self.report.clone()),
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

        #[cfg(target_os = "macos")]
        format_lease.settle().map_err(|error| {
            AudioError::Unexpected(format!("reading the output device's new format: {error}"))
        })?;

        Ok(Box::new(CpalStream {
            stream,
            invalidations,
            #[cfg(target_os = "macos")]
            _format: self.player_format.stream_started(format_lease),
        }))
    }

    /// Let the lease kept from the last stream go, putting the device back to
    /// the format it had before bae, unless another player's stream plays on
    /// it.
    #[cfg(target_os = "macos")]
    fn playback_ended(&mut self) {
        self.player_format.playback_ended();
    }
}

#[cfg(test)]
#[path = "cpal_output_tests.rs"]
mod tests;
