use crate::db::DbTrack;
use crate::encryption::EncryptionService;
use crate::library::LibraryManager;
use crate::playback::cpal_output::AudioOutput;
use crate::playback::data_source::{
    AudioDataReader, AudioReadConfig, CloudStorageReader, LocalFileReader,
};
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
/// Playback service that manages audio playback
pub struct PlaybackService {
    library_manager: LibraryManager,
    encryption_service: EncryptionService,
    command_rx: tokio_mpsc::UnboundedReceiver<PlaybackCommand>,
    progress_tx: tokio_mpsc::UnboundedSender<PlaybackProgress>,
    queue: VecDeque<String>,
    previous_track_id: Option<String>,
    current_track: Option<DbTrack>,
    current_position: Option<std::time::Duration>,
    /// Track duration from metadata (excludes pregap). Used for UI display only.
    /// May be slightly inaccurate depending on import source (CUE has ~13ms precision,
    /// other sources like Discogs/MusicBrainz may be less accurate).
    current_duration: Option<std::time::Duration>,
    /// Actual PCM audio duration (includes pregap). This is ground truth - use for
    /// seek validation, progress calculations, and anything requiring accuracy.
    current_decoded_duration: Option<std::time::Duration>,
    /// Pre-gap duration for CUE/FLAC tracks (None for regular tracks)
    current_pregap_ms: Option<i64>,
    is_paused: bool,
    current_position_shared: Arc<std::sync::Mutex<Option<std::time::Duration>>>,
    /// Generation counter to invalidate old position listeners after seek
    position_generation: Arc<std::sync::atomic::AtomicU64>,
    audio_output: AudioOutput,
    stream: Option<cpal::Stream>,
    /// Preloaded next track state (for gapless playback)
    next_track_id: Option<String>,
    next_duration: Option<std::time::Duration>,
    next_pregap_ms: Option<i64>,
    next_streaming_source: Option<Arc<Mutex<StreamingPcmSource>>>,
    next_sparse_buffer: Option<SharedSparseBuffer>,
    next_streaming_file_size: Option<u64>,
    next_streaming_source_path: Option<String>,
    next_flac_headers: Option<Vec<u8>>,
    next_sample_rate: Option<u32>,
    next_seektable_json: Option<String>,
    next_audio_data_start: Option<u64>,
    /// Current streaming source (if using streaming playback)
    current_streaming_source: Option<Arc<Mutex<StreamingPcmSource>>>,
    /// Current sparse buffer for seek support
    current_sparse_buffer: Option<SharedSparseBuffer>,
    /// File size or byte range size for current streaming track (for seek calculations)
    current_streaming_file_size: Option<u64>,
    /// Source path for current streaming track (for starting new downloads on seek)
    current_streaming_source_path: Option<String>,
    /// FLAC headers for current streaming track (prepended on decoder restart after seek)
    current_flac_headers: Option<Vec<u8>>,
    /// Sample rate for current streaming track (for time-to-sample conversion)
    current_sample_rate: Option<u32>,
    /// Seektable JSON for current streaming track (for frame-accurate seek)
    current_seektable_json: Option<String>,
    /// Audio data start offset for current streaming track (seektable byte offsets are relative to this)
    current_audio_data_start: Option<u64>,
}
impl PlaybackService {
    pub fn start(
        library_manager: LibraryManager,
        encryption_service: EncryptionService,
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
                    current_track: None,
                    current_position: None,
                    current_duration: None,
                    current_decoded_duration: None,
                    current_pregap_ms: None,
                    is_paused: false,
                    current_position_shared: Arc::new(std::sync::Mutex::new(None)),
                    position_generation: Arc::new(std::sync::atomic::AtomicU64::new(0)),
                    audio_output,
                    stream: None,
                    next_track_id: None,
                    next_duration: None,
                    next_pregap_ms: None,
                    next_streaming_source: None,
                    next_sparse_buffer: None,
                    next_streaming_file_size: None,
                    next_streaming_source_path: None,
                    next_flac_headers: None,
                    next_sample_rate: None,
                    next_seektable_json: None,
                    next_audio_data_start: None,
                    current_streaming_source: None,
                    current_sparse_buffer: None,
                    current_streaming_file_size: None,
                    current_streaming_source_path: None,
                    current_flac_headers: None,
                    current_sample_rate: None,
                    current_seektable_json: None,
                    current_audio_data_start: None,
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
                        .send_command(crate::playback::cpal_output::AudioCommand::Stop);
                    self.clear_next_track_state();
                    if let Some(current_track) = &self.current_track {
                        self.previous_track_id = Some(current_track.id.clone());
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
                    self.play_track(&track_id, false).await; // Direct selection: skip pregap
                }
                PlaybackCommand::PlayAlbum(track_ids) => {
                    if let Some(current_track) = &self.current_track {
                        self.previous_track_id = Some(current_track.id.clone());
                    }
                    self.queue.clear();
                    for track_id in track_ids {
                        self.queue.push_back(track_id);
                    }
                    if let Some(first_track) = self.queue.pop_front() {
                        self.emit_queue_update();
                        self.play_track(&first_track, false).await; // Direct selection: skip pregap
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
                    if let Some(preloaded_track_id) = self.next_track_id.clone() {
                        if self.next_streaming_source.is_some() {
                            info!("Using preloaded track: {}", preloaded_track_id);
                            if let Some(current_track) = &self.current_track {
                                self.previous_track_id = Some(current_track.id.clone());
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
                            let track =
                                match self.library_manager.get_track(&preloaded_track_id).await {
                                    Ok(Some(track)) => track,
                                    Ok(None) => {
                                        error!("Preloaded track not found: {}", preloaded_track_id);
                                        self.stop().await;
                                        continue;
                                    }
                                    Err(e) => {
                                        error!("Failed to get preloaded track metadata: {}", e);
                                        self.stop().await;
                                        continue;
                                    }
                                };
                            self.play_preloaded_track(
                                &preloaded_track_id,
                                track,
                                false, // Manual next: skip pregap
                            )
                            .await;
                        } else {
                            // Preload started but streaming source not ready yet
                            self.clear_next_track_state();
                            self.play_track(&preloaded_track_id, false).await;
                        }
                    } else if let Some(next_track) = self.queue.pop_front() {
                        info!("No preloaded track, playing from queue: {}", next_track);
                        self.emit_queue_update();
                        if let Some(current_track) = &self.current_track {
                            self.previous_track_id = Some(current_track.id.clone());
                        }
                        self.play_track(&next_track, false).await;
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
                    if let Some(preloaded_track_id) = self.next_track_id.clone() {
                        if self.next_streaming_source.is_some() {
                            info!("Using preloaded track: {}", preloaded_track_id);
                            if let Some(current_track) = &self.current_track {
                                self.previous_track_id = Some(current_track.id.clone());
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
                            let track =
                                match self.library_manager.get_track(&preloaded_track_id).await {
                                    Ok(Some(track)) => track,
                                    Ok(None) => {
                                        error!("Preloaded track not found: {}", preloaded_track_id);
                                        self.stop().await;
                                        continue;
                                    }
                                    Err(e) => {
                                        error!("Failed to get preloaded track metadata: {}", e);
                                        self.stop().await;
                                        continue;
                                    }
                                };
                            self.play_preloaded_track(
                                &preloaded_track_id,
                                track,
                                true, // Natural transition: play pregap
                            )
                            .await;
                        } else {
                            // Preload started but streaming source not ready yet
                            self.clear_next_track_state();
                            self.play_track(&preloaded_track_id, true).await;
                        }
                    } else if let Some(next_track) = self.queue.pop_front() {
                        info!("No preloaded track, playing from queue: {}", next_track);
                        self.emit_queue_update();
                        if let Some(current_track) = &self.current_track {
                            self.previous_track_id = Some(current_track.id.clone());
                        }
                        self.play_track(&next_track, true).await;
                    } else {
                        info!("No next track available, stopping");
                        self.emit_queue_update();
                        self.stop().await;
                    }
                }
                PlaybackCommand::Previous => {
                    if let Some(track) = &self.current_track {
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
                                self.play_track(&previous_track_id, false).await;
                            // Direct selection
                            } else {
                                info!("No previous track, restarting current track");
                                let track_id = track.id.clone();
                                self.play_track(&track_id, false).await; // Direct selection
                            }
                        } else {
                            info!("Restarting current track from beginning");
                            let track_id = track.id.clone();
                            let saved_previous = self.previous_track_id.clone();
                            self.play_track(&track_id, false).await; // Direct selection
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
                            if let Some(current_track) = &self.current_track {
                                if removed_track_id == current_track.id {
                                    self.stop().await;
                                }
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
    async fn play_track(&mut self, track_id: &str, is_natural_transition: bool) {
        info!(
            "Playing track: {} (natural_transition: {})",
            track_id, is_natural_transition
        );
        let _ = self.progress_tx.send(PlaybackProgress::StateChanged {
            state: PlaybackState::Loading {
                track_id: track_id.to_string(),
            },
        });
        let track = match self.library_manager.get_track(track_id).await {
            Ok(Some(track)) => track,
            Ok(None) => {
                error!("Track not found: {}", track_id);
                self.stop().await;
                return;
            }
            Err(e) => {
                error!("Failed to fetch track: {}", e);
                self.stop().await;
                return;
            }
        };

        let storage_profile = match self
            .library_manager
            .get_storage_profile_for_release(&track.release_id)
            .await
        {
            Ok(profile) => profile,
            Err(e) => {
                error!("Failed to get storage profile: {}", e);
                self.stop().await;
                return;
            }
        };

        // Get audio format metadata (headers, seektable, byte ranges)
        let audio_format = match self
            .library_manager
            .get_audio_format_by_track_id(track_id)
            .await
        {
            Ok(af) => af,
            Err(e) => {
                error!("Failed to get audio format: {}", e);
                self.stop().await;
                return;
            }
        };

        // Get the audio file via file_id FK
        let file_id = match audio_format.as_ref().and_then(|af| af.file_id.as_ref()) {
            Some(id) => id,
            None => {
                error!("No file_id in audio_format for track");
                self.stop().await;
                return;
            }
        };

        let audio_file = match self.library_manager.get_file_by_id(file_id).await {
            Ok(Some(f)) => f,
            Ok(None) => {
                error!("Audio file not found for file_id: {}", file_id);
                self.stop().await;
                return;
            }
            Err(e) => {
                error!("Failed to get audio file: {}", e);
                self.stop().await;
                return;
            }
        };

        let source_path = match audio_file.source_path {
            Some(p) => p,
            None => {
                error!("Audio file has no source_path");
                self.stop().await;
                return;
            }
        };
        let pregap_ms = audio_format.as_ref().and_then(|af| af.pregap_ms);
        let sample_rate = audio_format
            .as_ref()
            .and_then(|af| af.sample_rate.map(|r| r as u32))
            .unwrap_or(44100);

        let (start_byte, end_byte) = audio_format
            .as_ref()
            .and_then(|af| match (af.start_byte_offset, af.end_byte_offset) {
                (Some(s), Some(e)) => Some((Some(s as u64), Some(e as u64))),
                _ => None,
            })
            .unwrap_or((None, None));

        // Load all headers (for seek support)
        let all_flac_headers = audio_format.as_ref().and_then(|af| af.flac_headers.clone());

        // Headers to prepend during playback (only for CUE/FLAC where buffer doesn't have them)
        let needs_headers = audio_format.as_ref().is_some_and(|af| af.needs_headers);
        let flac_headers = if needs_headers {
            all_flac_headers.clone()
        } else {
            None
        };
        info!(
            "Play track: needs_headers={}, will_prepend_headers={}, all_headers_len={}",
            needs_headers,
            flac_headers.is_some(),
            all_flac_headers.as_ref().map(|h| h.len()).unwrap_or(0)
        );

        // Load seektable from audio format.
        // For CUE/FLAC tracks, the seektable is already per-track and adjusted during import
        // (byte offsets start at 0 = first byte of track audio data).
        let seektable: Option<Vec<crate::audio_codec::SeekEntry>> = audio_format
            .as_ref()
            .and_then(|af| af.seektable_json.as_ref())
            .and_then(|json| serde_json::from_str(json).ok());

        // For direct selection with pregap, calculate byte offset to skip to
        // We load the full track (including pregap) but start decoder at pregap offset
        // This allows seeking back into pregap later (hidden track support)
        let pregap_skip_duration = pregap_seek_position(pregap_ms, is_natural_transition);
        let pregap_byte_offset: Option<u64> = if let Some(pregap_duration) = pregap_skip_duration {
            if let Some(ref entries) = seektable {
                if let Some((byte_offset, _sample_offset)) =
                    find_frame_boundary(entries, pregap_duration, sample_rate)
                {
                    info!(
                        "Pregap skip: will start decoder at byte offset {} for {:?} pregap",
                        byte_offset, pregap_duration
                    );
                    Some(byte_offset)
                } else {
                    None
                }
            } else {
                None
            }
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
        let reader: Box<dyn AudioDataReader> = match &storage_profile {
            None => {
                // Non-storage: read local file directly (no encryption)
                Box::new(LocalFileReader::new(read_config))
            }
            Some(profile)
                if !profile.encrypted && profile.location == crate::db::StorageLocation::Local =>
            {
                // Unencrypted local storage: read directly
                Box::new(LocalFileReader::new(read_config))
            }
            Some(profile) => {
                // Encrypted or cloud storage: use CloudStorageReader (handles decryption)
                let storage = match create_storage_reader(profile).await {
                    Ok(s) => s,
                    Err(e) => {
                        error!("Failed to create storage reader: {}", e);
                        self.stop().await;
                        return;
                    }
                };
                Box::new(CloudStorageReader::new(
                    read_config,
                    storage,
                    Arc::new(self.encryption_service.clone()),
                    profile.encrypted,
                ))
            }
        };

        // Start reading data into buffer
        reader.start_reading(buffer.clone());

        // Create ring buffer for decoded audio
        let (mut sink, source) = create_streaming_pair(44100, 2);

        // Spawn decoder thread using AVIO-based streaming decode
        // FFmpeg handles all frame boundary detection - no seektable needed
        let decoder_buffer = buffer.clone();
        let decoder_skip_to = pregap_byte_offset.map(|offset| headers_len + offset);
        std::thread::spawn(move || {
            // If skipping pregap, seek buffer past headers to the pregap offset
            if let Some(skip_position) = decoder_skip_to {
                decoder_buffer.seek(skip_position);
            }
            if let Err(e) =
                crate::audio_codec::decode_audio_streaming_simple(decoder_buffer, &mut sink)
            {
                error!("Streaming decode failed: {}", e);
            }
        });

        // Store state for seek support
        self.current_sparse_buffer = Some(buffer);
        self.current_streaming_file_size = Some(file_size + headers_len);
        self.current_streaming_source_path = Some(source_path.clone());
        self.current_flac_headers = all_flac_headers; // Always store headers for seek
        self.current_sample_rate = audio_format
            .as_ref()
            .and_then(|af| af.sample_rate.map(|r| r as u32));
        // Store original seektable - not adjusted, so seeks can go anywhere including pregap
        self.current_seektable_json = audio_format
            .as_ref()
            .and_then(|af| af.seektable_json.clone());
        // For seek calculations, audio_data_start is where audio begins in our playback buffer.
        // - CUE/FLAC (needs_headers=true): We prepend headers, so audio starts at headers_len
        // - Regular FLAC: Buffer contains full file, use stored audio_data_start
        self.current_audio_data_start = if needs_headers {
            Some(headers_len)
        } else {
            audio_format
                .as_ref()
                .and_then(|af| af.audio_data_start.map(|s| s as u64))
        };

        let source = Arc::new(Mutex::new(source));
        let (source_sample_rate, source_channels) = {
            let guard = source.lock().unwrap();
            (guard.sample_rate(), guard.channels())
        };

        // Initial position depends on whether we skipped pregap at decoder start
        // If we skipped (pregap_byte_offset.is_some()), we start at pregap_ms
        // Otherwise (natural transition), we start at 0
        let start_position = if pregap_byte_offset.is_some() {
            std::time::Duration::from_millis(pregap_ms.unwrap_or(0).max(0) as u64)
        } else {
            std::time::Duration::ZERO
        };

        if let Some(stream) = self.stream.take() {
            drop(stream);
        }

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
                self.stop().await;
                return;
            }
        };

        if let Err(e) = stream.play() {
            error!("Failed to start streaming playback: {:?}", e);
            self.stop().await;
            return;
        }

        self.audio_output
            .send_command(crate::playback::cpal_output::AudioCommand::Play);
        self.stream = Some(stream);
        self.current_track = Some(track.clone());
        self.current_streaming_source = Some(source);
        // Position starts at start_position (pregap_ms if skipped, 0 if natural transition)
        self.current_position = Some(start_position);
        self.current_pregap_ms = pregap_ms;
        self.is_paused = false;
        *self.current_position_shared.lock().unwrap() = Some(start_position);

        let track_duration = track
            .duration_ms
            .map(|ms| std::time::Duration::from_millis(ms as u64))
            .unwrap_or(std::time::Duration::from_secs(300));
        self.current_duration = Some(track_duration);
        self.current_decoded_duration = Some(track_duration);

        let _ = self.progress_tx.send(PlaybackProgress::StateChanged {
            state: PlaybackState::Playing {
                track: track.clone(),
                position: start_position,
                duration: Some(track_duration),
                decoded_duration: track_duration,
                pregap_ms,
            },
        });

        // Preload next track
        let track_id_owned = track_id.to_string();
        if let Some(next_id) = self.queue.front().cloned() {
            self.preload_next_track(&next_id).await;
        }

        info!("Streaming playback started for track: {}", track_id_owned);

        // Monitor position and completion
        let progress_tx = self.progress_tx.clone();
        let current_position_shared = self.current_position_shared.clone();
        let position_generation = self.position_generation.clone();
        let gen = position_generation.load(std::sync::atomic::Ordering::SeqCst);
        let streaming_source = self.current_streaming_source.clone();

        tokio::spawn(async move {
            loop {
                tokio::select! {
                    Some(position) = position_rx_async.recv() => {
                        if position_generation.load(std::sync::atomic::Ordering::SeqCst) == gen {
                            *current_position_shared.lock().unwrap() = Some(position);
                            let _ = progress_tx.send(PlaybackProgress::PositionUpdate { position, track_id: track_id_owned.clone() });
                        }
                    }
                    Some(()) = completion_rx_async.recv() => {
                        if position_generation.load(std::sync::atomic::Ordering::SeqCst) == gen {
                            // Get decode error count from streaming source
                            let error_count = streaming_source
                                .as_ref()
                                .and_then(|s| s.lock().ok())
                                .map(|g| g.decode_error_count())
                                .unwrap_or(0);

                            info!("Streaming track completed: {} ({} decode errors)", track_id_owned, error_count);
                            let _ = progress_tx.send(PlaybackProgress::TrackCompleted { track_id: track_id_owned.clone() });
                            let _ = progress_tx.send(PlaybackProgress::DecodeStats {
                                track_id: track_id_owned.clone(),
                                error_count,
                            });
                        }
                        break;
                    }
                    else => break,
                }
            }
        });
    }
    /// Decode raw audio bytes to PCM source
    ///
    /// - frame_offset_samples: Skip this many samples at the start (frame boundary alignment)
    /// - exact_sample_count: Trim output to exactly this many samples (gapless playback)
    async fn preload_next_track(&mut self, track_id: &str) {
        let track = match self.library_manager.get_track(track_id).await {
            Ok(Some(track)) => track,
            Ok(None) => {
                error!("Cannot preload track {} - not found", track_id);
                return;
            }
            Err(e) => {
                error!("Cannot preload track {} - database error: {}", track_id, e);
                return;
            }
        };

        let storage_profile = match self
            .library_manager
            .get_storage_profile_for_release(&track.release_id)
            .await
        {
            Ok(profile) => profile,
            Err(e) => {
                error!("Failed to get storage profile for preload: {}", e);
                return;
            }
        };

        // Get audio format metadata (headers, seektable, byte ranges)
        let audio_format = match self
            .library_manager
            .get_audio_format_by_track_id(track_id)
            .await
        {
            Ok(af) => af,
            Err(e) => {
                error!("Failed to get audio format for preload: {}", e);
                return;
            }
        };

        // Get the audio file via file_id FK
        let file_id = match audio_format.as_ref().and_then(|af| af.file_id.as_ref()) {
            Some(id) => id,
            None => {
                error!("No file_id in audio_format for preload track");
                return;
            }
        };

        let audio_file = match self.library_manager.get_file_by_id(file_id).await {
            Ok(Some(f)) => f,
            Ok(None) => {
                error!("Audio file not found for preload file_id: {}", file_id);
                return;
            }
            Err(e) => {
                error!("Failed to get audio file for preload: {}", e);
                return;
            }
        };

        let source_path = match audio_file.source_path {
            Some(p) => p,
            None => {
                error!("Audio file has no source_path for preload");
                return;
            }
        };

        let pregap_ms = audio_format.as_ref().and_then(|af| af.pregap_ms);

        let (start_byte, end_byte) = audio_format
            .as_ref()
            .and_then(|af| match (af.start_byte_offset, af.end_byte_offset) {
                (Some(s), Some(e)) => Some((Some(s as u64), Some(e as u64))),
                _ => None,
            })
            .unwrap_or((None, None));

        // Load all headers (for seek support)
        let all_flac_headers = audio_format.as_ref().and_then(|af| af.flac_headers.clone());

        // Headers to prepend during playback (only for CUE/FLAC where buffer doesn't have them)
        let needs_headers = audio_format.as_ref().is_some_and(|af| af.needs_headers);
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
        let reader: Box<dyn AudioDataReader> = match &storage_profile {
            None => {
                // Non-storage: read local file directly (no encryption)
                Box::new(LocalFileReader::new(read_config))
            }
            Some(profile)
                if !profile.encrypted && profile.location == crate::db::StorageLocation::Local =>
            {
                // Unencrypted local storage: read directly
                Box::new(LocalFileReader::new(read_config))
            }
            Some(profile) => {
                // Encrypted or cloud storage: use CloudStorageReader (handles decryption)
                let storage = match create_storage_reader(profile).await {
                    Ok(s) => s,
                    Err(e) => {
                        error!("Failed to create storage reader for preload: {}", e);
                        return;
                    }
                };
                Box::new(CloudStorageReader::new(
                    read_config,
                    storage,
                    Arc::new(self.encryption_service.clone()),
                    profile.encrypted,
                ))
            }
        };

        // Start reading data into buffer
        reader.start_reading(buffer.clone());

        // Create ring buffer for decoded audio
        let (mut sink, source) = create_streaming_pair(44100, 2);

        // Spawn decoder thread using AVIO-based streaming decode
        let decoder_buffer = buffer.clone();
        std::thread::spawn(move || {
            if let Err(e) =
                crate::audio_codec::decode_audio_streaming_simple(decoder_buffer, &mut sink)
            {
                error!("Preload streaming decode failed: {}", e);
            }
        });

        let source = Arc::new(Mutex::new(source));

        // Store preloaded state
        self.next_sparse_buffer = Some(buffer);
        self.next_streaming_file_size = Some(file_size + headers_len);
        self.next_streaming_source_path = Some(source_path);
        self.next_flac_headers = all_flac_headers;
        self.next_sample_rate = audio_format
            .as_ref()
            .and_then(|af| af.sample_rate.map(|r| r as u32));
        self.next_seektable_json = audio_format
            .as_ref()
            .and_then(|af| af.seektable_json.clone());
        self.next_audio_data_start = if needs_headers {
            Some(headers_len)
        } else {
            audio_format
                .as_ref()
                .and_then(|af| af.audio_data_start.map(|s| s as u64))
        };
        self.next_streaming_source = Some(source);

        let duration = track
            .duration_ms
            .map(|ms| std::time::Duration::from_millis(ms as u64))
            .unwrap_or_else(|| panic!("Cannot preload track {} without duration", track_id));
        self.next_track_id = Some(track_id.to_string());
        self.next_duration = Some(duration);
        self.next_pregap_ms = pregap_ms;

        info!("Preloaded next track (streaming): {}", track_id);
    }
    async fn pause(&mut self) {
        self.audio_output
            .send_command(crate::playback::cpal_output::AudioCommand::Pause);
        if let Some(track) = &self.current_track {
            let position = self
                .current_position_shared
                .lock()
                .unwrap()
                .unwrap_or(std::time::Duration::ZERO);
            let duration = self.current_duration;
            let decoded_duration = self
                .current_decoded_duration
                .unwrap_or(std::time::Duration::ZERO);
            self.is_paused = true;
            let pregap_ms = self.current_pregap_ms;
            let _ = self.progress_tx.send(PlaybackProgress::StateChanged {
                state: PlaybackState::Paused {
                    track: track.clone(),
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
            .send_command(crate::playback::cpal_output::AudioCommand::Resume);
        if let Some(track) = &self.current_track {
            let position = self
                .current_position_shared
                .lock()
                .unwrap()
                .unwrap_or(std::time::Duration::ZERO);
            let duration = self.current_duration;
            let decoded_duration = self
                .current_decoded_duration
                .unwrap_or(std::time::Duration::ZERO);
            let pregap_ms = self.current_pregap_ms;
            self.is_paused = false;
            let _ = self.progress_tx.send(PlaybackProgress::StateChanged {
                state: PlaybackState::Playing {
                    track: track.clone(),
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
        if let Some(buffer) = &self.next_sparse_buffer {
            buffer.cancel();
        }
        self.next_track_id = None;
        self.next_duration = None;
        self.next_pregap_ms = None;
        self.next_streaming_source = None;
        self.next_sparse_buffer = None;
        self.next_streaming_file_size = None;
        self.next_streaming_source_path = None;
        self.next_flac_headers = None;
        self.next_sample_rate = None;
        self.next_seektable_json = None;
        self.next_audio_data_start = None;
    }

    /// Play a preloaded track by swapping next_* state to current_* and starting the audio stream.
    async fn play_preloaded_track(
        &mut self,
        track_id: &str,
        track: DbTrack,
        is_natural_transition: bool,
    ) {
        let pregap_ms = self.next_pregap_ms;

        // If we need to skip pregap (direct selection), the preloaded state won't work
        // because it was set up for auto-advance (starting at byte 0).
        // Fall back to play_track which handles pregap at decoder start.
        if !is_natural_transition && pregap_ms.is_some_and(|p| p > 0) {
            info!("Pregap skip needed for preloaded track - falling back to play_track");
            self.clear_next_track_state();
            self.play_track(track_id, is_natural_transition).await;
            return;
        }

        self.next_pregap_ms = None;
        let duration = self
            .next_duration
            .take()
            .expect("Preloaded track has no duration");

        // Cancel current streaming source if active
        if let Some(source) = self.current_streaming_source.take() {
            if let Ok(guard) = source.lock() {
                guard.cancel();
            }
        }
        if let Some(buffer) = &self.current_sparse_buffer {
            buffer.cancel();
        }
        if let Some(stream) = self.stream.take() {
            drop(stream);
        }

        // Swap next_* to current_*
        self.current_sparse_buffer = self.next_sparse_buffer.take();
        self.current_streaming_file_size = self.next_streaming_file_size.take();
        self.current_streaming_source_path = self.next_streaming_source_path.take();
        self.current_flac_headers = self.next_flac_headers.take();
        self.current_sample_rate = self.next_sample_rate.take();
        self.current_seektable_json = self.next_seektable_json.take();
        self.current_audio_data_start = self.next_audio_data_start.take();

        let source = self
            .next_streaming_source
            .take()
            .expect("Preloaded track has no streaming source");
        self.next_track_id = None;

        let (source_sample_rate, source_channels) = {
            let guard = source.lock().unwrap();
            (guard.sample_rate(), guard.channels())
        };

        // This path is only used for natural transitions (auto-advance) where we play from the beginning
        let start_position = std::time::Duration::ZERO;

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
                error!(
                    "Failed to create streaming audio stream for preloaded track: {:?}",
                    e
                );
                self.stop().await;
                return;
            }
        };

        if let Err(e) = stream.play() {
            error!(
                "Failed to start streaming playback for preloaded track: {:?}",
                e
            );
            self.stop().await;
            return;
        }

        self.audio_output
            .send_command(crate::playback::cpal_output::AudioCommand::Play);
        self.stream = Some(stream);
        self.current_track = Some(track.clone());
        self.current_streaming_source = Some(source);
        // Natural transition: start at position 0 (INDEX 00, pregap plays)
        self.current_position = Some(start_position);
        self.current_pregap_ms = pregap_ms;
        self.is_paused = false;
        *self.current_position_shared.lock().unwrap() = Some(start_position);

        self.current_duration = Some(duration);
        self.current_decoded_duration = Some(duration);

        let _ = self.progress_tx.send(PlaybackProgress::StateChanged {
            state: PlaybackState::Playing {
                track: track.clone(),
                position: start_position,
                duration: Some(duration),
                decoded_duration: duration,
                pregap_ms,
            },
        });

        // Start completion listener
        let track_id_owned = track_id.to_string();
        let progress_tx = self.progress_tx.clone();
        let streaming_source = self.current_streaming_source.clone();
        let position_shared = self.current_position_shared.clone();
        let generation = self
            .position_generation
            .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        let position_generation = self.position_generation.clone();

        info!(
            "Play preloaded: Spawning completion listener task for track: {}",
            track_id
        );
        tokio::spawn(async move {
            info!("Play preloaded: Task started, waiting for position updates and completion");
            loop {
                tokio::select! {
                    Some(pos) = position_rx_async.recv() => {
                        let current_gen = position_generation.load(std::sync::atomic::Ordering::SeqCst);
                        if current_gen != generation {
                            info!("Play preloaded: Generation mismatch ({} != {}), exiting", current_gen, generation);
                            break;
                        }
                        *position_shared.lock().unwrap() = Some(pos);
                        let _ = progress_tx.send(PlaybackProgress::PositionUpdate {
                            position: pos,
                            track_id: track_id_owned.clone(),
                        });
                    }
                    result = completion_rx_async.recv() => {
                        if result.is_some() {
                            let decode_error_count = streaming_source.as_ref().map(|s| {
                                s.lock().map(|g| g.decode_error_count()).unwrap_or(0)
                            }).unwrap_or(0);
                            info!("Track completed: {} ({} decode errors)", track_id_owned, decode_error_count);
                            let _ = progress_tx.send(PlaybackProgress::TrackCompleted {
                                track_id: track_id_owned.clone(),
                            });
                            let _ = progress_tx.send(PlaybackProgress::DecodeStats {
                                track_id: track_id_owned.clone(),
                                error_count: decode_error_count,
                            });
                        }
                        info!("Play preloaded: Completion received, exiting");
                        break;
                    }
                    else => {
                        info!("Play preloaded: Channels closed, exiting");
                        break;
                    }
                }
            }
            info!(
                "Play preloaded: Completion listener task exiting for track: {}",
                track_id_owned
            );
        });

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
        if let Some(buffer) = &self.current_sparse_buffer {
            buffer.cancel();
        }

        self.current_track = None;
        self.current_position = None;
        self.current_duration = None;
        self.clear_next_track_state();
        self.current_sparse_buffer = None;
        self.current_streaming_file_size = None;
        self.current_streaming_source_path = None;
        self.current_flac_headers = None;
        self.current_sample_rate = None;
        self.current_seektable_json = None;
        self.current_audio_data_start = None;
        self.audio_output
            .send_command(crate::playback::cpal_output::AudioCommand::Stop);
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

        let (buffer, file_size, track) = match (
            &self.current_sparse_buffer,
            self.current_streaming_file_size,
            &self.current_track,
        ) {
            (Some(b), Some(fs), Some(t)) => (b, fs, t),
            _ => {
                error!("Cannot seek: missing buffer, file_size, or track");
                return;
            }
        };

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

        let track_duration = self
            .current_decoded_duration
            .unwrap_or_else(|| std::time::Duration::from_secs(300));

        // Try frame-accurate seek using seektable, fall back to linear interpolation
        // Seektable byte offsets are relative to audio_data_start, so add it to get file offset
        let audio_data_start = self.current_audio_data_start.unwrap_or(0);
        let (target_byte, _sample_offset) = if let (Some(sample_rate), Some(ref seektable)) =
            (self.current_sample_rate, &self.current_seektable_json)
        {
            if let Some((frame_byte, offset)) =
                find_frame_boundary_for_seek(position, sample_rate, seektable)
            {
                // frame_byte is relative to audio data start, convert to file offset
                let file_byte = audio_data_start + frame_byte;
                info!(
                            "Seek using seektable: position {:?}, frame_byte {}, file_byte {}, sample_offset {}",
                            position, frame_byte, file_byte, offset
                        );
                (file_byte, offset)
            } else {
                let byte = calculate_byte_offset_for_seek(position, track_duration, file_size);
                (byte, 0)
            }
        } else {
            let byte = calculate_byte_offset_for_seek(position, track_duration, file_size);
            (byte, 0)
        };

        info!(
            "Seek: position {:?}, target_byte {}, file_size {}, audio_data_start {}",
            position, target_byte, file_size, audio_data_start
        );

        if buffer.is_buffered(target_byte) {
            // Data is buffered - create new buffer with headers + data from frame boundary
            info!(
                "Seek within buffer, creating decoder buffer from byte {}",
                target_byte
            );

            // Increment generation FIRST to invalidate old position listeners
            // This prevents stale updates during the blocking data copy below
            let gen = self
                .position_generation
                .fetch_add(1, std::sync::atomic::Ordering::SeqCst)
                + 1;

            // Cancel old streaming source to stop the decoder
            if let Some(source) = &self.current_streaming_source {
                if let Ok(guard) = source.lock() {
                    guard.cancel();
                }
            }

            // Drop old stream
            if let Some(stream) = self.stream.take() {
                drop(stream);
            }

            // Create a new buffer with headers prepended + audio data from frame boundary
            let seek_buffer = create_sparse_buffer();

            // Prepend FLAC headers if available
            let mut write_pos = 0u64;
            if let Some(ref headers) = self.current_flac_headers {
                seek_buffer.append_at(0, headers);
                write_pos = headers.len() as u64;
            }

            // Copy buffered audio data from target_byte onwards
            // Read from the original buffer and write to seek_buffer

            // Check what bytes are at audio_data_start (should be first frame sync code ff f8)
            buffer.seek(audio_data_start);
            let mut first_frame = [0u8; 4];
            if buffer.read(&mut first_frame).is_some() {
                info!(
                    "Original buffer at audio_data_start {}: {:02x} {:02x} {:02x} {:02x}",
                    audio_data_start,
                    first_frame[0],
                    first_frame[1],
                    first_frame[2],
                    first_frame[3]
                );
            }

            // Check what bytes are at target_byte in original buffer
            buffer.seek(target_byte);
            let mut orig_check = [0u8; 4];
            if let Some(n) = buffer.read(&mut orig_check) {
                info!(
                    "Original buffer at target_byte {}: {:02x} {:02x} {:02x} {:02x} (read {})",
                    target_byte, orig_check[0], orig_check[1], orig_check[2], orig_check[3], n
                );
            }
            buffer.seek(target_byte); // Reset after check
            let mut temp_buf = vec![0u8; 65536]; // 64KB chunks
            loop {
                match buffer.read(&mut temp_buf) {
                    Some(0) => break, // EOF
                    Some(n) => {
                        seek_buffer.append_at(write_pos, &temp_buf[..n]);
                        write_pos += n as u64;
                    }
                    None => break, // Cancelled
                }
            }
            seek_buffer.mark_eof();

            // Check first bytes of audio data to verify sync code
            let audio_start_pos = self
                .current_flac_headers
                .as_ref()
                .map(|h| h.len() as u64)
                .unwrap_or(0);
            seek_buffer.seek(audio_start_pos);
            let mut check_buf = [0u8; 4];
            if let Some(n) = seek_buffer.read(&mut check_buf) {
                info!(
                            "Seek buffer: {} bytes total, headers_len={}, first audio bytes: {:02x} {:02x} {:02x} {:02x} (read {})",
                            write_pos,
                            audio_start_pos,
                            check_buf[0], check_buf[1], check_buf[2], check_buf[3],
                            n
                        );
            }
            // Reset seek position to start for decoder
            seek_buffer.seek(0);

            // Start decoder on the seek buffer with adjusted seektable
            let (mut sink, source) = create_streaming_pair(44100, 2);

            // Parse and adjust seektable for seek position
            // target_byte is absolute file offset, seektable offsets are relative to audio_data_start
            let rel_seek_byte = target_byte.saturating_sub(audio_data_start);
            let original_seektable: Vec<crate::audio_codec::SeekEntry> = self
                .current_seektable_json
                .as_ref()
                .and_then(|json| serde_json::from_str(json).ok())
                .unwrap_or_default();

            // Log some seektable entries to debug
            if !original_seektable.is_empty() {
                info!(
                    "Seek: seektable[0] sample={} byte={}, seektable[last] sample={} byte={}",
                    original_seektable[0].sample,
                    original_seektable[0].byte,
                    original_seektable.last().unwrap().sample,
                    original_seektable.last().unwrap().byte,
                );
            }
            info!(
                "Seek: seektable has {} entries, rel_seek_byte={}",
                original_seektable.len(),
                rel_seek_byte
            );

            std::thread::spawn(move || {
                if let Err(e) =
                    crate::audio_codec::decode_audio_streaming_simple(seek_buffer, &mut sink)
                {
                    error!("Seek decode failed: {}", e);
                }
            });

            // Wait for decoder to start
            tokio::time::sleep(std::time::Duration::from_millis(50)).await;

            let source = Arc::new(Mutex::new(source));
            let (source_sample_rate, source_channels) = {
                let guard = source.lock().unwrap();
                (guard.sample_rate(), guard.channels())
            };

            // Create new channels
            let (position_tx, position_rx) = mpsc::channel();
            let (completion_tx, completion_rx) = mpsc::channel();
            let (position_tx_async, mut position_rx_async) = tokio_mpsc::unbounded_channel();
            let (completion_tx_async, mut completion_rx_async) = tokio_mpsc::unbounded_channel();

            // Bridge channels
            let position_rx_clone = position_rx;
            tokio::spawn(async move {
                let position_rx = Arc::new(std::sync::Mutex::new(position_rx_clone));
                loop {
                    let rx = position_rx.clone();
                    match tokio::task::spawn_blocking(move || rx.lock().unwrap().recv()).await {
                        Ok(Ok(pos)) => {
                            let _ = position_tx_async.send(pos);
                        }
                        Ok(Err(_)) | Err(_) => break,
                    }
                }
            });

            let completion_rx_clone = completion_rx;
            tokio::spawn(async move {
                let completion_rx = Arc::new(std::sync::Mutex::new(completion_rx_clone));
                loop {
                    let rx = completion_rx.clone();
                    match tokio::task::spawn_blocking(move || rx.lock().unwrap().recv()).await {
                        Ok(Ok(())) => {
                            let _ = completion_tx_async.send(());
                        }
                        Ok(Err(_)) | Err(_) => break,
                    }
                }
            });

            // Create new stream
            let stream = match self.audio_output.create_stream(
                source.clone(),
                source_sample_rate,
                source_channels,
                position_tx,
                completion_tx,
            ) {
                Ok(stream) => stream,
                Err(e) => {
                    error!("Failed to create stream after seek: {:?}", e);
                    return;
                }
            };

            if let Err(e) = stream.play() {
                error!("Failed to start stream after seek: {:?}", e);
                return;
            }

            let was_paused = self.is_paused;
            if was_paused {
                self.audio_output
                    .send_command(crate::playback::cpal_output::AudioCommand::Pause);
            } else {
                self.audio_output
                    .send_command(crate::playback::cpal_output::AudioCommand::Play);
            }

            self.stream = Some(stream);
            self.current_streaming_source = Some(source.clone());

            // gen was already incremented at the start of the seek
            *self.current_position_shared.lock().unwrap() = Some(position);

            // Spawn position/completion listener
            // Note: positions from the decoder are relative to the seek buffer (starting from 0),
            // so we add the seek target to get the actual track position
            let seek_offset = position;
            let progress_tx = self.progress_tx.clone();
            let track_id = track.id.clone();
            let current_position_for_listener = self.current_position_shared.clone();
            let position_generation = self.position_generation.clone();
            let streaming_source_for_stats = Some(source);

            tokio::spawn(async move {
                loop {
                    tokio::select! {
                        Some(pos) = position_rx_async.recv() => {
                            // Only update if this listener is still current
                            if position_generation.load(std::sync::atomic::Ordering::SeqCst) == gen {
                                // Add seek offset to get actual track position
                                let actual_pos = seek_offset + pos;
                                *current_position_for_listener.lock().unwrap() = Some(actual_pos);
                                let _ = progress_tx.send(PlaybackProgress::PositionUpdate {
                                    position: actual_pos,
                                    track_id: track_id.clone(),
                                });
                            }
                        }
                        Some(()) = completion_rx_async.recv() => {
                            if position_generation.load(std::sync::atomic::Ordering::SeqCst) == gen {
                                // Get decode error count from streaming source
                                let error_count = streaming_source_for_stats
                                    .as_ref()
                                    .and_then(|s| s.lock().ok())
                                    .map(|g| g.decode_error_count())
                                    .unwrap_or(0);

                                info!("Track completed after seek: {} ({} decode errors)", track_id, error_count);
                                let _ = progress_tx.send(PlaybackProgress::TrackCompleted {
                                    track_id: track_id.clone(),
                                });
                                let _ = progress_tx.send(PlaybackProgress::DecodeStats {
                                    track_id: track_id.clone(),
                                    error_count,
                                });
                            }
                            break;
                        }
                        else => break,
                    }
                }
            });

            let _ = self.progress_tx.send(PlaybackProgress::Seeked {
                position,
                track_id: track.id.clone(),
                was_paused,
            });
        } else {
            // Data not buffered - start new download from target position
            info!(
                "Seek past buffer: byte {} not buffered, buffered ranges: {:?}",
                target_byte,
                buffer.get_ranges()
            );

            // Start new download at target byte (if we have source path)
            if let Some(source_path) = &self.current_streaming_source_path {
                let read_buffer = buffer.clone();
                let read_path = source_path.clone();
                let start_offset = target_byte;

                // Spawn new reader task starting at target offset
                tokio::spawn(async move {
                    use tokio::io::{AsyncReadExt, AsyncSeekExt};

                    let mut file = match tokio::fs::File::open(&read_path).await {
                        Ok(f) => f,
                        Err(e) => {
                            error!("Failed to open file for seek download {}: {}", read_path, e);
                            return;
                        }
                    };

                    if let Err(e) = file.seek(std::io::SeekFrom::Start(start_offset)).await {
                        error!("Failed to seek for download: {}", e);
                        return;
                    }

                    let mut buffer_pos = start_offset;
                    let mut chunk = vec![0u8; 65536];

                    loop {
                        if read_buffer.is_cancelled() {
                            return;
                        }

                        match file.read(&mut chunk).await {
                            Ok(0) => break, // EOF
                            Ok(n) => {
                                read_buffer.append_at(buffer_pos, &chunk[..n]);
                                buffer_pos += n as u64;
                            }
                            Err(e) => {
                                error!("Read error during seek download: {}", e);
                                break;
                            }
                        }
                    }
                });

                // Seek buffer to target position (will block until data arrives)
                buffer.seek(target_byte);

                // Cancel old streaming source to stop the decoder
                if let Some(source) = &self.current_streaming_source {
                    if let Ok(guard) = source.lock() {
                        guard.cancel();
                    }
                }

                // Drop old stream
                if let Some(stream) = self.stream.take() {
                    drop(stream);
                }

                // Wait briefly for new download to start buffering
                tokio::time::sleep(std::time::Duration::from_millis(100)).await;

                // Create seek buffer with headers prepended
                let seek_buffer = create_sparse_buffer();
                let mut write_pos = 0u64;
                if let Some(ref headers) = self.current_flac_headers {
                    seek_buffer.append_at(0, headers);
                    write_pos = headers.len() as u64;
                }

                // Copy downloaded data to seek buffer
                buffer.seek(target_byte);
                let mut temp_buf = vec![0u8; 65536];
                loop {
                    match buffer.read(&mut temp_buf) {
                        Some(0) => break,
                        Some(n) => {
                            seek_buffer.append_at(write_pos, &temp_buf[..n]);
                            write_pos += n as u64;
                        }
                        None => break,
                    }
                }
                seek_buffer.mark_eof();

                // Restart decoder from seek buffer with adjusted seektable
                let (mut sink, source) = create_streaming_pair(44100, 2);

                // Parse and adjust seektable for seek position
                std::thread::spawn(move || {
                    if let Err(e) =
                        crate::audio_codec::decode_audio_streaming_simple(seek_buffer, &mut sink)
                    {
                        error!("Seek past buffer decode failed: {}", e);
                    }
                });

                // Wait for decoder to start
                tokio::time::sleep(std::time::Duration::from_millis(50)).await;

                let source = Arc::new(Mutex::new(source));
                let (source_sample_rate, source_channels) = {
                    let guard = source.lock().unwrap();
                    (guard.sample_rate(), guard.channels())
                };

                // Create new channels
                let (position_tx, position_rx) = mpsc::channel();
                let (completion_tx, completion_rx) = mpsc::channel();
                let (position_tx_async, mut position_rx_async) = tokio_mpsc::unbounded_channel();
                let (completion_tx_async, mut completion_rx_async) =
                    tokio_mpsc::unbounded_channel();

                // Bridge channels
                let position_rx_clone = position_rx;
                tokio::spawn(async move {
                    let position_rx = Arc::new(std::sync::Mutex::new(position_rx_clone));
                    loop {
                        let rx = position_rx.clone();
                        match tokio::task::spawn_blocking(move || rx.lock().unwrap().recv()).await {
                            Ok(Ok(pos)) => {
                                let _ = position_tx_async.send(pos);
                            }
                            Ok(Err(_)) | Err(_) => break,
                        }
                    }
                });

                let completion_rx_clone = completion_rx;
                tokio::spawn(async move {
                    let completion_rx = Arc::new(std::sync::Mutex::new(completion_rx_clone));
                    loop {
                        let rx = completion_rx.clone();
                        match tokio::task::spawn_blocking(move || rx.lock().unwrap().recv()).await {
                            Ok(Ok(())) => {
                                let _ = completion_tx_async.send(());
                            }
                            Ok(Err(_)) | Err(_) => break,
                        }
                    }
                });

                // Create new stream
                let stream = match self.audio_output.create_stream(
                    source.clone(),
                    source_sample_rate,
                    source_channels,
                    position_tx,
                    completion_tx,
                ) {
                    Ok(stream) => stream,
                    Err(e) => {
                        error!("Failed to create stream after seek past buffer: {:?}", e);
                        return;
                    }
                };

                if let Err(e) = stream.play() {
                    error!("Failed to start stream after seek past buffer: {:?}", e);
                    return;
                }

                let was_paused = self.is_paused;
                if was_paused {
                    self.audio_output
                        .send_command(crate::playback::cpal_output::AudioCommand::Pause);
                } else {
                    self.audio_output
                        .send_command(crate::playback::cpal_output::AudioCommand::Play);
                }

                self.stream = Some(stream);
                self.current_streaming_source = Some(source);

                // Increment generation to invalidate any old position listeners
                let gen = self
                    .position_generation
                    .fetch_add(1, std::sync::atomic::Ordering::SeqCst)
                    + 1;

                *self.current_position_shared.lock().unwrap() = Some(position);

                // Spawn position/completion listener
                // Note: positions from the decoder are relative to the seek point,
                // so we add the seek target to get the actual track position
                let seek_offset = position;
                let progress_tx = self.progress_tx.clone();
                let track_id = track.id.clone();
                let current_position_for_listener = self.current_position_shared.clone();
                let position_generation = self.position_generation.clone();

                tokio::spawn(async move {
                    loop {
                        tokio::select! {
                            Some(pos) = position_rx_async.recv() => {
                                if position_generation.load(std::sync::atomic::Ordering::SeqCst) == gen {
                                    let actual_pos = seek_offset + pos;
                                    *current_position_for_listener.lock().unwrap() = Some(actual_pos);
                                    let _ = progress_tx.send(PlaybackProgress::PositionUpdate {
                                        position: actual_pos,
                                        track_id: track_id.clone(),
                                    });
                                }
                            }
                            Some(()) = completion_rx_async.recv() => {
                                if position_generation.load(std::sync::atomic::Ordering::SeqCst) == gen {
                                    info!("Track completed after seek past buffer: {}", track_id);
                                    let _ = progress_tx.send(PlaybackProgress::TrackCompleted {
                                        track_id: track_id.clone(),
                                    });
                                }
                                break;
                            }
                            else => break,
                        }
                    }
                });

                let _ = self.progress_tx.send(PlaybackProgress::Seeked {
                    position,
                    track_id: track.id.clone(),
                    was_paused,
                });
            }
        }
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
pub fn calculate_byte_offset_for_seek(
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
pub fn find_frame_boundary_for_seek(
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
pub fn find_frame_boundary(
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
pub fn pregap_seek_position(
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
}
