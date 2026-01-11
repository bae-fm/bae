//! # Playback Service
//!
//! The playback service manages audio playback through a command-based architecture.
//! It runs in its own thread and processes commands from a channel.
//!
//! ## Audio State
//!
//! Audio state is a shared atomic (`AudioState` enum: `Stopped`, `Playing`, `Paused`).
//! The audio callback reads it on every iteration and outputs samples if `Playing`,
//! silence otherwise. Infrastructure (streams, buffers, decoders) is set up
//! separately via `init_streaming()`.
//!
//! ## Seek Flow
//!
//! 1. Cancel old streaming source (makes callback output silence)
//! 2. Create new seek buffer (local: fresh file reader, cloud: fresh range request)
//! 3. Spawn decoder on seek buffer
//! 4. Wait for buffer to be 50% full
//! 5. Call `init_streaming()` which drops old stream and creates new one
//! 6. State remains unchanged (Playing or Paused) - new stream inherits it
//! 7. Send `Seeked` progress event

use crate::cloud_storage::CloudStorage;
use crate::db::DbTrack;
use crate::encryption::EncryptionService;
use crate::library::LibraryManager;
use crate::playback::cpal_output::AudioOutput;
use crate::playback::data_source::{
    AudioDataReader, AudioReadConfig, CloudStorageReader, LocalFileReader,
};
use crate::playback::error::PlaybackError;
use crate::playback::progress::{PlaybackProgress, PlaybackProgressHandle};
use crate::playback::sparse_buffer::{create_sparse_buffer, SharedSparseBuffer};
use crate::playback::{create_streaming_pair, StreamingPcmSource};
use crate::storage::create_storage_reader;
use cpal::traits::StreamTrait;
use std::collections::VecDeque;
use std::sync::{mpsc, Arc, Mutex};
use tokio::sync::mpsc as tokio_mpsc;
use tracing::{error, info, trace};
/// Playback commands sent to the service
#[derive(Debug, Clone)]
pub enum PlaybackCommand {
    Play(String),
    PlayAlbum(Vec<String>),
    Pause,
    Resume,
    Stop,
    /// Manual next track (skip pregap)
    Next,
    /// Auto-advance from track completion (play pregap)
    AutoAdvance,
    Previous,
    Seek(std::time::Duration),
    SetVolume(f32),
    AddToQueue(Vec<String>),
    AddNext(Vec<String>),
    RemoveFromQueue(usize),
    ReorderQueue {
        from: usize,
        to: usize,
    },
    ClearQueue,
    GetQueue,
}
/// Current playback state
#[derive(Debug, Clone)]
pub enum PlaybackState {
    Stopped,
    Playing {
        track: DbTrack,
        position: std::time::Duration,
        /// Expected duration from database metadata
        duration: Option<std::time::Duration>,
        /// Actual duration of decoded audio (may differ for CUE/FLAC if byte extraction fails)
        decoded_duration: std::time::Duration,
        /// Pre-gap duration in ms (for CUE/FLAC tracks with INDEX 00)
        /// UI should show negative time when position < pregap_ms
        pregap_ms: Option<i64>,
    },
    Paused {
        track: DbTrack,
        position: std::time::Duration,
        duration: Option<std::time::Duration>,
        decoded_duration: std::time::Duration,
        /// Pre-gap duration in ms (for CUE/FLAC tracks with INDEX 00)
        pregap_ms: Option<i64>,
    },
    Loading {
        track_id: String,
    },
}
/// Handle to the playback service for sending commands
#[derive(Clone)]
pub struct PlaybackHandle {
    command_tx: tokio_mpsc::UnboundedSender<PlaybackCommand>,
    progress_handle: PlaybackProgressHandle,
}
impl PlaybackHandle {
    pub fn play(&self, track_id: String) {
        let _ = self.command_tx.send(PlaybackCommand::Play(track_id));
    }
    pub fn play_album(&self, track_ids: Vec<String>) {
        let _ = self.command_tx.send(PlaybackCommand::PlayAlbum(track_ids));
    }
    pub fn pause(&self) {
        let _ = self.command_tx.send(PlaybackCommand::Pause);
    }
    pub fn resume(&self) {
        let _ = self.command_tx.send(PlaybackCommand::Resume);
    }
    pub fn stop(&self) {
        let _ = self.command_tx.send(PlaybackCommand::Stop);
    }
    pub fn next(&self) {
        let _ = self.command_tx.send(PlaybackCommand::Next);
    }
    pub fn previous(&self) {
        let _ = self.command_tx.send(PlaybackCommand::Previous);
    }
    pub fn seek(&self, position: std::time::Duration) {
        let _ = self.command_tx.send(PlaybackCommand::Seek(position));
    }
    pub fn set_volume(&self, volume: f32) {
        let _ = self.command_tx.send(PlaybackCommand::SetVolume(volume));
    }
    pub async fn get_state(&self) -> PlaybackState {
        PlaybackState::Stopped
    }
    pub fn subscribe_progress(&self) -> tokio_mpsc::UnboundedReceiver<PlaybackProgress> {
        self.progress_handle.subscribe_all()
    }
    pub fn add_to_queue(&self, track_ids: Vec<String>) {
        let _ = self.command_tx.send(PlaybackCommand::AddToQueue(track_ids));
    }
    pub fn add_next(&self, track_ids: Vec<String>) {
        let _ = self.command_tx.send(PlaybackCommand::AddNext(track_ids));
    }
    pub fn remove_from_queue(&self, index: usize) {
        let _ = self
            .command_tx
            .send(PlaybackCommand::RemoveFromQueue(index));
    }
    pub fn reorder_queue(&self, from: usize, to: usize) {
        let _ = self
            .command_tx
            .send(PlaybackCommand::ReorderQueue { from, to });
    }
    pub fn clear_queue(&self) {
        let _ = self.command_tx.send(PlaybackCommand::ClearQueue);
    }
    pub fn get_queue(&self) {
        let _ = self.command_tx.send(PlaybackCommand::GetQueue);
    }
}

/// Prepared track data for playback.
/// Contains all metadata and buffer state needed to start decoding from any position.
struct PreparedTrack {
    track: DbTrack,
    /// Raw audio buffer (may have headers prepended for CUE/FLAC)
    buffer: SharedSparseBuffer,
    /// FLAC headers for seek support (prepended when restarting decoder)
    flac_headers: Option<Vec<u8>>,
    /// Dense seektable for frame-accurate seeking (JSON array of {sample, byte} entries)
    seektable_json: String,
    /// Sample rate in Hz for time-to-sample conversion
    sample_rate: u32,
    /// Byte offset in buffer where audio data starts (after headers)
    audio_data_start: u64,
    /// Total size of audio data in buffer (for seek calculations)
    file_size: u64,
    /// Source path for re-reading on seek past buffer
    source_path: String,
    /// Pre-gap duration in ms (for CUE/FLAC tracks)
    pregap_ms: Option<i64>,
    /// Track duration from metadata
    duration: std::time::Duration,
    /// True if this track uses local file storage (fast seek via direct file read)
    is_local_storage: bool,
    /// For CUE/FLAC: track's start byte position in original file.
    /// Used to convert buffer-relative seek position to file-absolute position.
    track_start_byte_offset: Option<u64>,
    /// For CUE/FLAC: track's end byte position in original file.
    /// Used to limit reading when seeking so track doesn't play into next track.
    track_end_byte_offset: Option<u64>,
    /// Cloud storage instance for creating new readers on seek (None for local files)
    cloud_storage: Option<Arc<dyn CloudStorage>>,
    /// Whether cloud storage is encrypted
    cloud_encrypted: bool,
    /// Encryption nonce (24 bytes) for efficient encrypted range requests.
    /// Stored in DB at import time, used during seek to avoid fetching nonce from cloud.
    encryption_nonce: Option<Vec<u8>>,
}

/// Fetch track metadata, create buffer, start reading audio data.
/// This is the common preparation logic used by both play_track and preload_next_track.
async fn prepare_track(
    library_manager: &LibraryManager,
    encryption_service: Option<&EncryptionService>,
    track_id: &str,
) -> Result<PreparedTrack, PlaybackError> {
    let track = library_manager
        .get_track(track_id)
        .await
        .map_err(PlaybackError::database)?
        .ok_or_else(|| PlaybackError::not_found("Track", track_id))?;

    let storage_profile = library_manager
        .get_storage_profile_for_release(&track.release_id)
        .await
        .map_err(PlaybackError::database)?;

    let audio_format = library_manager
        .get_audio_format_by_track_id(track_id)
        .await
        .map_err(PlaybackError::database)?
        .ok_or_else(|| PlaybackError::not_found("Audio format", track_id))?;

    let file_id = audio_format
        .file_id
        .as_ref()
        .ok_or_else(|| PlaybackError::not_found("file_id in audio_format", track_id))?;

    let audio_file = library_manager
        .get_file_by_id(file_id)
        .await
        .map_err(PlaybackError::database)?
        .ok_or_else(|| PlaybackError::not_found("Audio file", file_id))?;

    let source_path = audio_file
        .source_path
        .ok_or_else(|| PlaybackError::not_found("source_path", track_id))?;

    let pregap_ms = audio_format.pregap_ms;

    let (start_byte, end_byte) =
        match (audio_format.start_byte_offset, audio_format.end_byte_offset) {
            (Some(s), Some(e)) => (Some(s as u64), Some(e as u64)),
            _ => (None, None),
        };

    // Load all headers (for seek support)
    let all_flac_headers = audio_format.flac_headers.clone();

    // Headers to prepend during playback (only for CUE/FLAC where buffer doesn't have them)
    let needs_headers = audio_format.needs_headers;
    let flac_headers = if needs_headers {
        all_flac_headers.clone()
    } else {
        None
    };

    let file_size = if let (Some(start), Some(end)) = (start_byte, end_byte) {
        end - start
    } else {
        audio_file.file_size as u64
    };
    let headers_len = flac_headers.as_ref().map(|h| h.len() as u64).unwrap_or(0);

    // Create sparse buffer for streaming
    let buffer = create_sparse_buffer();

    let read_config = AudioReadConfig {
        path: source_path.clone(),
        flac_headers: flac_headers.clone(),
        start_byte,
        end_byte,
    };

    // Create appropriate data reader based on storage profile
    // Also capture cloud storage info for seek support
    type ReaderInfo = (
        Box<dyn AudioDataReader>,
        bool,
        Option<Arc<dyn CloudStorage>>,
        bool,
    );
    let (reader, is_local_storage, cloud_storage, cloud_encrypted): ReaderInfo =
        match &storage_profile {
            None => (
                Box::new(LocalFileReader::new(read_config)),
                true,
                None,
                false,
            ),
            Some(profile)
                if !profile.encrypted && profile.location == crate::db::StorageLocation::Local =>
            {
                (
                    Box::new(LocalFileReader::new(read_config)),
                    true,
                    None,
                    false,
                )
            }
            Some(profile) => {
                let storage = create_storage_reader(profile)
                    .await
                    .map_err(PlaybackError::cloud)?;
                let encrypted = profile.encrypted;
                (
                    Box::new(CloudStorageReader::new(
                        read_config,
                        storage.clone(),
                        encryption_service.map(|e| Arc::new(e.clone())),
                        encrypted,
                    )),
                    false,
                    Some(storage),
                    encrypted,
                )
            }
        };

    // Start reading data into buffer
    reader.start_reading(buffer.clone());

    // Determine audio_data_start for seek calculations
    let audio_data_start = if needs_headers {
        headers_len
    } else {
        audio_format.audio_data_start as u64
    };

    let duration = track
        .duration_ms
        .map(|ms| std::time::Duration::from_millis(ms as u64))
        .unwrap_or(std::time::Duration::from_secs(300));

    Ok(PreparedTrack {
        track,
        buffer,
        flac_headers: all_flac_headers,
        seektable_json: audio_format.seektable_json.clone(),
        sample_rate: audio_format.sample_rate as u32,
        audio_data_start,
        file_size: file_size + headers_len,
        source_path,
        pregap_ms,
        duration,
        is_local_storage,
        track_start_byte_offset: start_byte,
        track_end_byte_offset: end_byte,
        cloud_storage,
        cloud_encrypted,
        encryption_nonce: audio_file.encryption_nonce,
    })
}

/// Playback service that manages audio playback
pub struct PlaybackService {
    library_manager: LibraryManager,
    encryption_service: Option<EncryptionService>,
    command_rx: tokio_mpsc::UnboundedReceiver<PlaybackCommand>,
    progress_tx: tokio_mpsc::UnboundedSender<PlaybackProgress>,
    queue: VecDeque<String>,
    previous_track_id: Option<String>,
    current_position_shared: Arc<std::sync::Mutex<Option<std::time::Duration>>>,
    /// Generation counter to invalidate old position listeners after seek
    position_generation: Arc<std::sync::atomic::AtomicU64>,
    audio_output: AudioOutput,
    stream: Option<cpal::Stream>,
    /// Current track prepared data and streaming state
    current_prepared: Option<PreparedTrack>,
    /// Current streaming source (decoder output)
    current_streaming_source: Option<Arc<Mutex<StreamingPcmSource>>>,
    /// Preloaded next track prepared data
    next_prepared: Option<PreparedTrack>,
    /// Preloaded next track streaming source (decoder already started)
    next_streaming_source: Option<Arc<Mutex<StreamingPcmSource>>>,
}

impl PlaybackService {
    // Helper accessors for current/next track state
    fn current_track_id(&self) -> Option<&str> {
        self.current_prepared.as_ref().map(|p| p.track.id.as_str())
    }

    fn next_track_id(&self) -> Option<&str> {
        self.next_prepared.as_ref().map(|p| p.track.id.as_str())
    }

    /// Initialize streaming infrastructure without changing audio state.
    ///
    /// Sets up the cpal stream, position listeners, and completion handlers.
    /// The audio output state remains unchanged - caller must explicitly
    /// call `audio_output.set_state(Playing)` to start audio output.
    ///
    /// Returns true if initialization succeeded, false on error.
    async fn init_streaming(
        &mut self,
        source: Arc<Mutex<StreamingPcmSource>>,
        position_offset: std::time::Duration,
        track_id: String,
    ) -> bool {
        let (source_sample_rate, source_channels) = {
            let guard = source.lock().unwrap();
            (guard.sample_rate(), guard.channels())
        };

        // Drop old stream first
        if let Some(stream) = self.stream.take() {
            drop(stream);
        }

        // Create channels for position updates and completion notification
        let (position_tx, position_rx) = mpsc::channel();
        let (completion_tx, completion_rx) = mpsc::channel();
        let (position_tx_async, mut position_rx_async) = tokio_mpsc::unbounded_channel();
        let (completion_tx_async, mut completion_rx_async) = tokio_mpsc::unbounded_channel();

        // Bridge sync channels to async
        tokio::spawn({
            let position_rx = Arc::new(std::sync::Mutex::new(position_rx));
            async move {
                loop {
                    let rx = position_rx.clone();
                    match tokio::task::spawn_blocking(move || rx.lock().unwrap().recv()).await {
                        Ok(Ok(pos)) => {
                            let _ = position_tx_async.send(pos);
                        }
                        _ => break,
                    }
                }
            }
        });

        tokio::spawn({
            let completion_rx = Arc::new(std::sync::Mutex::new(completion_rx));
            async move {
                loop {
                    let rx = completion_rx.clone();
                    match tokio::task::spawn_blocking(move || rx.lock().unwrap().recv()).await {
                        Ok(Ok(())) => {
                            let _ = completion_tx_async.send(());
                        }
                        _ => break,
                    }
                }
            }
        });

        // Create streaming audio output
        let stream = match self.audio_output.create_stream(
            source.clone(),
            source_sample_rate,
            source_channels,
            position_tx,
            completion_tx,
        ) {
            Ok(stream) => stream,
            Err(e) => {
                error!("Failed to create streaming audio stream: {:?}", e);
                return false;
            }
        };

        if let Err(e) = stream.play() {
            error!("Failed to start streaming playback: {:?}", e);
            return false;
        }

        // Update state
        self.stream = Some(stream);
        self.current_streaming_source = Some(source.clone());
        *self.current_position_shared.lock().unwrap() = Some(position_offset);

        // Spawn position/completion listener
        let progress_tx = self.progress_tx.clone();
        let current_position_shared = self.current_position_shared.clone();
        let position_generation = self.position_generation.clone();
        let gen = position_generation.load(std::sync::atomic::Ordering::SeqCst);
        let streaming_source = Some(source);

        tokio::spawn(async move {
            loop {
                tokio::select! {
                    Some(pos) = position_rx_async.recv() => {
                        if position_generation.load(std::sync::atomic::Ordering::SeqCst) == gen {
                            // Add offset to convert decoder-relative to track-relative position
                            let actual_pos = position_offset + pos;
                            *current_position_shared.lock().unwrap() = Some(actual_pos);
                            let _ = progress_tx.send(PlaybackProgress::PositionUpdate {
                                position: actual_pos,
                                track_id: track_id.clone(),
                            });
                        }
                    }
                    Some(()) = completion_rx_async.recv() => {
                        if position_generation.load(std::sync::atomic::Ordering::SeqCst) == gen {
                            let (error_count, samples_decoded) = streaming_source
                                .as_ref()
                                .and_then(|s| s.lock().ok())
                                .map(|g| (g.decode_error_count(), g.samples_decoded()))
                                .unwrap_or((0, 0));

                            info!("Track completed: {} ({} decode errors, {} samples)", track_id, error_count, samples_decoded);
                            let _ = progress_tx.send(PlaybackProgress::TrackCompleted {
                                track_id: track_id.clone(),
                            });
                            let _ = progress_tx.send(PlaybackProgress::DecodeStats {
                                track_id: track_id.clone(),
                                error_count,
                                samples_decoded,
                            });
                        }
                        break;
                    }
                    else => break,
                }
            }
        });

        true
    }

    pub fn start(
        library_manager: LibraryManager,
        encryption_service: Option<EncryptionService>,
        runtime_handle: tokio::runtime::Handle,
    ) -> PlaybackHandle {
        let (command_tx, command_rx) = tokio_mpsc::unbounded_channel();
        let (progress_tx, progress_rx) = tokio_mpsc::unbounded_channel();
        let progress_handle = PlaybackProgressHandle::new(progress_rx, runtime_handle.clone());
        let handle = PlaybackHandle {
            command_tx: command_tx.clone(),
            progress_handle: progress_handle.clone(),
        };
        let command_tx_for_completion = command_tx.clone();
        let progress_handle_for_completion = progress_handle.clone();
        runtime_handle.spawn(async move {
            let mut progress_rx = progress_handle_for_completion.subscribe_all();
            while let Some(progress) = progress_rx.recv().await {
                if let PlaybackProgress::TrackCompleted { track_id } = progress {
                    info!(
                        "Auto-advance: Track completed, sending AutoAdvance command: {}",
                        track_id
                    );
                    let _ = command_tx_for_completion.send(PlaybackCommand::AutoAdvance);
                }
            }
        });
        std::thread::spawn(move || {
            let rt = tokio::runtime::Runtime::new().expect("Failed to create runtime");
            rt.block_on(async move {
                let audio_output = match AudioOutput::new() {
                    Ok(output) => output,
                    Err(e) => {
                        error!("Failed to initialize audio output: {:?}", e);
                        return;
                    }
                };
                let mut service = PlaybackService {
                    library_manager,
                    encryption_service,
                    command_rx,
                    progress_tx,
                    queue: VecDeque::new(),
                    previous_track_id: None,
                    current_position_shared: Arc::new(std::sync::Mutex::new(None)),
                    position_generation: Arc::new(std::sync::atomic::AtomicU64::new(0)),
                    audio_output,
                    stream: None,
                    current_prepared: None,
                    current_streaming_source: None,
                    next_prepared: None,
                    next_streaming_source: None,
                };
                service.run().await;
            });
        });
        handle
    }
    async fn run(&mut self) {
        info!("PlaybackService started");
        while let Some(command) = self.command_rx.recv().await {
            match command {
                PlaybackCommand::Play(track_id) => {
                    if let Some(stream) = self.stream.take() {
                        drop(stream);
                    }
                    self.audio_output
                        .set_state(crate::playback::cpal_output::AudioState::Stopped);
                    self.clear_next_track_state();
                    if let Some(id) = self.current_track_id() {
                        self.previous_track_id = Some(id.to_string());
                    }
                    self.queue.clear();
                    self.emit_queue_update();
                    if let Ok(Some(track)) = self.library_manager.get_track(&track_id).await {
                        if let Ok(mut release_tracks) =
                            self.library_manager.get_tracks(&track.release_id).await
                        {
                            release_tracks.sort_by(|a, b| {
                                let disc_cmp = match (a.disc_number, b.disc_number) {
                                    (Some(a_disc), Some(b_disc)) => a_disc.cmp(&b_disc),
                                    (Some(_), None) => std::cmp::Ordering::Less,
                                    (None, Some(_)) => std::cmp::Ordering::Greater,
                                    (None, None) => std::cmp::Ordering::Equal,
                                };
                                if disc_cmp == std::cmp::Ordering::Equal {
                                    match (a.track_number, b.track_number) {
                                        (Some(a_num), Some(b_num)) => a_num.cmp(&b_num),
                                        (Some(_), None) => std::cmp::Ordering::Less,
                                        (None, Some(_)) => std::cmp::Ordering::Greater,
                                        (None, None) => std::cmp::Ordering::Equal,
                                    }
                                } else {
                                    disc_cmp
                                }
                            });
                            if self.previous_track_id.is_none() {
                                let mut previous_track_id = None;
                                for release_track in &release_tracks {
                                    if release_track.id == track_id {
                                        break;
                                    }
                                    previous_track_id = Some(release_track.id.clone());
                                }
                                self.previous_track_id = previous_track_id;
                            }
                            let mut found_current = false;
                            for release_track in release_tracks {
                                if found_current {
                                    self.queue.push_back(release_track.id);
                                } else if release_track.id == track_id {
                                    found_current = true;
                                }
                            }
                            self.emit_queue_update();
                        }
                    }
                    self.play_track(&track_id, false, false).await; // Direct selection: skip pregap, start playing
                }
                PlaybackCommand::PlayAlbum(track_ids) => {
                    if let Some(id) = self.current_track_id() {
                        self.previous_track_id = Some(id.to_string());
                    }
                    self.queue.clear();
                    for track_id in track_ids {
                        self.queue.push_back(track_id);
                    }
                    if let Some(first_track) = self.queue.pop_front() {
                        self.emit_queue_update();
                        self.play_track(&first_track, false, false).await; // Direct selection: skip pregap, start playing
                    } else {
                        self.emit_queue_update();
                    }
                }
                PlaybackCommand::Pause => {
                    self.pause().await;
                }
                PlaybackCommand::Resume => {
                    self.resume().await;
                }
                PlaybackCommand::Stop => {
                    self.stop().await;
                }
                PlaybackCommand::Next => {
                    info!("Next command received, queue length: {}", self.queue.len());
                    if let Some(preloaded_track_id) = self.next_track_id().map(|s| s.to_string()) {
                        if self.next_streaming_source.is_some() {
                            info!("Using preloaded track: {}", preloaded_track_id);
                            if let Some(id) = self.current_track_id() {
                                self.previous_track_id = Some(id.to_string());
                            }
                            if self
                                .queue
                                .front()
                                .map(|id| id == &preloaded_track_id)
                                .unwrap_or(false)
                            {
                                self.queue.pop_front();
                                self.emit_queue_update();
                            }
                            self.play_preloaded_track(false, true).await; // skip pregap, preserve paused
                        } else {
                            // Preload started but streaming source not ready yet
                            self.clear_next_track_state();
                            self.play_track(&preloaded_track_id, false, true).await;
                            // preserve paused
                        }
                    } else if let Some(next_track) = self.queue.pop_front() {
                        info!("No preloaded track, playing from queue: {}", next_track);
                        self.emit_queue_update();
                        if let Some(id) = self.current_track_id() {
                            self.previous_track_id = Some(id.to_string());
                        }
                        self.play_track(&next_track, false, true).await; // preserve paused
                    } else {
                        info!("No next track available, stopping");
                        self.emit_queue_update();
                        self.stop().await;
                    }
                }
                PlaybackCommand::AutoAdvance => {
                    info!(
                        "AutoAdvance command received (natural transition), queue length: {}",
                        self.queue.len()
                    );
                    if let Some(preloaded_track_id) = self.next_track_id().map(|s| s.to_string()) {
                        if self.next_streaming_source.is_some() {
                            info!("Using preloaded track: {}", preloaded_track_id);
                            if let Some(id) = self.current_track_id() {
                                self.previous_track_id = Some(id.to_string());
                            }
                            if self
                                .queue
                                .front()
                                .map(|id| id == &preloaded_track_id)
                                .unwrap_or(false)
                            {
                                self.queue.pop_front();
                                self.emit_queue_update();
                            }
                            self.play_preloaded_track(true, false).await; // natural transition, start playing
                        } else {
                            // Preload started but streaming source not ready yet
                            self.clear_next_track_state();
                            self.play_track(&preloaded_track_id, true, false).await;
                            // start playing
                        }
                    } else if let Some(next_track) = self.queue.pop_front() {
                        info!("No preloaded track, playing from queue: {}", next_track);
                        self.emit_queue_update();
                        if let Some(id) = self.current_track_id() {
                            self.previous_track_id = Some(id.to_string());
                        }
                        self.play_track(&next_track, true, false).await; // start playing
                    } else {
                        info!("No next track available, stopping");
                        self.emit_queue_update();
                        self.stop().await;
                    }
                }
                PlaybackCommand::Previous => {
                    if let Some(current_track_id) = self.current_track_id().map(|s| s.to_string()) {
                        let current_position = self
                            .current_position_shared
                            .lock()
                            .unwrap()
                            .unwrap_or(std::time::Duration::ZERO);
                        if current_position < std::time::Duration::from_secs(3) {
                            if let Some(previous_track_id) = self.previous_track_id.clone() {
                                info!("Going to previous track: {}", previous_track_id);
                                if let Ok(Some(previous_track)) =
                                    self.library_manager.get_track(&previous_track_id).await
                                {
                                    if let Ok(mut release_tracks) = self
                                        .library_manager
                                        .get_tracks(&previous_track.release_id)
                                        .await
                                    {
                                        release_tracks.sort_by(|a, b| {
                                            match (a.track_number, b.track_number) {
                                                (Some(a_num), Some(b_num)) => a_num.cmp(&b_num),
                                                (Some(_), None) => std::cmp::Ordering::Less,
                                                (None, Some(_)) => std::cmp::Ordering::Greater,
                                                (None, None) => std::cmp::Ordering::Equal,
                                            }
                                        });
                                        let mut new_previous_track_id = None;
                                        for release_track in &release_tracks {
                                            if release_track.id == previous_track_id {
                                                break;
                                            }
                                            new_previous_track_id = Some(release_track.id.clone());
                                        }
                                        self.previous_track_id = new_previous_track_id;
                                        self.queue.clear();
                                        let mut found_current = false;
                                        for release_track in release_tracks {
                                            if found_current {
                                                self.queue.push_back(release_track.id);
                                            } else if release_track.id == previous_track_id {
                                                found_current = true;
                                            }
                                        }
                                        self.emit_queue_update();
                                    }
                                }
                                self.clear_next_track_state();
                                self.play_track(&previous_track_id, false, true).await;
                            // preserve paused
                            } else {
                                info!("No previous track, restarting current track");
                                self.play_track(&current_track_id, false, true).await;
                                // preserve paused
                            }
                        } else {
                            info!("Restarting current track from beginning");
                            let saved_previous = self.previous_track_id.clone();
                            self.play_track(&current_track_id, false, true).await; // preserve paused
                            if saved_previous.is_some() {
                                self.previous_track_id = saved_previous;
                            }
                        }
                    }
                }
                PlaybackCommand::Seek(position) => {
                    self.seek(position).await;
                }
                PlaybackCommand::SetVolume(volume) => {
                    self.audio_output.set_volume(volume);
                }
                PlaybackCommand::AddToQueue(track_ids) => {
                    for track_id in track_ids {
                        self.queue.push_back(track_id);
                    }
                    self.emit_queue_update();
                }
                PlaybackCommand::AddNext(track_ids) => {
                    for track_id in track_ids.into_iter().rev() {
                        self.queue.push_front(track_id);
                    }
                    self.emit_queue_update();
                }
                PlaybackCommand::RemoveFromQueue(index) => {
                    if index < self.queue.len() {
                        if let Some(removed_track_id) = self.queue.remove(index) {
                            if self
                                .current_track_id()
                                .map(|id| id == removed_track_id)
                                .unwrap_or(false)
                            {
                                self.stop().await;
                            }
                        }
                        self.emit_queue_update();
                    }
                }
                PlaybackCommand::ReorderQueue { from, to } => {
                    if from < self.queue.len() && to < self.queue.len() && from != to {
                        if let Some(track_id) = self.queue.remove(from) {
                            if to > from {
                                self.queue.insert(to - 1, track_id);
                            } else {
                                self.queue.insert(to, track_id);
                            }
                            self.emit_queue_update();
                        }
                    }
                }
                PlaybackCommand::ClearQueue => {
                    self.queue.clear();
                    self.emit_queue_update();
                }
                PlaybackCommand::GetQueue => {
                    self.emit_queue_update();
                }
            }
        }
        info!("PlaybackService stopped");
    }
    /// Play a track.
    /// - `is_natural_transition`: if true, plays from INDEX 00 (pregap included)
    /// - `preserve_paused`: if true, inherits current paused state; if false, always starts playing
    async fn play_track(
        &mut self,
        track_id: &str,
        is_natural_transition: bool,
        preserve_paused: bool,
    ) {
        info!(
            "Playing track: {} (natural_transition: {}, preserve_paused: {})",
            track_id, is_natural_transition, preserve_paused
        );

        let _ = self.progress_tx.send(PlaybackProgress::StateChanged {
            state: PlaybackState::Loading {
                track_id: track_id.to_string(),
            },
        });

        // Prepare track: fetch metadata, create buffer, start reading
        let prepared = match prepare_track(
            &self.library_manager,
            self.encryption_service.as_ref(),
            track_id,
        )
        .await
        {
            Ok(p) => p,
            Err(e) => {
                error!("Failed to prepare track {}: {}", track_id, e);
                self.stop().await;
                return;
            }
        };

        // Calculate pregap byte offset if needed (direct selection skips pregap)
        let pregap_skip_duration = pregap_seek_position(prepared.pregap_ms, is_natural_transition);
        let pregap_byte_offset: Option<u64> = pregap_skip_duration.and_then(|pregap_duration| {
            serde_json::from_str::<Vec<crate::audio_codec::SeekEntry>>(&prepared.seektable_json)
                .ok()
                .and_then(|entries| {
                    find_frame_boundary(&entries, pregap_duration, prepared.sample_rate).map(
                        |(byte_offset, _)| {
                            info!(
                                "Pregap skip: will start decoder at byte offset {} for {:?} pregap",
                                byte_offset, pregap_duration
                            );
                            byte_offset
                        },
                    )
                })
        });

        // Create decoder sink/source with track's actual sample rate
        let (mut sink, source, _ready) = create_streaming_pair(prepared.sample_rate, 2);

        // Spawn decoder thread
        let decoder_buffer = prepared.buffer.clone();
        let decoder_skip_to = pregap_byte_offset.map(|offset| prepared.audio_data_start + offset);
        std::thread::spawn(move || {
            if let Some(skip_position) = decoder_skip_to {
                decoder_buffer.seek(skip_position);
            }
            if let Err(e) = crate::audio_codec::decode_audio_streaming(decoder_buffer, &mut sink, 0)
            {
                error!("Streaming decode failed: {}", e);
            }
        });

        // Position offset: when we skip pregap, decoder positions start at 0 but actual
        // track position is pregap_ms
        let position_offset = if pregap_byte_offset.is_some() {
            std::time::Duration::from_millis(prepared.pregap_ms.unwrap_or(0).max(0) as u64)
        } else {
            std::time::Duration::ZERO
        };

        let track = prepared.track.clone();
        let duration = prepared.duration;
        let pregap_ms = prepared.pregap_ms;

        // Store prepared track state
        self.current_prepared = Some(prepared);

        // Initialize streaming
        let source = Arc::new(Mutex::new(source));
        if !self
            .init_streaming(source, position_offset, track_id.to_string())
            .await
        {
            self.stop().await;
            return;
        }

        // Set audio state: always Playing unless preserving paused state
        if !preserve_paused {
            self.audio_output
                .set_state(crate::playback::cpal_output::AudioState::Playing);
        }

        // Send state notification
        let state = if self.audio_output.is_paused() {
            PlaybackState::Paused {
                track: track.clone(),
                position: position_offset,
                duration: Some(duration),
                decoded_duration: duration,
                pregap_ms,
            }
        } else {
            PlaybackState::Playing {
                track: track.clone(),
                position: position_offset,
                duration: Some(duration),
                decoded_duration: duration,
                pregap_ms,
            }
        };
        let _ = self
            .progress_tx
            .send(PlaybackProgress::StateChanged { state });

        info!("Streaming playback started for track: {}", track_id);

        // Preload next track
        if let Some(next_id) = self.queue.front().cloned() {
            self.preload_next_track(&next_id).await;
        }
    }
    /// Preload the next track for gapless playback.
    /// This eagerly starts the decoder so samples are ready when we switch tracks.
    async fn preload_next_track(&mut self, track_id: &str) {
        // Prepare track: fetch metadata, create buffer, start reading
        let prepared = match prepare_track(
            &self.library_manager,
            self.encryption_service.as_ref(),
            track_id,
        )
        .await
        {
            Ok(p) => p,
            Err(e) => {
                error!("Failed to preload track {}: {}", track_id, e);
                return;
            }
        };

        // Create decoder sink/source and start decoder eagerly for gapless playback
        let (mut sink, source, _ready) = create_streaming_pair(prepared.sample_rate, 2);
        let decoder_buffer = prepared.buffer.clone();
        std::thread::spawn(move || {
            if let Err(e) = crate::audio_codec::decode_audio_streaming(decoder_buffer, &mut sink, 0)
            {
                error!("Preload streaming decode failed: {}", e);
            }
        });

        let source = Arc::new(Mutex::new(source));

        // Store preloaded state
        self.next_prepared = Some(prepared);
        self.next_streaming_source = Some(source);

        info!("Preloaded next track (streaming): {}", track_id);
    }
    async fn pause(&mut self) {
        self.audio_output
            .set_state(crate::playback::cpal_output::AudioState::Paused);
        if let Some(prepared) = &self.current_prepared {
            let position = self
                .current_position_shared
                .lock()
                .unwrap()
                .unwrap_or(std::time::Duration::ZERO);
            let duration = Some(prepared.duration);
            let decoded_duration = prepared.duration;
            let pregap_ms = prepared.pregap_ms;
            let track = prepared.track.clone();
            let _ = self.progress_tx.send(PlaybackProgress::StateChanged {
                state: PlaybackState::Paused {
                    track,
                    position,
                    duration,
                    decoded_duration,
                    pregap_ms,
                },
            });
        }
    }

    async fn resume(&mut self) {
        self.audio_output
            .set_state(crate::playback::cpal_output::AudioState::Playing);
        if let Some(prepared) = &self.current_prepared {
            let position = self
                .current_position_shared
                .lock()
                .unwrap()
                .unwrap_or(std::time::Duration::ZERO);
            let duration = Some(prepared.duration);
            let decoded_duration = prepared.duration;
            let pregap_ms = prepared.pregap_ms;
            let track = prepared.track.clone();
            let _ = self.progress_tx.send(PlaybackProgress::StateChanged {
                state: PlaybackState::Playing {
                    track,
                    position,
                    duration,
                    decoded_duration,
                    pregap_ms,
                },
            });
        }
    }

    fn clear_next_track_state(&mut self) {
        // Cancel any active streaming source for the next track
        if let Some(source) = self.next_streaming_source.take() {
            if let Ok(guard) = source.lock() {
                guard.cancel();
            }
        }
        // Cancel any active sparse buffer for the next track
        if let Some(prepared) = &self.next_prepared {
            prepared.buffer.cancel();
        }
        self.next_prepared = None;
    }

    /// Play a preloaded track by swapping next state to current and starting the audio stream.
    /// Play a preloaded track. The decoder is already running from preload_next_track.
    /// Play the preloaded next track.
    /// - `is_natural_transition`: if true, plays from INDEX 00 (pregap included)
    /// - `preserve_paused`: if true, inherits current paused state; if false, always starts playing
    async fn play_preloaded_track(&mut self, is_natural_transition: bool, preserve_paused: bool) {
        let next_prepared = match self.next_prepared.take() {
            Some(p) => p,
            None => {
                error!("play_preloaded_track called but no next_prepared");
                return;
            }
        };

        let pregap_ms = next_prepared.pregap_ms;
        let track_id = next_prepared.track.id.clone();

        // If we need to skip pregap (direct selection), the preloaded state won't work
        // because it was set up for auto-advance (starting at byte 0).
        // Fall back to play_track which handles pregap at decoder start.
        if !is_natural_transition && pregap_ms.is_some_and(|p| p > 0) {
            info!("Pregap skip needed for preloaded track - falling back to play_track");
            next_prepared.buffer.cancel();
            if let Some(source) = self.next_streaming_source.take() {
                if let Ok(guard) = source.lock() {
                    guard.cancel();
                }
            }
            self.play_track(&track_id, is_natural_transition, preserve_paused)
                .await;
            return;
        }

        let duration = next_prepared.duration;
        let track = next_prepared.track.clone();

        // Cancel current streaming state
        if let Some(source) = self.current_streaming_source.take() {
            if let Ok(guard) = source.lock() {
                guard.cancel();
            }
        }
        if let Some(prepared) = &self.current_prepared {
            prepared.buffer.cancel();
        }

        // Swap next to current
        self.current_prepared = Some(next_prepared);
        let source = self
            .next_streaming_source
            .take()
            .expect("Preloaded track has no streaming source");

        // Natural transition: start at position 0 (INDEX 00, pregap plays)
        let start_position = std::time::Duration::ZERO;

        // Initialize streaming with the preloaded source
        if !self
            .init_streaming(source, start_position, track_id.clone())
            .await
        {
            self.stop().await;
            return;
        }

        // Set audio state: always Playing unless preserving paused state
        if !preserve_paused {
            self.audio_output
                .set_state(crate::playback::cpal_output::AudioState::Playing);
        }

        // Send state notification
        let state = if self.audio_output.is_paused() {
            PlaybackState::Paused {
                track: track.clone(),
                position: start_position,
                duration: Some(duration),
                decoded_duration: duration,
                pregap_ms,
            }
        } else {
            PlaybackState::Playing {
                track: track.clone(),
                position: start_position,
                duration: Some(duration),
                decoded_duration: duration,
                pregap_ms,
            }
        };
        let _ = self
            .progress_tx
            .send(PlaybackProgress::StateChanged { state });

        // Preload next track if available
        if let Some(next_track_id) = self.queue.front().cloned() {
            self.preload_next_track(&next_track_id).await;
        }
    }

    async fn stop(&mut self) {
        if let Some(stream) = self.stream.take() {
            drop(stream);
        }

        // Cancel streaming source if active
        if let Some(source) = self.current_streaming_source.take() {
            if let Ok(guard) = source.lock() {
                guard.cancel();
            }
        }

        // Cancel sparse buffer if active
        if let Some(prepared) = &self.current_prepared {
            prepared.buffer.cancel();
        }

        self.current_prepared = None;
        self.clear_next_track_state();
        *self.current_position_shared.lock().unwrap() = None;
        self.audio_output
            .set_state(crate::playback::cpal_output::AudioState::Stopped);
        let _ = self.progress_tx.send(PlaybackProgress::StateChanged {
            state: PlaybackState::Stopped,
        });
    }
    async fn seek(&mut self, position: std::time::Duration) {
        // Verify streaming state is available
        if self.current_streaming_source.is_none() {
            error!("Cannot seek: no streaming source active");
            return;
        }

        let prepared = match &self.current_prepared {
            Some(p) => p,
            None => {
                error!("Cannot seek: no current_prepared");
                return;
            }
        };

        let file_size = prepared.file_size;
        let track_id = prepared.track.id.clone();

        // Check for same-position seek (difference < 100ms)
        let current_position = self
            .current_position_shared
            .lock()
            .unwrap()
            .unwrap_or(std::time::Duration::ZERO);
        let position_diff = position.abs_diff(current_position);
        if position_diff < std::time::Duration::from_millis(100) {
            trace!(
                "Seek: Skipping seek to same position (difference: {:?} < 100ms)",
                position_diff
            );
            let _ = self.progress_tx.send(PlaybackProgress::SeekSkipped {
                requested_position: position,
                current_position,
            });
            return;
        }

        let track_duration = prepared.duration;

        // Cancel old source (makes callback output silence until stream is dropped)
        if let Some(old_source) = &self.current_streaming_source {
            if let Ok(guard) = old_source.lock() {
                guard.cancel();
            }
        }

        // Try frame-accurate seek using seektable, fall back to linear interpolation
        // The seektable is track-relative (sample 0 = track start, byte 0 = track start in buffer)
        let audio_data_start = prepared.audio_data_start;
        let (buffer_byte, sample_offset) = if let Some((frame_byte, offset)) =
            find_frame_boundary_for_seek(position, prepared.sample_rate, &prepared.seektable_json)
        {
            // frame_byte is track-relative, add audio_data_start to get buffer position
            let buffer_pos = audio_data_start + frame_byte;
            info!(
                "Seek using seektable: position {:?}, frame_byte {}, buffer_pos {}, sample_offset {}",
                position, frame_byte, buffer_pos, offset
            );
            (buffer_pos, offset)
        } else {
            let byte = calculate_byte_offset_for_seek(position, track_duration, file_size);
            (byte, 0)
        };

        // For CUE/FLAC, convert buffer position to file position for LocalFileReader
        // The buffer has [headers][track data], but we need to seek in the original file
        let file_byte = if let Some(track_start) = prepared.track_start_byte_offset {
            // frame_byte is track-relative, so file position = track_start + frame_byte
            let frame_byte = buffer_byte.saturating_sub(audio_data_start);
            track_start + frame_byte
        } else {
            // Regular FLAC: buffer position == file position
            buffer_byte
        };

        info!(
            "Seek: position {:?}, buffer_byte {}, file_byte {}, file_size {}, local={}, track_start={:?}",
            position, buffer_byte, file_byte, file_size, prepared.is_local_storage, prepared.track_start_byte_offset
        );

        // Create seek buffer - both local and cloud now use fresh readers at seek position
        let seek_buffer = if prepared.is_local_storage {
            // Local files: seek directly in file
            self.create_seek_buffer_for_local(prepared, file_byte)
        } else {
            // Cloud: start fresh range request at seek position
            self.create_seek_buffer_for_cloud(prepared, file_byte)
        };

        // Spawn decoder on the seek buffer, skipping sample_offset samples
        // to reach the exact seek position (not just the frame boundary)
        let (mut sink, source, ready_rx) = create_streaming_pair(prepared.sample_rate, 2);
        std::thread::spawn(move || {
            if let Err(e) =
                crate::audio_codec::decode_audio_streaming(seek_buffer, &mut sink, sample_offset)
            {
                error!("Seek decode failed: {}", e);
            }
        });

        // Wait for buffer to be ready (50% full or finished)
        // Timeout after 5s to prevent hangs on broken streams
        match tokio::time::timeout(std::time::Duration::from_secs(5), ready_rx).await {
            Ok(Ok(())) => {} // Buffer ready
            Ok(Err(_)) => {
                // Sender dropped without sending - decoder thread crashed
                error!("Seek decoder failed to signal ready");
                return;
            }
            Err(_) => {
                // Timeout - something is very wrong
                error!("Seek buffer ready timeout after 5s");
                return;
            }
        }

        // Increment generation to invalidate old position listeners
        self.position_generation
            .fetch_add(1, std::sync::atomic::Ordering::SeqCst);

        let source = Arc::new(Mutex::new(source));

        // Initialize streaming (position offset = seek target, so positions are relative to seek point)
        // State remains unchanged (Playing or Paused) - new stream inherits it
        if !self
            .init_streaming(source, position, track_id.clone())
            .await
        {
            return;
        }

        let _ = self.progress_tx.send(PlaybackProgress::Seeked {
            position,
            track_id,
            was_paused: self.audio_output.is_paused(),
        });
    }

    /// Create a seek buffer for local files by starting a new reader at target_byte.
    /// This avoids blocking on the existing buffer and is much faster.
    fn create_seek_buffer_for_local(
        &self,
        prepared: &PreparedTrack,
        target_byte: u64,
    ) -> SharedSparseBuffer {
        let seek_buffer = create_sparse_buffer();

        // Create a new reader starting at target_byte, ending at track end
        let config = AudioReadConfig {
            path: prepared.source_path.clone(),
            flac_headers: prepared.flac_headers.clone(),
            start_byte: Some(target_byte),
            end_byte: prepared.track_end_byte_offset,
        };

        let reader = Box::new(LocalFileReader::new(config));
        reader.start_reading(seek_buffer.clone());

        seek_buffer
    }

    /// Create a seek buffer for cloud storage by starting a fresh range request.
    /// This creates a new CloudStorageReader at target_byte, avoiding the need to
    /// wait for data to download sequentially.
    fn create_seek_buffer_for_cloud(
        &self,
        prepared: &PreparedTrack,
        target_byte: u64,
    ) -> SharedSparseBuffer {
        let seek_buffer = create_sparse_buffer();

        // Create config for new reader starting at target_byte
        let config = AudioReadConfig {
            path: prepared.source_path.clone(),
            flac_headers: prepared.flac_headers.clone(),
            start_byte: Some(target_byte),
            end_byte: prepared.track_end_byte_offset,
        };

        // Create a new cloud reader at the seek position
        if let Some(storage) = &prepared.cloud_storage {
            let reader = Box::new(
                CloudStorageReader::new(
                    config,
                    storage.clone(),
                    self.encryption_service
                        .as_ref()
                        .map(|e| Arc::new(e.clone())),
                    prepared.cloud_encrypted,
                )
                .with_encryption_nonce(prepared.encryption_nonce.clone()),
            );
            reader.start_reading(seek_buffer.clone());
        } else {
            // Fallback: shouldn't happen, but cancel the buffer if no storage
            error!("create_seek_buffer_for_cloud called but no cloud_storage available");
            seek_buffer.cancel();
        }

        seek_buffer
    }
    /// Emit queue update to all subscribers
    fn emit_queue_update(&self) {
        let track_ids: Vec<String> = self.queue.iter().cloned().collect();
        let _ = self
            .progress_tx
            .send(PlaybackProgress::QueueUpdated { tracks: track_ids });
    }
}

/// Calculate byte offset for seeking based on time position.
///
/// Uses linear interpolation assuming constant bitrate.
/// This is approximate but works well for CBR audio like FLAC.
fn calculate_byte_offset_for_seek(
    seek_time: std::time::Duration,
    track_duration: std::time::Duration,
    file_size: u64,
) -> u64 {
    if track_duration.is_zero() {
        return 0;
    }
    let ratio = seek_time.as_secs_f64() / track_duration.as_secs_f64();
    let ratio = ratio.clamp(0.0, 1.0);
    (file_size as f64 * ratio) as u64
}

/// Find frame-aligned byte offset using seektable.
///
/// Returns (byte_offset, sample_offset) where:
/// - byte: frame-aligned position in the file
/// - sample_offset: samples to skip after decoding to reach exact target
pub(crate) fn find_frame_boundary_for_seek(
    seek_time: std::time::Duration,
    sample_rate: u32,
    seektable_json: &str,
) -> Option<(u64, u64)> {
    let entries: Vec<crate::audio_codec::SeekEntry> = serde_json::from_str(seektable_json).ok()?;
    find_frame_boundary(&entries, seek_time, sample_rate)
}

/// Find frame-aligned byte offset from a seektable Vec.
///
/// Returns (byte_offset, sample_offset) where:
/// - byte: frame-aligned position in the file
/// - sample_offset: samples to skip after decoding to reach exact target
fn find_frame_boundary(
    entries: &[crate::audio_codec::SeekEntry],
    seek_time: std::time::Duration,
    sample_rate: u32,
) -> Option<(u64, u64)> {
    if entries.is_empty() {
        return None;
    }

    let target_sample = (seek_time.as_secs_f64() * sample_rate as f64) as u64;

    // Binary search for the frame at or before target_sample
    let mut best_idx = 0;
    for (i, entry) in entries.iter().enumerate() {
        if entry.sample <= target_sample {
            best_idx = i;
        } else {
            break;
        }
    }

    let frame = &entries[best_idx];
    let sample_offset = target_sample.saturating_sub(frame.sample);

    Some((frame.byte, sample_offset))
}

/// Determine if we need to seek to skip the pregap.
///
/// Returns `Some(position)` if a seek is needed to skip the pregap (direct selection),
/// or `None` if playback should start from the beginning (natural transition or no pregap).
///
/// CD-like pregap behavior:
/// - Direct selection (play, next, previous): skip pregap, start at INDEX 01
/// - Natural transition (auto-advance): play pregap from INDEX 00
fn pregap_seek_position(
    pregap_ms: Option<i64>,
    is_natural_transition: bool,
) -> Option<std::time::Duration> {
    if is_natural_transition {
        // Natural transition: start at INDEX 00, play the pregap
        None
    } else {
        // Direct selection: skip to INDEX 01 if there's a pregap
        pregap_ms
            .filter(|&p| p > 0)
            .map(|p| std::time::Duration::from_millis(p as u64))
    }
}

/// Validate a seek position against the decoded audio duration.
///
/// IMPORTANT: This must use `decoded_duration` (actual PCM length including pregap),
/// NOT `track_duration` (metadata duration excluding pregap).
///
/// The UI sends seek positions that include pregap offset, so validation must
/// compare against the full decoded audio length.
#[cfg(test)]
mod tests {
    use super::*;

    /// Validate that a seek position is within bounds
    fn validate_seek_position(
        seek_position: std::time::Duration,
        decoded_duration: std::time::Duration,
    ) -> Result<(), SeekValidationError> {
        if seek_position > decoded_duration {
            Err(SeekValidationError::PastEnd {
                requested: seek_position,
                max_seekable: decoded_duration,
            })
        } else {
            Ok(())
        }
    }

    /// Error returned when seek validation fails
    #[derive(Debug, Clone, PartialEq, Eq)]
    enum SeekValidationError {
        PastEnd {
            requested: std::time::Duration,
            max_seekable: std::time::Duration,
        },
    }

    #[test]
    fn test_direct_selection_skips_pregap() {
        // When directly selecting a track with 3-second pregap,
        // we need to seek to 3000ms (skipping the pregap)
        let pregap_ms = Some(3000i64);
        let is_natural_transition = false;

        let seek_pos = pregap_seek_position(pregap_ms, is_natural_transition);

        assert_eq!(
            seek_pos,
            Some(std::time::Duration::from_millis(3000)),
            "Direct selection should return seek position to skip pregap"
        );
    }

    #[test]
    fn test_natural_transition_plays_pregap() {
        // When naturally transitioning to a track with 3-second pregap,
        // no seek needed - play from start (including pregap)
        let pregap_ms = Some(3000i64);
        let is_natural_transition = true;

        let seek_pos = pregap_seek_position(pregap_ms, is_natural_transition);

        assert_eq!(
            seek_pos, None,
            "Natural transition should return None (no seek, play pregap)"
        );
    }

    #[test]
    fn test_direct_selection_no_pregap() {
        // When directly selecting a track without pregap,
        // no seek needed
        let pregap_ms = None;
        let is_natural_transition = false;

        let seek_pos = pregap_seek_position(pregap_ms, is_natural_transition);

        assert_eq!(
            seek_pos, None,
            "Direct selection without pregap should return None (no seek needed)"
        );
    }

    #[test]
    fn test_natural_transition_no_pregap() {
        // When naturally transitioning to a track without pregap,
        // no seek needed
        let pregap_ms = None;
        let is_natural_transition = true;

        let seek_pos = pregap_seek_position(pregap_ms, is_natural_transition);

        assert_eq!(
            seek_pos, None,
            "Natural transition without pregap should return None"
        );
    }

    #[test]
    fn test_seek_validation_within_bounds() {
        // Seeking within decoded duration should succeed
        let decoded_duration = std::time::Duration::from_secs(180);
        let seek_position = std::time::Duration::from_secs(90);

        assert!(
            validate_seek_position(seek_position, decoded_duration).is_ok(),
            "Seeking within bounds should succeed"
        );
    }

    #[test]
    fn test_seek_validation_at_end() {
        // Seeking to exactly the end should succeed
        let decoded_duration = std::time::Duration::from_secs(180);
        let seek_position = std::time::Duration::from_secs(180);

        assert!(
            validate_seek_position(seek_position, decoded_duration).is_ok(),
            "Seeking to exactly the end should succeed"
        );
    }

    #[test]
    fn test_seek_validation_past_end() {
        // Seeking past the end should fail
        let decoded_duration = std::time::Duration::from_secs(180);
        let seek_position = std::time::Duration::from_secs(181);

        let result = validate_seek_position(seek_position, decoded_duration);
        assert!(result.is_err(), "Seeking past end should fail");

        if let Err(SeekValidationError::PastEnd {
            requested,
            max_seekable,
        }) = result
        {
            assert_eq!(requested, seek_position);
            assert_eq!(max_seekable, decoded_duration);
        }
    }

    #[test]
    fn test_seek_validation_with_pregap_track() {
        // For a track with 3 second pregap:
        // - track_duration (metadata) = 180 seconds (excludes pregap)
        // - decoded_duration (actual PCM) = 183 seconds (includes pregap)
        //
        // When user seeks to end of slider (180s adjusted), UI sends 180 + 3 = 183s
        // This MUST be valid because decoded audio is 183 seconds long.
        //
        // BUG that was fixed: validation was using track_duration (180s) instead of
        // decoded_duration (183s), causing seeks near the end to fail.
        let track_duration_metadata = std::time::Duration::from_secs(180);
        let pregap = std::time::Duration::from_secs(3);
        let decoded_duration = track_duration_metadata + pregap; // 183 seconds

        // User seeks to end of track (slider at max = 180s)
        // UI adds pregap back: 180 + 3 = 183
        let seek_position = std::time::Duration::from_secs(183);

        // With the fix, this should succeed (183 <= 183)
        assert!(
            validate_seek_position(seek_position, decoded_duration).is_ok(),
            "Seeking to end of pregap track should succeed when using decoded_duration"
        );

        // The bug was: validation used track_duration instead of decoded_duration
        // This would have failed: 183 > 180
        assert!(
            validate_seek_position(seek_position, track_duration_metadata).is_err(),
            "This shows the bug: using track_duration would incorrectly reject the seek"
        );
    }

    // Seek tests for SparseStreamingBuffer integration
    use crate::playback::sparse_buffer::SparseStreamingBuffer;

    #[test]
    fn test_calculate_byte_offset_for_seek() {
        // 3 minute track at 1411 kbps (CD quality) ≈ 31.7 MB
        let track_duration = std::time::Duration::from_secs(180);
        let file_size = 31_700_000u64;

        // Seek to 1 minute (1/3 of the track)
        let seek_time = std::time::Duration::from_secs(60);
        let offset = calculate_byte_offset_for_seek(seek_time, track_duration, file_size);

        // Should be roughly 1/3 of file size
        let expected = file_size / 3;
        assert!(
            (offset as i64 - expected as i64).abs() < 1000,
            "offset {} should be close to {} (1/3 of file)",
            offset,
            expected
        );
    }

    #[test]
    fn test_calculate_byte_offset_zero_duration() {
        let track_duration = std::time::Duration::ZERO;
        let file_size = 1000u64;
        let seek_time = std::time::Duration::from_secs(10);

        let offset = calculate_byte_offset_for_seek(seek_time, track_duration, file_size);
        assert_eq!(offset, 0, "Zero duration should return 0");
    }

    #[test]
    fn test_seek_within_buffer() {
        let buffer = SparseStreamingBuffer::new();
        // Buffer has first 10000 bytes
        buffer.append_at(0, &vec![0u8; 10000]);

        // Seek to byte 5000 - should be buffered
        assert!(
            buffer.is_buffered(5000),
            "Position 5000 should be within buffered range"
        );
    }

    #[test]
    fn test_seek_past_buffer() {
        let buffer = SparseStreamingBuffer::new();
        // Buffer has first 10000 bytes
        buffer.append_at(0, &vec![0u8; 10000]);

        // Seek to byte 50000 - should NOT be buffered
        assert!(
            !buffer.is_buffered(50000),
            "Position 50000 should be past buffered range"
        );
    }

    #[test]
    fn test_seek_multiple_ranges() {
        let buffer = SparseStreamingBuffer::new();
        // Buffer has 0-10000 and 50000-60000
        buffer.append_at(0, &vec![0u8; 10000]);
        buffer.append_at(50000, &vec![0u8; 10000]);

        // Currently at 55000, seek back to 5000 should reuse first range
        assert!(buffer.is_buffered(5000), "Position 5000 should be buffered");
        assert!(
            buffer.is_buffered(55000),
            "Position 55000 should be buffered"
        );
        assert!(
            !buffer.is_buffered(30000),
            "Position 30000 should NOT be buffered (gap)"
        );
    }

    #[test]
    fn test_seek_decision_buffered_vs_not() {
        use crate::playback::sparse_buffer::create_sparse_buffer;

        let buffer = create_sparse_buffer();
        let file_size = 100_000u64;
        let track_duration = std::time::Duration::from_secs(60);

        // Simulate downloading first 30%
        buffer.append_at(0, &vec![0u8; 30000]);

        // Seek to 10 seconds (1/6 of track) = ~16,666 bytes - should be buffered
        let seek_10s = std::time::Duration::from_secs(10);
        let byte_offset_10s = calculate_byte_offset_for_seek(seek_10s, track_duration, file_size);
        assert!(
            buffer.is_buffered(byte_offset_10s),
            "10s seek should be within buffered data"
        );

        // Seek to 50 seconds (5/6 of track) = ~83,333 bytes - should NOT be buffered
        let seek_50s = std::time::Duration::from_secs(50);
        let byte_offset_50s = calculate_byte_offset_for_seek(seek_50s, track_duration, file_size);
        assert!(
            !buffer.is_buffered(byte_offset_50s),
            "50s seek should be past buffered data"
        );
    }

    #[test]
    fn test_seek_back_after_forward_seek() {
        use crate::playback::sparse_buffer::create_sparse_buffer;

        let buffer = create_sparse_buffer();

        // Initial download: 0-30000
        buffer.append_at(0, &vec![0u8; 30000]);

        // User seeks forward to byte 70000 - new download starts there
        // Simulating: 70000-90000
        buffer.append_at(70000, &vec![0u8; 20000]);

        // Now we have two ranges: 0-30000 and 70000-90000
        assert_eq!(
            buffer.get_ranges(),
            vec![(0, 30000), (70000, 90000)],
            "Should have two non-contiguous ranges"
        );

        // User seeks back to byte 15000 - should be buffered (first range)
        assert!(buffer.is_buffered(15000), "15000 should be in first range");

        // User seeks to byte 75000 - should be buffered (second range)
        assert!(buffer.is_buffered(75000), "75000 should be in second range");

        // User seeks to byte 50000 - gap between ranges, not buffered
        assert!(!buffer.is_buffered(50000), "50000 should be in the gap");
    }

    #[test]
    fn test_ranges_merge_when_gap_filled() {
        use crate::playback::sparse_buffer::create_sparse_buffer;

        let buffer = create_sparse_buffer();

        // Initial download: 0-10000
        buffer.append_at(0, &vec![0u8; 10000]);

        // Seek forward creates second range: 20000-30000
        buffer.append_at(20000, &vec![0u8; 10000]);

        assert_eq!(buffer.get_ranges().len(), 2, "Should have two ranges");

        // Original download continues and fills gap: 10000-20000
        buffer.append_at(10000, &vec![0u8; 10000]);

        // Ranges should now be merged
        assert_eq!(buffer.get_ranges().len(), 1, "Ranges should be merged");
        assert_eq!(
            buffer.get_ranges(),
            vec![(0, 30000)],
            "Should be single contiguous range"
        );
    }

    #[test]
    fn test_full_track_buffered_all_seeks_instant() {
        use crate::playback::sparse_buffer::create_sparse_buffer;

        let buffer = create_sparse_buffer();
        let file_size = 100_000u64;
        let track_duration = std::time::Duration::from_secs(60);

        // Full track downloaded
        buffer.append_at(0, &vec![0u8; 100_000]);

        // All seeks should be within buffer
        for secs in [0, 10, 30, 45, 59] {
            let seek_time = std::time::Duration::from_secs(secs);
            let byte_offset = calculate_byte_offset_for_seek(seek_time, track_duration, file_size);
            assert!(
                buffer.is_buffered(byte_offset),
                "Seek to {}s (byte {}) should be buffered when full track cached",
                secs,
                byte_offset
            );
        }
    }

    #[test]
    fn test_find_frame_boundary_returns_sample_offset() {
        use crate::audio_codec::SeekEntry;

        // Create seektable with frames at known positions
        // Frame 0: sample 0, byte 0
        // Frame 1: sample 4096, byte 10000
        // Frame 2: sample 8192, byte 20000
        let entries = vec![
            SeekEntry { sample: 0, byte: 0 },
            SeekEntry {
                sample: 4096,
                byte: 10000,
            },
            SeekEntry {
                sample: 8192,
                byte: 20000,
            },
        ];

        let sample_rate = 44100;

        // Seek to exactly frame 1 boundary - sample_offset should be 0
        let target_sample_1 = 4096;
        let seek_time_1 =
            std::time::Duration::from_secs_f64(target_sample_1 as f64 / sample_rate as f64);
        let (byte_offset_1, sample_offset_1) =
            find_frame_boundary(&entries, seek_time_1, sample_rate).unwrap();
        assert_eq!(byte_offset_1, 10000, "Should seek to frame 1 byte offset");
        assert_eq!(
            sample_offset_1, 0,
            "Seeking exactly to frame boundary should have 0 sample offset"
        );

        // Seek to 500 samples after frame 1 - sample_offset should be ~500
        // (small rounding due to time<->sample conversion)
        let target_sample_2 = 4096 + 500;
        let seek_time_2 =
            std::time::Duration::from_secs_f64(target_sample_2 as f64 / sample_rate as f64);
        let (byte_offset_2, sample_offset_2) =
            find_frame_boundary(&entries, seek_time_2, sample_rate).unwrap();
        assert_eq!(
            byte_offset_2, 10000,
            "Should still seek to frame 1 byte offset"
        );
        assert!(
            (499..=501).contains(&sample_offset_2),
            "sample_offset should be ~500 (got {})",
            sample_offset_2
        );

        // Seek to 1000 samples after frame 2 - sample_offset should be ~1000
        let target_sample_3 = 8192 + 1000;
        let seek_time_3 =
            std::time::Duration::from_secs_f64(target_sample_3 as f64 / sample_rate as f64);
        let (byte_offset_3, sample_offset_3) =
            find_frame_boundary(&entries, seek_time_3, sample_rate).unwrap();
        assert_eq!(byte_offset_3, 20000, "Should seek to frame 2 byte offset");
        assert!(
            (998..=1002).contains(&sample_offset_3),
            "sample_offset should be ~1000 (got {})",
            sample_offset_3
        );
    }

    /// Test that demonstrates sample_offset must be used to avoid audio glitches.
    ///
    /// When seeking:
    /// 1. find_frame_boundary returns (byte_offset, sample_offset)
    /// 2. byte_offset is the frame BEFORE or AT the target
    /// 3. sample_offset is how many samples to SKIP to reach exact target
    ///
    /// If sample_offset is ignored:
    /// - Audio plays from frame boundary (earlier than requested)
    /// - Position reports the seek target
    /// - Result: audio/position mismatch, potential pops/clicks
    ///
    /// The fix: pass sample_offset to decoder or streaming sink to skip those samples.
    #[test]
    fn test_sample_offset_represents_samples_to_skip() {
        use crate::audio_codec::SeekEntry;

        let entries = vec![
            SeekEntry { sample: 0, byte: 0 },
            SeekEntry {
                sample: 96000, // 1 second at 96kHz
                byte: 100000,
            },
        ];

        let sample_rate = 96000; // 96kHz

        // Seek to 1.5 seconds
        let seek_time = std::time::Duration::from_secs_f64(1.5);
        let (byte_offset, sample_offset) =
            find_frame_boundary(&entries, seek_time, sample_rate).unwrap();

        // Should seek to frame at 1 second (byte 100000)
        assert_eq!(byte_offset, 100000);

        // sample_offset should be 48000 samples (0.5 seconds worth)
        // This tells us: after seeking to byte 100000 and decoding,
        // skip the first 48000 samples to reach the exact 1.5s position
        assert_eq!(
            sample_offset, 48000,
            "sample_offset should be 48000 (0.5s at 96kHz)"
        );

        // If this sample_offset is NOT used:
        // - Audio starts at 1.0s (frame boundary)
        // - Position reports 1.5s
        // - User hears 0.5s of audio they shouldn't (glitch)
        //
        // Current bug: sample_offset is computed but stored in _sample_offset (ignored)
    }

    /// Test that CUE/FLAC seek correctly converts buffer position to file position.
    ///
    /// For CUE/FLAC tracks:
    /// - The seektable is track-relative (sample 0 = track start, byte 0 = track data start)
    /// - The buffer has [headers][track data from start_byte_offset to end_byte_offset]
    /// - When seeking in LocalFileReader, we need file-absolute position, not buffer position
    ///
    /// The bug was: create_seek_buffer_for_local used buffer_byte directly as file position,
    /// so seeking in track 4 (at file byte 569MB) would instead seek to byte 0+frame_byte
    /// in the file, landing in track 1!
    #[test]
    fn test_cue_flac_seek_buffer_to_file_position() {
        // Simulate track 4 Barbarian which starts at byte 569,483,167 in the file
        let track_start_byte_offset: u64 = 569_483_167;
        let headers_len: u64 = 86;
        let sample_rate = 96000u32;

        // Build a track-relative seektable (as CUE/FLAC import creates)
        // sample 0 = track start, byte 0 = start of track data in buffer
        let seektable: Vec<crate::audio_codec::SeekEntry> = (0..100)
            .map(|sec| crate::audio_codec::SeekEntry {
                sample: sec * sample_rate as u64,
                byte: sec * 1_000_000, // ~1MB per second (compressed)
            })
            .collect();
        let seektable_json = serde_json::to_string(&seektable).unwrap();

        // User seeks to 60s into the track
        let seek_position = std::time::Duration::from_secs(60);

        // Seektable lookup (track-relative)
        let (frame_byte, _sample_offset) =
            find_frame_boundary_for_seek(seek_position, sample_rate, &seektable_json).unwrap();

        // frame_byte is track-relative: 60MB (60 seconds at 1MB/s)
        assert_eq!(frame_byte, 60_000_000, "frame_byte should be 60MB");

        // Buffer position = headers + frame_byte
        let buffer_byte = headers_len + frame_byte;
        assert_eq!(buffer_byte, 60_000_086);

        // === THE BUG ===
        // Old code used buffer_byte as file position for LocalFileReader:
        // This would seek to file byte 60,000,086 which is in track 1!

        // === THE FIX ===
        // File position = track_start_byte_offset + frame_byte
        let file_byte = track_start_byte_offset + frame_byte;
        assert_eq!(
            file_byte, 629_483_167,
            "file_byte should be in track 4's range"
        );

        // Verify file_byte is within track 4's range (569MB - 713MB)
        let track_end_byte_offset: u64 = 713_915_001;
        assert!(
            file_byte >= track_start_byte_offset && file_byte < track_end_byte_offset,
            "file_byte {} should be in track 4 range [{}, {})",
            file_byte,
            track_start_byte_offset,
            track_end_byte_offset
        );

        // The bug would have put us in track 1's range (0 - 67MB)
        let track1_end: u64 = 67_301_338;
        assert!(
            buffer_byte < track1_end,
            "Bug: buffer_byte {} used as file position would land in track 1 (ends at {})",
            buffer_byte,
            track1_end
        );
    }
}
