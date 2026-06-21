use super::audio_output::{
    AudioError, AudioOutput, AudioState, AudioStream, CompletionEvent, PositionEvent,
};
use crate::playback::source::PlaybackSource;
use cpal::traits::{DeviceTrait, HostTrait};
use cpal::{SampleRate, Stream, StreamConfig};
use std::sync::atomic::{AtomicU32, AtomicU8, Ordering};
use std::sync::Arc;
use std::sync::{mpsc, Mutex};
use tracing::{debug, error, info, warn};

// -- cpal::Stream implements AudioStream --

impl AudioStream for Stream {
    fn play(&self) -> Result<(), AudioError> {
        cpal::traits::StreamTrait::play(self)
            .map_err(|e| AudioError::StreamBuildError(e.to_string()))
    }
}

/// Audio output using the system audio device via CPAL.
pub struct CpalAudioOutput {
    state: Arc<AtomicU8>,
    volume: Arc<AtomicU32>,
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
            state: Arc::new(AtomicU8::new(AudioState::Stopped as u8)),
            volume: Arc::new(AtomicU32::new(initial_volume)),
        })
    }
}

impl AudioOutput for CpalAudioOutput {
    fn create_stream(
        &mut self,
        source: Arc<Mutex<PlaybackSource>>,
        source_sample_rate: u32,
        source_channels: u32,
        position_tx: mpsc::Sender<PositionEvent>,
        completion_tx: mpsc::Sender<CompletionEvent>,
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

        let state = self.state.clone();
        let volume = self.volume.clone();

        let mut last_position_update = std::time::Instant::now();
        let position_update_interval =
            std::time::Duration::from_millis(position_update_interval_ms as u64);
        let mut completion_sent = false;

        let stream = device
            .build_output_stream(
                &stream_config,
                move |data: &mut [f32], _: &cpal::OutputCallbackInfo| {
                    if AudioState::from_u8(state.load(Ordering::Relaxed)) != AudioState::Playing {
                        data.fill(0.0);
                        return;
                    }

                    let vol = volume.load(Ordering::Relaxed) as f32 / 10000.0;

                    let mut source_guard = match source.try_lock() {
                        Ok(guard) => guard,
                        Err(_) => {
                            data.fill(0.0);
                            return;
                        }
                    };

                    let read = source_guard.pull_samples(data);

                    if read == 0 {
                        if source_guard.is_finished() && !completion_sent {
                            info!("Streaming audio callback: End of stream");
                            state.store(AudioState::Stopped as u8, Ordering::Relaxed);
                            if completion_tx.send(source_guard.completion_event()).is_err() {
                                warn!("Failed to send completion signal");
                            }
                            completion_sent = true;
                        }
                        data.fill(0.0);
                        return;
                    }

                    // Apply volume + this track's replay gain in-place as one
                    // combined factor; zero any unfilled tail. Mute (`vol == 0`)
                    // still zeroes the output. The peak cap was already applied
                    // when the gain was derived, so no clamp is needed here.
                    let combined = source_guard.current_replay_gain_linear() * vol;
                    for sample in &mut data[..read] {
                        *sample *= combined;
                    }
                    data[read..].fill(0.0);

                    if last_position_update.elapsed() >= position_update_interval {
                        // Position ticks are high-frequency (~20 Hz); a
                        // dropped receiver is expected during teardown. Log
                        // at debug so the masking is surfaced without flooding
                        // user-facing levels.
                        if position_tx.send(source_guard.position_event()).is_err() {
                            debug!("Position tick: receiver dropped");
                        }
                        last_position_update = std::time::Instant::now();
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
        self.state.store(new_state as u8, Ordering::Relaxed);
    }

    fn get_state(&self) -> AudioState {
        AudioState::from_u8(self.state.load(Ordering::Relaxed))
    }

    fn set_volume(&self, volume: f32) {
        self.volume
            .store((volume.clamp(0.0, 1.0) * 10000.0) as u32, Ordering::Relaxed);
    }

    fn get_volume(&self) -> f32 {
        self.volume.load(Ordering::Relaxed) as f32 / 10000.0
    }
}
