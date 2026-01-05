use crate::playback::pcm_source::PcmSource;
use crate::playback::streaming_source::StreamingPcmSource;
use cpal::traits::{DeviceTrait, HostTrait};
use cpal::{Device, Stream, StreamConfig};
use std::fmt::{Display, Formatter, Result as FmtResult};
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::{mpsc, Mutex};
use std::sync::Arc;
use tracing::{error, info, trace, warn};
#[derive(Debug, Clone)]
pub enum AudioCommand {
    Play,
    Pause,
    Resume,
    Stop,
    SetVolume(f32),
}
#[derive(Debug)]
pub enum AudioError {
    DeviceNotFound,
    StreamConfigError(String),
    StreamBuildError(String),
}
impl Display for AudioError {
    fn fmt(&self, f: &mut Formatter<'_>) -> FmtResult {
        match self {
            AudioError::DeviceNotFound => write!(f, "Audio device not found"),
            AudioError::StreamConfigError(msg) => {
                write!(f, "Stream config error: {}", msg)
            }
            AudioError::StreamBuildError(msg) => write!(f, "Stream build error: {}", msg),
        }
    }
}
impl std::error::Error for AudioError {}
/// Audio output manager using CPAL
pub struct AudioOutput {
    device: Device,
    stream_config: StreamConfig,
    command_tx: mpsc::Sender<AudioCommand>,
    is_playing: Arc<AtomicBool>,
    is_paused: Arc<AtomicBool>,
    volume: Arc<AtomicU32>,
}
impl AudioOutput {
    /// Create a new audio output manager
    pub fn new() -> Result<Self, AudioError> {
        let host = cpal::default_host();
        let device = host
            .default_output_device()
            .ok_or(AudioError::DeviceNotFound)?;
        let default_config = device
            .default_output_config()
            .map_err(|e| AudioError::StreamConfigError(e.to_string()))?;
        let sample_format = default_config.sample_format();
        let stream_config = StreamConfig::from(default_config.clone());
        info!(
            "Audio device: {} channels, {} Hz, {:?}",
            stream_config.channels, stream_config.sample_rate.0, sample_format
        );
        let (command_tx, _command_rx) = mpsc::channel();
        let initial_volume = if std::env::var("SKIP_AUDIO_TESTS").is_ok()
            || std::env::var("MUTE_TEST_AUDIO").is_ok()
        {
            0u32
        } else {
            10000u32
        };
        Ok(Self {
            device,
            stream_config,
            command_tx,
            is_playing: Arc::new(AtomicBool::new(false)),
            is_paused: Arc::new(AtomicBool::new(false)),
            volume: Arc::new(AtomicU32::new(initial_volume)),
        })
    }
    /// Create a stream with PCM source
    pub fn create_stream(
        &mut self,
        source: Arc<PcmSource>,
        position_tx: mpsc::Sender<std::time::Duration>,
        completion_tx: mpsc::Sender<()>,
    ) -> Result<Stream, AudioError> {
        let output_sample_rate = self.stream_config.sample_rate.0;
        let output_channels = self.stream_config.channels as usize;
        let source_sample_rate = source.sample_rate();
        let source_channels = source.channels() as usize;
        let sample_rate_ratio = source_sample_rate as f64 / output_sample_rate as f64;
        let is_playing = self.is_playing.clone();
        let is_paused = self.is_paused.clone();
        let volume = self.volume.clone();
        let (command_tx_for_stream, command_rx) = mpsc::channel();
        self.command_tx = command_tx_for_stream;
        let mut resample_buffer: Vec<f32> = Vec::new();
        let mut resample_pos = 0usize;
        let mut last_position_update = std::time::Instant::now();
        let position_update_interval = std::time::Duration::from_millis(250);
        let stream = self
            .device
            .build_output_stream(
                &self.stream_config,
                move |data: &mut [f32], _: &cpal::OutputCallbackInfo| {
                    while let Ok(cmd) = command_rx.try_recv() {
                        match cmd {
                            AudioCommand::Play => {
                                is_playing.store(true, Ordering::Relaxed);
                                is_paused.store(false, Ordering::Relaxed);
                            }
                            AudioCommand::Pause => {
                                is_paused.store(true, Ordering::Relaxed);
                            }
                            AudioCommand::Resume => {
                                is_paused.store(false, Ordering::Relaxed);
                            }
                            AudioCommand::Stop => {
                                is_playing.store(false, Ordering::Relaxed);
                                is_paused.store(false, Ordering::Relaxed);
                            }
                            AudioCommand::SetVolume(vol) => {
                                volume
                                    .store(
                                        (vol.clamp(0.0, 1.0) * 10000.0) as u32,
                                        Ordering::Relaxed,
                                    );
                            }
                        }
                    }
                    if !is_playing.load(Ordering::Relaxed)
                        || is_paused.load(Ordering::Relaxed)
                    {
                        data.fill(0.0);
                        return;
                    }
                    let vol = volume.load(Ordering::Relaxed) as f32 / 10000.0;
                    let mut output_pos = 0;
                    while output_pos < data.len() {
                        if resample_pos >= resample_buffer.len() {
                            let samples_needed = (data.len() as f64 * sample_rate_ratio)
                                as usize + source_channels;
                            match source.next_samples(samples_needed) {
                                Some(samples) => {
                                    resample_buffer.clear();
                                    resample_pos = 0;
                                    let input_frames = samples.len() / source_channels;
                                    let converted = if sample_rate_ratio != 1.0 {
                                        let output_frames = (input_frames as f64
                                            / sample_rate_ratio) as usize;
                                        let mut resampled = Vec::with_capacity(
                                            output_frames * source_channels,
                                        );
                                        for frame_idx in 0..output_frames {
                                            let src_idx = (frame_idx as f64 * sample_rate_ratio)
                                                as usize;
                                            if src_idx < input_frames {
                                                for ch in 0..source_channels {
                                                    let idx = src_idx * source_channels + ch;
                                                    if idx < samples.len() {
                                                        resampled.push(samples[idx]);
                                                    } else {
                                                        resampled.push(0.0);
                                                    }
                                                }
                                            }
                                        }
                                        resampled
                                    } else {
                                        samples
                                    };
                                    let frames = converted.len() / source_channels;
                                    if source_channels != output_channels {
                                        for frame_idx in 0..frames {
                                            let base_idx = frame_idx * source_channels;
                                            if output_channels == 1 && source_channels >= 1 {
                                                resample_buffer.push(converted[base_idx]);
                                            } else if output_channels == 2 && source_channels == 1 {
                                                let sample = converted[base_idx];
                                                resample_buffer.push(sample);
                                                resample_buffer.push(sample);
                                            } else if output_channels == 2 && source_channels >= 2 {
                                                resample_buffer.push(converted[base_idx]);
                                                resample_buffer.push(converted[base_idx + 1]);
                                            } else {
                                                resample_buffer
                                                    .extend(std::iter::repeat_n(0.0, output_channels));
                                            }
                                        }
                                    } else {
                                        resample_buffer = converted;
                                    }
                                }
                                None => {
                                    info!("Audio callback: End of stream detected");
                                    is_playing.store(false, Ordering::Relaxed);
                                    if completion_tx.send(()).is_err() {
                                        warn!(
                                            "Failed to send completion signal - receiver may be dropped"
                                        );
                                    }
                                    data[output_pos..].fill(0.0);
                                    return;
                                }
                            }
                        }
                        while output_pos < data.len()
                            && resample_pos < resample_buffer.len()
                        {
                            data[output_pos] = resample_buffer[resample_pos] * vol;
                            output_pos += 1;
                            resample_pos += 1;
                        }
                    }
                    if last_position_update.elapsed() >= position_update_interval {
                        let _ = position_tx.send(source.position());
                        last_position_update = std::time::Instant::now();
                    }
                },
                |err| {
                    error!("Audio stream error: {:?}", err);
                },
                None,
            )
            .map_err(|e| AudioError::StreamBuildError(e.to_string()))?;
        Ok(stream)
    }
    /// Create a stream that pulls from a streaming PCM source (ring buffer).
    ///
    /// Unlike `create_stream`, this pulls f32 samples from a `StreamingPcmSource`
    /// which is fed by a decoder thread. Handles buffer underrun with silence.
    pub fn create_streaming_stream(
        &mut self,
        source: Arc<Mutex<StreamingPcmSource>>,
        source_sample_rate: u32,
        source_channels: u32,
        position_tx: mpsc::Sender<std::time::Duration>,
        completion_tx: mpsc::Sender<()>,
    ) -> Result<Stream, AudioError> {
        let output_sample_rate = self.stream_config.sample_rate.0;
        let output_channels = self.stream_config.channels as usize;
        let source_channels = source_channels as usize;
        let sample_rate_ratio = source_sample_rate as f64 / output_sample_rate as f64;

        let is_playing = self.is_playing.clone();
        let is_paused = self.is_paused.clone();
        let volume = self.volume.clone();

        let (command_tx_for_stream, command_rx) = mpsc::channel();
        self.command_tx = command_tx_for_stream;

        let mut resample_buffer: Vec<f32> = Vec::new();
        let mut resample_pos = 0usize;
        let mut last_position_update = std::time::Instant::now();
        let position_update_interval = std::time::Duration::from_millis(250);
        let mut completion_sent = false;

        let stream = self
            .device
            .build_output_stream(
                &self.stream_config,
                move |data: &mut [f32], _: &cpal::OutputCallbackInfo| {
                    // Process commands
                    while let Ok(cmd) = command_rx.try_recv() {
                        match cmd {
                            AudioCommand::Play => {
                                is_playing.store(true, Ordering::Relaxed);
                                is_paused.store(false, Ordering::Relaxed);
                            }
                            AudioCommand::Pause => {
                                is_paused.store(true, Ordering::Relaxed);
                            }
                            AudioCommand::Resume => {
                                is_paused.store(false, Ordering::Relaxed);
                            }
                            AudioCommand::Stop => {
                                is_playing.store(false, Ordering::Relaxed);
                                is_paused.store(false, Ordering::Relaxed);
                            }
                            AudioCommand::SetVolume(vol) => {
                                volume.store(
                                    (vol.clamp(0.0, 1.0) * 10000.0) as u32,
                                    Ordering::Relaxed,
                                );
                            }
                        }
                    }

                    if !is_playing.load(Ordering::Relaxed) || is_paused.load(Ordering::Relaxed) {
                        data.fill(0.0);
                        return;
                    }

                    let vol = volume.load(Ordering::Relaxed) as f32 / 10000.0;
                    let mut output_pos = 0;

                    // Try to lock the source (non-blocking in audio callback)
                    let mut source_guard = match source.try_lock() {
                        Ok(guard) => guard,
                        Err(_) => {
                            // Can't get lock, output silence
                            data.fill(0.0);
                            return;
                        }
                    };

                    while output_pos < data.len() {
                        if resample_pos >= resample_buffer.len() {
                            // Need more samples from source
                            let samples_needed =
                                (data.len() as f64 * sample_rate_ratio) as usize + source_channels;
                            let mut raw_samples = vec![0.0f32; samples_needed];
                            let read = source_guard.pull_samples(&mut raw_samples);

                            if read == 0 {
                                if source_guard.is_finished() {
                                    // End of stream
                                    if !completion_sent {
                                        info!("Streaming audio callback: End of stream");
                                        is_playing.store(false, Ordering::Relaxed);
                                        if completion_tx.send(()).is_err() {
                                            warn!("Failed to send completion signal");
                                        }
                                        completion_sent = true;
                                    }
                                    data[output_pos..].fill(0.0);
                                    return;
                                } else {
                                    // Buffer underrun - output silence and continue
                                    trace!("Streaming buffer underrun");
                                    data[output_pos..].fill(0.0);
                                    return;
                                }
                            }

                            raw_samples.truncate(read);
                            resample_buffer.clear();
                            resample_pos = 0;

                            let input_frames = raw_samples.len() / source_channels;

                            // Resample if needed
                            let converted = if sample_rate_ratio != 1.0 {
                                let output_frames =
                                    (input_frames as f64 / sample_rate_ratio) as usize;
                                let mut resampled =
                                    Vec::with_capacity(output_frames * source_channels);

                                for frame_idx in 0..output_frames {
                                    let src_idx =
                                        (frame_idx as f64 * sample_rate_ratio) as usize;
                                    if src_idx < input_frames {
                                        for ch in 0..source_channels {
                                            let idx = src_idx * source_channels + ch;
                                            if idx < raw_samples.len() {
                                                resampled.push(raw_samples[idx]);
                                            } else {
                                                resampled.push(0.0);
                                            }
                                        }
                                    }
                                }
                                resampled
                            } else {
                                raw_samples
                            };

                            // Channel conversion
                            let frames = converted.len() / source_channels;
                            if source_channels != output_channels {
                                for frame_idx in 0..frames {
                                    let base_idx = frame_idx * source_channels;
                                    if output_channels == 1 && source_channels >= 1 {
                                        resample_buffer.push(converted[base_idx]);
                                    } else if output_channels == 2 && source_channels == 1 {
                                        let sample = converted[base_idx];
                                        resample_buffer.push(sample);
                                        resample_buffer.push(sample);
                                    } else if output_channels == 2 && source_channels >= 2 {
                                        resample_buffer.push(converted[base_idx]);
                                        resample_buffer.push(converted[base_idx + 1]);
                                    } else {
                                        resample_buffer
                                            .extend(std::iter::repeat_n(0.0, output_channels));
                                    }
                                }
                            } else {
                                resample_buffer = converted;
                            }
                        }

                        // Copy from resample buffer to output
                        while output_pos < data.len() && resample_pos < resample_buffer.len() {
                            data[output_pos] = resample_buffer[resample_pos] * vol;
                            output_pos += 1;
                            resample_pos += 1;
                        }
                    }

                    // Position updates
                    if last_position_update.elapsed() >= position_update_interval {
                        let _ = position_tx.send(source_guard.position());
                        last_position_update = std::time::Instant::now();
                    }
                },
                |err| {
                    error!("Streaming audio error: {:?}", err);
                },
                None,
            )
            .map_err(|e| AudioError::StreamBuildError(e.to_string()))?;

        Ok(stream)
    }

    pub fn send_command(&self, cmd: AudioCommand) {
        let _ = self.command_tx.send(cmd);
    }

    pub fn set_volume(&self, volume: f32) {
        self.send_command(AudioCommand::SetVolume(volume));
    }
}
impl Default for AudioOutput {
    fn default() -> Self {
        Self::new().expect("Failed to initialize audio output")
    }
}
