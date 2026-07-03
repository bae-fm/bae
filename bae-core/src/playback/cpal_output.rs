use super::audio_output::{
    AudioDrain, AudioError, AudioLockMissLog, AudioOutput, AudioOutputControls, AudioState,
    AudioStream, CompletionEvent, DrainStatus, PositionEvent,
};
use crate::playback::source::PlaybackSource;
use cpal::traits::{DeviceTrait, HostTrait};
use cpal::{SampleRate, Stream, StreamConfig};
use std::sync::Arc;
use std::sync::Mutex;
use tokio::sync::mpsc as tokio_mpsc;
use tracing::{error, info};

// -- cpal::Stream implements AudioStream --

impl AudioStream for Stream {
    fn play(&self) -> Result<(), AudioError> {
        cpal::traits::StreamTrait::play(self)
            .map_err(|e| AudioError::StreamBuildError(e.to_string()))
    }
}

/// Audio output using the system audio device via CPAL.
pub struct CpalAudioOutput {
    controls: AudioOutputControls,
}

impl CpalAudioOutput {
    pub fn new() -> Result<Self, AudioError> {
        // Verify a default output device exists at startup so construction
        // fails fast when there's no audio hardware. The device is
        // deliberately not cached here: each stream binds to whatever the
        // system default output device is at build time (see `create_stream`),
        // so playback follows the user's current output selection across track
        // changes instead of pinning to the startup default.
        let host = cpal::default_host();
        host.default_output_device()
            .ok_or(AudioError::DeviceNotFound)?;

        let initial_volume = if std::env::var("SKIP_AUDIO_TESTS").is_ok()
            || std::env::var("MUTE_TEST_AUDIO").is_ok()
        {
            0u32
        } else {
            10000u32
        };
        Ok(Self {
            controls: AudioOutputControls::new(initial_volume),
        })
    }
}

impl AudioOutput for CpalAudioOutput {
    fn create_stream(
        &mut self,
        source: Arc<Mutex<PlaybackSource>>,
        source_sample_rate: u32,
        source_channels: u32,
        position_tx: tokio_mpsc::UnboundedSender<PositionEvent>,
        completion_tx: tokio_mpsc::UnboundedSender<CompletionEvent>,
        position_update_interval_ms: u32,
    ) -> Result<Box<dyn AudioStream>, AudioError> {
        // Resolve the current default output device on every build so playback
        // follows the user's current output selection. Caching the device at
        // construction pins to whatever was default at startup: on macOS cpal
        // sets the audio unit's `CurrentDevice` to that cached id on each
        // (re)build, so after a track change the new stream binds to the stale
        // device instead of the current default — e.g. reverting to built-in
        // speakers while an external display's speakers stay selected.
        let host = cpal::default_host();
        let device = host
            .default_output_device()
            .ok_or(AudioError::DeviceNotFound)?;
        let default_config = device
            .default_output_config()
            .map_err(|e| AudioError::StreamConfigError(e.to_string()))?;
        let mut stream_config = StreamConfig::from(default_config);
        stream_config.sample_rate = SampleRate(source_sample_rate);
        stream_config.channels = source_channels as u16;

        info!(
            device = ?device.name(),
            sample_rate = source_sample_rate,
            channels = source_channels,
            "Building audio output stream"
        );

        let mut drain = AudioDrain::new(
            self.controls.clone(),
            source,
            position_tx,
            completion_tx,
            position_update_interval_ms,
        );
        let mut lock_miss = AudioLockMissLog::new();

        let stream = device
            .build_output_stream(
                &stream_config,
                move |data: &mut [f32], _: &cpal::OutputCallbackInfo| {
                    if matches!(
                        drain.drain_iteration(data, true, true, Some(&mut lock_miss)),
                        DrainStatus::Completed
                    ) {
                        info!("Streaming audio callback: End of stream");
                    }
                },
                |err| {
                    error!("Streaming audio error: {:?}", err);
                },
                None,
            )
            .map_err(|e| AudioError::StreamBuildError(e.to_string()))?;

        Ok(Box::new(stream))
    }

    fn set_state(&self, new_state: AudioState) {
        self.controls.set_state(new_state);
    }

    fn get_state(&self) -> AudioState {
        self.controls.get_state()
    }

    fn set_volume(&self, volume: f32) {
        self.controls.set_volume(volume);
    }

    fn get_volume(&self) -> f32 {
        self.controls.get_volume()
    }
}
