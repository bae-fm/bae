use crate::db::DbTrack;
use crate::encryption::EncryptionService;
use crate::library::LibraryManager;
use crate::playback::cpal_output::AudioOutput;
use crate::playback::data_source::{
    AudioDataReader, AudioReadConfig, CloudStorageReader, LocalFileReader,
};
use crate::playback::progress::{PlaybackProgress, PlaybackProgressHandle};
use crate::playback::sparse_buffer::{create_sparse_buffer, SharedSparseBuffer};
use crate::playback::{create_streaming_pair, PcmSource, PlaybackError, StreamingPcmSource};
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
    current_pcm_source: Option<Arc<PcmSource>>,
    next_pcm_source: Option<Arc<PcmSource>>,
    next_track_id: Option<String>,
    /// Metadata duration for preloaded track (decoded duration computed when played)
    next_duration: Option<std::time::Duration>,
    next_pregap_ms: Option<i64>,
    /// Current streaming source (if using streaming playback)
    current_streaming_source: Option<Arc<Mutex<StreamingPcmSource>>>,
    /// Current sparse buffer for smart seek support
    current_sparse_buffer: Option<SharedSparseBuffer>,
    /// File size or byte range size for current streaming track (for seek calculations)
    current_streaming_file_size: Option<u64>,
    /// Source path for current streaming track (for starting new downloads on seek)
    current_streaming_source_path: Option<String>,
    /// FLAC headers for current streaming track (for smart seek - prepended on decoder restart)
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
                    current_pcm_source: None,
                    next_pcm_source: None,
                    next_track_id: None,
                    next_duration: None,
                    next_pregap_ms: None,
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
                    self.next_pcm_source = None;
                    self.next_track_id = None;
                    self.next_duration = None;
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
                    if let Some((preloaded_source, preloaded_track_id)) =
                        self.next_pcm_source.take().zip(self.next_track_id.take())
                    {
                        let preloaded_duration = self
                            .next_duration
                            .take()
                            .expect("Preloaded track has no duration");
                        let preloaded_pregap_ms = self.next_pregap_ms.take();
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
                        let track = match self.library_manager.get_track(&preloaded_track_id).await
                        {
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
                        self.play_track_with_source(
                            &preloaded_track_id,
                            track,
                            preloaded_source,
                            preloaded_duration,
                            preloaded_pregap_ms,
                            false, // Manual next: skip pregap
                        )
                        .await;
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
                    if let Some((preloaded_source, preloaded_track_id)) =
                        self.next_pcm_source.take().zip(self.next_track_id.take())
                    {
                        let preloaded_duration = self
                            .next_duration
                            .take()
                            .expect("Preloaded track has no duration");
                        let preloaded_pregap_ms = self.next_pregap_ms.take();
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
                        let track = match self.library_manager.get_track(&preloaded_track_id).await
                        {
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
                        self.play_track_with_source(
                            &preloaded_track_id,
                            track,
                            preloaded_source,
                            preloaded_duration,
                            preloaded_pregap_ms,
                            true, // Natural transition: play pregap
                        )
                        .await;
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
                                self.next_pcm_source = None;
                                self.next_track_id = None;
                                self.next_duration = None;
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

        // Find the audio file
        let files = match self
            .library_manager
            .get_files_for_release(&track.release_id)
            .await
        {
            Ok(files) => files,
            Err(e) => {
                error!("Failed to get files: {}", e);
                self.stop().await;
                return;
            }
        };

        let audio_file = match files.iter().find(|f| {
            let ext = f.format.to_lowercase();
            (ext == "flac" || ext == "mp3" || ext == "wav" || ext == "ogg")
                && f.source_path.is_some()
        }) {
            Some(f) => f,
            None => {
                error!("No audio file with source_path found");
                self.stop().await;
                return;
            }
        };

        let source_path = audio_file.source_path.clone().unwrap();
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

        let seektable: Option<Vec<crate::audio_codec::SeekEntry>> = audio_format
            .as_ref()
            .and_then(|af| af.seektable_json.as_ref())
            .and_then(|json| serde_json::from_str(json).ok());

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

        // Spawn decoder thread
        let decoder_buffer = buffer.clone();
        let decoder_headers = flac_headers.clone();
        let decoder_seektable = seektable.clone();
        std::thread::spawn(move || {
            if let Err(e) = crate::audio_codec::decode_audio_streaming_with_seektable(
                decoder_buffer,
                &mut sink,
                decoder_headers,
                decoder_seektable,
            ) {
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
        self.current_seektable_json = audio_format
            .as_ref()
            .and_then(|af| af.seektable_json.clone());
        self.current_audio_data_start = audio_format
            .as_ref()
            .and_then(|af| af.audio_data_start.map(|s| s as u64));

        let source = Arc::new(Mutex::new(source));
        let (source_sample_rate, source_channels) = {
            let guard = source.lock().unwrap();
            (guard.sample_rate(), guard.channels())
        };

        let start_position = calculate_start_position(pregap_ms, is_natural_transition);

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
        let stream = match self.audio_output.create_streaming_stream(
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
                            info!("Streaming track completed: {}", track_id_owned);
                            let _ = progress_tx.send(PlaybackProgress::TrackCompleted { track_id: track_id_owned.clone() });
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
    async fn decode_audio_bytes(
        audio_data: &[u8],
        frame_offset_samples: Option<i64>,
        exact_sample_count: Option<i64>,
    ) -> Result<Arc<PcmSource>, PlaybackError> {
        // Check for FLAC header (for backwards compatibility)
        if audio_data.len() >= 4 && &audio_data[0..4] == b"fLaC" {
            // FLAC file - proceed
        } else if audio_data.len() < 4 {
            return Err(PlaybackError::flac("Audio data too short"));
        }
        // FFmpeg can handle format detection automatically
        let audio_data = audio_data.to_vec();
        let decoded = tokio::task::spawn_blocking(move || {
            crate::audio_codec::decode_audio(&audio_data, None, None)
        })
        .await
        .map_err(PlaybackError::task)?
        .map_err(PlaybackError::flac)?;

        let channels = decoded.channels as usize;

        // Skip lead-in samples if frame_offset_samples is set
        // (Due to FLAC frame alignment, extracted bytes may start before the actual track)
        let start_idx = frame_offset_samples
            .filter(|&s| s > 0)
            .map(|s| s as usize * channels)
            .unwrap_or(0);

        // Trim to exact_sample_count if set (for gapless playback)
        let end_idx = exact_sample_count
            .filter(|&s| s > 0)
            .map(|s| start_idx + (s as usize * channels))
            .unwrap_or(decoded.samples.len());

        let samples = decoded.samples[start_idx..end_idx.min(decoded.samples.len())].to_vec();

        Ok(Arc::new(PcmSource::new(
            samples,
            decoded.sample_rate,
            decoded.channels,
            decoded.bits_per_sample,
        )))
    }
    async fn play_track_with_source(
        &mut self,
        track_id: &str,
        track: DbTrack,
        pcm_source: Arc<PcmSource>,
        track_duration: std::time::Duration,
        pregap_ms: Option<i64>,
        is_natural_transition: bool,
    ) {
        let start_position = calculate_start_position(pregap_ms, is_natural_transition);

        info!(
            "Starting playback for track: {} at position {:?} (natural_transition: {}, pregap_ms: {:?})",
            track_id, start_position, is_natural_transition, pregap_ms
        );

        // Seek to start position if not starting from the beginning
        if !start_position.is_zero() {
            pcm_source.seek(start_position);
        }

        let decoded_duration = pcm_source.duration();
        self.current_pcm_source = Some(pcm_source.clone());
        self.current_pregap_ms = pregap_ms;
        if let Some(stream) = self.stream.take() {
            drop(stream);
        }
        let (position_tx, position_rx) = mpsc::channel();
        let (completion_tx, completion_rx) = mpsc::channel();
        let (position_tx_async, mut position_rx_async) = tokio_mpsc::unbounded_channel();
        let (completion_tx_async, mut completion_rx_async) = tokio_mpsc::unbounded_channel();
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
        let stream = match self
            .audio_output
            .create_stream(pcm_source, position_tx, completion_tx)
        {
            Ok(stream) => {
                info!("Audio stream created successfully");
                stream
            }
            Err(e) => {
                error!("Failed to create audio stream: {:?}", e);
                self.stop().await;
                return;
            }
        };
        if let Err(e) = stream.play() {
            error!("Failed to start stream: {:?}", e);
            self.stop().await;
            return;
        }
        info!("Stream started, sending Play command");
        self.audio_output
            .send_command(crate::playback::cpal_output::AudioCommand::Play);
        self.stream = Some(stream);
        self.current_track = Some(track.clone());
        self.current_position = Some(start_position);
        self.current_duration = Some(track_duration);
        self.current_decoded_duration = Some(decoded_duration);
        self.is_paused = false;
        *self.current_position_shared.lock().unwrap() = Some(start_position);
        let _ = self.progress_tx.send(PlaybackProgress::StateChanged {
            state: PlaybackState::Playing {
                track: track.clone(),
                position: start_position,
                duration: Some(track_duration),
                decoded_duration,
                pregap_ms,
            },
        });
        let progress_tx = self.progress_tx.clone();
        let track_id = track_id.to_string();
        let track_duration_for_completion = decoded_duration;
        let current_position_for_listener = self.current_position_shared.clone();
        tokio::spawn(async move {
            info!(
                "Play: Spawning completion listener task for track: {}",
                track_id
            );
            info!("Play: Task started, waiting for position updates and completion");
            loop {
                tokio::select! {
                    Some(position) = position_rx_async.recv() => { *
                    current_position_for_listener.lock().unwrap() = Some(position); let _
                    = progress_tx.send(PlaybackProgress::PositionUpdate { position,
                    track_id : track_id.clone(), }); } Some(()) = completion_rx_async
                    .recv() => { info!("Track completed: {}", track_id); let _ =
                    progress_tx.send(PlaybackProgress::PositionUpdate { position :
                    track_duration_for_completion, track_id : track_id.clone(), }); let _
                    = progress_tx.send(PlaybackProgress::TrackCompleted { track_id :
                    track_id.clone(), }); break; } else => {
                    info!("Play: Channels closed, exiting"); break; }
                }
            }
            info!(
                "Play: Completion listener task exiting for track: {}",
                track_id
            );
        });
        if let Some(next_track_id) = self.queue.front().cloned() {
            self.preload_next_track(&next_track_id).await;
        }
    }
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

        // Load audio format for CUE/FLAC track metadata
        let (pregap_ms, frame_offset_samples, exact_sample_count) = match self
            .library_manager
            .get_audio_format_by_track_id(track_id)
            .await
        {
            Ok(Some(af)) => (af.pregap_ms, af.frame_offset_samples, af.exact_sample_count),
            Ok(None) => (None, None, None),
            Err(e) => {
                error!("Failed to get audio format for preload: {}", e);
                (None, None, None)
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
        let pcm_source = match &storage_profile {
            None => {
                match self
                    .load_audio_from_source_path(track_id, &track.release_id)
                    .await
                {
                    Ok(data) => {
                        match Self::decode_audio_bytes(
                            &data,
                            frame_offset_samples,
                            exact_sample_count,
                        )
                        .await
                        {
                            Ok(source) => source,
                            Err(e) => {
                                error!("Failed to decode FLAC for preload: {}", e);
                                return;
                            }
                        }
                    }
                    Err(e) => {
                        error!("Failed to load audio from source path for preload: {}", e);
                        return;
                    }
                }
            }
            Some(profile) => {
                match self
                    .load_audio_from_storage(track_id, &track.release_id, profile)
                    .await
                {
                    Ok(data) => {
                        match Self::decode_audio_bytes(
                            &data,
                            frame_offset_samples,
                            exact_sample_count,
                        )
                        .await
                        {
                            Ok(source) => source,
                            Err(e) => {
                                error!("Failed to decode FLAC for preload: {}", e);
                                return;
                            }
                        }
                    }
                    Err(e) => {
                        error!("Failed to load audio from storage for preload: {}", e);
                        return;
                    }
                }
            }
        };
        let duration = track
            .duration_ms
            .map(|ms| std::time::Duration::from_millis(ms as u64))
            .unwrap_or_else(|| panic!("Cannot preload track {} without duration", track_id));
        self.next_pcm_source = Some(pcm_source);
        self.next_track_id = Some(track_id.to_string());
        self.next_duration = Some(duration);
        self.next_pregap_ms = pregap_ms;
        info!("Preloaded next track: {}", track_id);
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
        self.current_pcm_source = None;
        self.current_position = None;
        self.current_duration = None;
        self.next_pcm_source = None;
        self.next_track_id = None;
        self.next_duration = None;
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
        // Check if we're using streaming playback with smart seek support
        if self.current_streaming_source.is_some() {
            if let (Some(buffer), Some(file_size), Some(track)) = (
                &self.current_sparse_buffer,
                self.current_streaming_file_size,
                &self.current_track,
            ) {
                // Check for same-position seek (difference < 100ms)
                let current_position = self
                    .current_position_shared
                    .lock()
                    .unwrap()
                    .unwrap_or(std::time::Duration::ZERO);
                let position_diff = position.abs_diff(current_position);
                if position_diff < std::time::Duration::from_millis(100) {
                    trace!(
                        "Streaming seek: Skipping seek to same position (difference: {:?} < 100ms)",
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
                let (target_byte, _sample_offset) = if let (
                    Some(sample_rate),
                    Some(ref seektable),
                ) =
                    (self.current_sample_rate, &self.current_seektable_json)
                {
                    if let Some((frame_byte, offset)) =
                        find_frame_boundary_for_seek(position, sample_rate, seektable)
                    {
                        // frame_byte is relative to audio data start, convert to file offset
                        let file_byte = audio_data_start + frame_byte;
                        info!(
                            "Smart seek using seektable: position {:?}, frame_byte {}, file_byte {}, sample_offset {}",
                            position, frame_byte, file_byte, offset
                        );
                        (file_byte, offset)
                    } else {
                        let byte =
                            calculate_byte_offset_for_seek(position, track_duration, file_size);
                        (byte, 0)
                    }
                } else {
                    let byte = calculate_byte_offset_for_seek(position, track_duration, file_size);
                    (byte, 0)
                };

                info!(
                    "Smart seek: position {:?}, target_byte {}, file_size {}, audio_data_start {}",
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
                    buffer.seek(target_byte);
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

                    // Start decoder on the seek buffer
                    let (mut sink, source) = create_streaming_pair(44100, 2);

                    std::thread::spawn(move || {
                        if let Err(e) = crate::audio_codec::decode_audio_streaming_sparse(
                            seek_buffer,
                            &mut sink,
                        ) {
                            error!("Smart seek decode failed: {}", e);
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
                    let (position_tx_async, mut position_rx_async) =
                        tokio_mpsc::unbounded_channel();
                    let (completion_tx_async, mut completion_rx_async) =
                        tokio_mpsc::unbounded_channel();

                    // Bridge channels
                    let position_rx_clone = position_rx;
                    tokio::spawn(async move {
                        let position_rx = Arc::new(std::sync::Mutex::new(position_rx_clone));
                        loop {
                            let rx = position_rx.clone();
                            match tokio::task::spawn_blocking(move || rx.lock().unwrap().recv())
                                .await
                            {
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
                            match tokio::task::spawn_blocking(move || rx.lock().unwrap().recv())
                                .await
                            {
                                Ok(Ok(())) => {
                                    let _ = completion_tx_async.send(());
                                }
                                Ok(Err(_)) | Err(_) => break,
                            }
                        }
                    });

                    // Create new stream
                    let stream = match self.audio_output.create_streaming_stream(
                        source.clone(),
                        source_sample_rate,
                        source_channels,
                        position_tx,
                        completion_tx,
                    ) {
                        Ok(stream) => stream,
                        Err(e) => {
                            error!("Failed to create stream after smart seek: {:?}", e);
                            return;
                        }
                    };

                    if let Err(e) = stream.play() {
                        error!("Failed to start stream after smart seek: {:?}", e);
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
                                        info!("Track completed after smart seek: {}", track_id);
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

                    return;
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
                                    error!(
                                        "Failed to open file for seek download {}: {}",
                                        read_path, e
                                    );
                                    return;
                                }
                            };

                            if let Err(e) = file.seek(std::io::SeekFrom::Start(start_offset)).await
                            {
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

                        // Restart decoder from new buffer position
                        let (mut sink, source) = create_streaming_pair(44100, 2);
                        let decoder_buffer = buffer.clone();

                        std::thread::spawn(move || {
                            if let Err(e) = crate::audio_codec::decode_audio_streaming_sparse(
                                decoder_buffer,
                                &mut sink,
                            ) {
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
                        let (position_tx_async, mut position_rx_async) =
                            tokio_mpsc::unbounded_channel();
                        let (completion_tx_async, mut completion_rx_async) =
                            tokio_mpsc::unbounded_channel();

                        // Bridge channels
                        let position_rx_clone = position_rx;
                        tokio::spawn(async move {
                            let position_rx = Arc::new(std::sync::Mutex::new(position_rx_clone));
                            loop {
                                let rx = position_rx.clone();
                                match tokio::task::spawn_blocking(move || rx.lock().unwrap().recv())
                                    .await
                                {
                                    Ok(Ok(pos)) => {
                                        let _ = position_tx_async.send(pos);
                                    }
                                    Ok(Err(_)) | Err(_) => break,
                                }
                            }
                        });

                        let completion_rx_clone = completion_rx;
                        tokio::spawn(async move {
                            let completion_rx =
                                Arc::new(std::sync::Mutex::new(completion_rx_clone));
                            loop {
                                let rx = completion_rx.clone();
                                match tokio::task::spawn_blocking(move || rx.lock().unwrap().recv())
                                    .await
                                {
                                    Ok(Ok(())) => {
                                        let _ = completion_tx_async.send(());
                                    }
                                    Ok(Err(_)) | Err(_) => break,
                                }
                            }
                        });

                        // Create new stream
                        let stream = match self.audio_output.create_streaming_stream(
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

                        return;
                    }
                }
            }

            // Fallback: streaming without smart seek support
            info!(
                "Seek not fully supported for this streaming track. Requested position: {:?}",
                position
            );

            let _ = self.progress_tx.send(PlaybackProgress::SeekSkipped {
                requested_position: position,
                current_position: self
                    .current_position_shared
                    .lock()
                    .unwrap()
                    .unwrap_or(std::time::Duration::ZERO),
            });
            return;
        }

        let (track_id, pcm_source) = match (&self.current_track, &self.current_pcm_source) {
            (Some(track), Some(source)) => (track.id.clone(), source.clone()),
            _ => {
                error!("Cannot seek: no track playing or PCM source not available");
                return;
            }
        };
        let current_position = self
            .current_position_shared
            .lock()
            .unwrap()
            .unwrap_or(std::time::Duration::ZERO);
        let position_diff = position.abs_diff(current_position);
        info!(
            "Seeking to position: {:?}, current position: {:?}, difference: {:?}",
            position, current_position, position_diff
        );
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
        if let Some(stream) = self.stream.take() {
            trace!("Seek: Dropping old stream");
            drop(stream);
            trace!("Seek: Old stream dropped");
        } else {
            trace!("Seek: No stream to drop");
        }
        // Use decoded_duration for validation since UI sends positions that include pregap
        // (decoded audio includes pregap from INDEX 00, while track metadata duration excludes it)
        let decoded_duration = self
            .current_decoded_duration
            .expect("Cannot seek: track has no decoded duration");
        if let Err(e) = validate_seek_position(position, decoded_duration) {
            error!("Cannot seek past end of track: {:?}", e);
            let _ = self.progress_tx.send(PlaybackProgress::SeekError {
                requested_position: position,
                track_duration: decoded_duration,
            });
            return;
        }
        pcm_source.seek(position);
        trace!("Seek: Creating new channels");
        let (position_tx, position_rx) = mpsc::channel();
        let (completion_tx, completion_rx) = mpsc::channel();
        let (position_tx_async, mut position_rx_async) = tokio_mpsc::unbounded_channel();
        let (completion_tx_async, mut completion_rx_async) = tokio_mpsc::unbounded_channel();
        trace!("Seek: Created new channels for position updates");
        let position_rx_clone = position_rx;
        tokio::spawn(async move {
            let position_rx = Arc::new(std::sync::Mutex::new(position_rx_clone));
            trace!("Seek: Bridge position task started");
            loop {
                let rx = position_rx.clone();
                match tokio::task::spawn_blocking(move || rx.lock().unwrap().recv()).await {
                    Ok(Ok(pos)) => {
                        let send_result = position_tx_async.send(pos);
                        if let Err(e) = send_result {
                            error!("Seek: Bridge failed to forward position update: {:?}", e);
                            break;
                        }
                    }
                    Ok(Err(_)) | Err(_) => {
                        trace!("Seek: Bridge position channel closed");
                        break;
                    }
                }
            }
            trace!("Seek: Bridge position task exiting");
        });
        let completion_rx_clone = completion_rx;
        tokio::spawn(async move {
            let completion_rx = Arc::new(std::sync::Mutex::new(completion_rx_clone));
            loop {
                let rx = completion_rx.clone();
                match tokio::task::spawn_blocking(move || rx.lock().unwrap().recv()).await {
                    Ok(Ok(())) => {
                        trace!("Seek: Bridge received completion signal");
                        let _ = completion_tx_async.send(());
                    }
                    Ok(Err(_)) | Err(_) => {
                        trace!("Seek: Bridge completion channel closed");
                        break;
                    }
                }
            }
        });
        let progress_tx_for_task = self.progress_tx.clone();
        let track_id_for_task = track_id.clone();
        let track_duration_for_completion = decoded_duration;
        let current_position_for_seek_listener = self.current_position_shared.clone();
        let was_paused = self.is_paused;
        tokio::spawn(async move {
            trace!(
                "Seek: Spawning completion listener task for track: {}",
                track_id_for_task
            );
            trace!("Seek: Task started, waiting for position updates and completion");
            loop {
                tokio::select! {
                    Some(pos) = position_rx_async.recv() => {
                    *
                    current_position_for_seek_listener.lock().unwrap() = Some(pos); let
                    send_result = progress_tx_for_task
                    .send(PlaybackProgress::PositionUpdate { position : pos, track_id :
                    track_id_for_task.clone(), }); if let Err(e) = send_result {
                    error!("Seek: Listener failed to send position update: {:?}", e); } }
                    Some(()) = completion_rx_async.recv() => {
                    info!("Seek: Track completed: {}", track_id_for_task); let _ =
                    progress_tx_for_task.send(PlaybackProgress::PositionUpdate { position
                    : track_duration_for_completion, track_id : track_id_for_task
                    .clone(), }); let _ = progress_tx_for_task
                    .send(PlaybackProgress::TrackCompleted { track_id : track_id_for_task
                    .clone(), }); break; } else => {
                    trace!("Seek: Channels closed, exiting listener task"); break; }
                }
            }
            trace!(
                "Seek: Completion listener task exiting for track: {}",
                track_id_for_task
            );
        });
        trace!("Seek: Creating new audio stream");
        let stream = match self
            .audio_output
            .create_stream(pcm_source, position_tx, completion_tx)
        {
            Ok(stream) => {
                trace!("Seek: Audio stream created successfully");
                stream
            }
            Err(e) => {
                error!("Failed to create audio stream for seek: {:?}", e);
                self.stop().await;
                return;
            }
        };
        if let Err(e) = stream.play() {
            error!("Failed to start stream after seek: {:?}", e);
            self.stop().await;
            return;
        }
        trace!("Seek: Stream started successfully");
        self.stream = Some(stream);
        *self.current_position_shared.lock().unwrap() = Some(position);
        trace!("Seek: Updated shared position to: {:?}", position);
        if was_paused {
            trace!("Seek: Was paused, keeping paused");
            self.audio_output
                .send_command(crate::playback::cpal_output::AudioCommand::Pause);
        } else {
            trace!("Seek: Was playing, sending Play command");
            self.audio_output
                .send_command(crate::playback::cpal_output::AudioCommand::Play);
        }
        if let Some(track) = &self.current_track {
            let _ = self.progress_tx.send(PlaybackProgress::Seeked {
                position,
                track_id: track.id.clone(),
                was_paused,
            });
        }
    }
    /// Emit queue update to all subscribers
    fn emit_queue_update(&self) {
        let track_ids: Vec<String> = self.queue.iter().cloned().collect();
        let _ = self
            .progress_tx
            .send(PlaybackProgress::QueueUpdated { tracks: track_ids });
    }
    /// Load audio directly from source_path for None storage.
    ///
    /// For single-file-per-track imports, finds the audio file and reads it.
    /// For CUE/FLAC imports, reads only the track's byte range using seek + read_exact.
    async fn load_audio_from_source_path(
        &self,
        track_id: &str,
        release_id: &str,
    ) -> Result<Vec<u8>, PlaybackError> {
        info!(
            "Loading audio from source path for track {} (None storage)",
            track_id
        );
        let audio_format = self
            .library_manager
            .get_audio_format_by_track_id(track_id)
            .await
            .map_err(PlaybackError::database)?;
        let files = self
            .library_manager
            .get_files_for_release(release_id)
            .await
            .map_err(PlaybackError::database)?;
        let track = self
            .library_manager
            .get_track(track_id)
            .await
            .map_err(PlaybackError::database)?
            .ok_or_else(|| PlaybackError::not_found("Track", track_id))?;
        let audio_file = files
            .iter()
            .filter(|f| {
                let ext = f.format.to_lowercase();
                (ext == "flac" || ext == "mp3" || ext == "wav" || ext == "ogg")
                    && f.source_path.is_some()
            })
            .find(|f| {
                if let Some(track_num) = track.track_number {
                    let num_str = format!("{:02}", track_num);
                    f.original_filename.contains(&num_str)
                } else {
                    true
                }
            })
            .or_else(|| {
                files.iter().find(|f| {
                    let ext = f.format.to_lowercase();
                    (ext == "flac" || ext == "mp3" || ext == "wav" || ext == "ogg")
                        && f.source_path.is_some()
                })
            })
            .ok_or_else(|| PlaybackError::not_found("Audio file", release_id))?;
        let source_path = audio_file
            .source_path
            .as_ref()
            .ok_or_else(|| PlaybackError::not_found("Source path", &audio_file.id))?;

        // Check if this is a CUE/FLAC track that needs byte range extraction
        if let Some(audio_format) = &audio_format {
            if let (Some(start_offset), Some(end_offset)) =
                (audio_format.start_byte_offset, audio_format.end_byte_offset)
            {
                let start_byte = start_offset as u64;
                let end_byte = end_offset as u64;

                info!(
                    "CUE/FLAC range read: bytes {}-{} ({} bytes) from {}, needs_headers={}, has_headers={}",
                    start_byte,
                    end_byte,
                    end_byte - start_byte,
                    source_path,
                    audio_format.needs_headers,
                    audio_format.flac_headers.is_some()
                );

                // Read only the byte range we need using seek + read_exact
                use tokio::io::{AsyncReadExt, AsyncSeekExt};
                let mut file = tokio::fs::File::open(source_path).await?;
                file.seek(std::io::SeekFrom::Start(start_byte)).await?;

                let len = (end_byte - start_byte) as usize;
                let mut track_bytes = vec![0u8; len];
                file.read_exact(&mut track_bytes).await?;

                if audio_format.needs_headers {
                    if let Some(headers) = &audio_format.flac_headers {
                        info!(
                            "Prepending {} bytes of FLAC headers to {} bytes of track data",
                            headers.len(),
                            track_bytes.len()
                        );
                        let mut result = headers.clone();
                        result.extend_from_slice(&track_bytes);
                        return Ok(result);
                    }
                }
                return Ok(track_bytes);
            }
        }

        // Non-CUE/FLAC: read entire file
        info!("Reading full audio file from: {}", source_path);
        let file_data = tokio::fs::read(source_path).await?;
        Ok(file_data)
    }
    /// Load audio from storage.
    ///
    /// For CUE/FLAC tracks, reads only the byte range needed instead of the entire file.
    /// For encrypted files, calculates the encrypted range, fetches those chunks, and decrypts.
    async fn load_audio_from_storage(
        &self,
        track_id: &str,
        release_id: &str,
        storage_profile: &crate::db::DbStorageProfile,
    ) -> Result<Vec<u8>, PlaybackError> {
        info!(
            "Loading audio from storage for track {} (profile: {})",
            track_id, storage_profile.name
        );
        let audio_format = self
            .library_manager
            .get_audio_format_by_track_id(track_id)
            .await
            .map_err(PlaybackError::database)?;
        let files = self
            .library_manager
            .get_files_for_release(release_id)
            .await
            .map_err(PlaybackError::database)?;
        let audio_file = files
            .iter()
            .find(|f| f.format.to_lowercase() == "flac" && f.source_path.is_some())
            .ok_or_else(|| PlaybackError::not_found("FLAC file", release_id))?;
        let source_path = audio_file
            .source_path
            .as_ref()
            .ok_or_else(|| PlaybackError::not_found("Source path", &audio_file.id))?;

        // Check if this is a CUE/FLAC track that needs byte range extraction
        if let Some(audio_format) = &audio_format {
            if let (Some(start_offset), Some(end_offset)) =
                (audio_format.start_byte_offset, audio_format.end_byte_offset)
            {
                let start_byte = start_offset as u64;
                let end_byte = end_offset as u64;

                let track_bytes = if storage_profile.encrypted {
                    // Encrypted: calculate encrypted range, fetch chunks, decrypt
                    let (enc_start, enc_end) =
                        crate::encryption::encrypted_range_for_plaintext(start_byte, end_byte);

                    info!(
                        "CUE/FLAC encrypted range read: plaintext {}-{} -> encrypted {}-{} from {}",
                        start_byte, end_byte, enc_start, enc_end, source_path
                    );

                    let encrypted_data = match storage_profile.location {
                        crate::db::StorageLocation::Local => {
                            use tokio::io::{AsyncReadExt, AsyncSeekExt};
                            let mut file = tokio::fs::File::open(source_path).await?;
                            file.seek(std::io::SeekFrom::Start(enc_start)).await?;

                            let len = (enc_end - enc_start) as usize;
                            let mut buffer = vec![0u8; len];
                            file.read_exact(&mut buffer).await?;
                            buffer
                        }
                        crate::db::StorageLocation::Cloud => {
                            let storage = create_storage_reader(storage_profile)
                                .await
                                .map_err(PlaybackError::cloud)?;
                            storage
                                .download_range(source_path, enc_start, enc_end)
                                .await
                                .map_err(PlaybackError::cloud)?
                        }
                    };

                    self.encryption_service
                        .decrypt_range(&encrypted_data, start_byte, end_byte)
                        .map_err(PlaybackError::decrypt)?
                } else {
                    // Unencrypted: direct range read
                    info!(
                        "CUE/FLAC range read: bytes {}-{} ({} bytes) from {}",
                        start_byte,
                        end_byte,
                        end_byte - start_byte,
                        source_path
                    );

                    let storage = create_storage_reader(storage_profile)
                        .await
                        .map_err(PlaybackError::cloud)?;
                    storage
                        .download_range(source_path, start_byte, end_byte)
                        .await
                        .map_err(PlaybackError::cloud)?
                };

                if audio_format.needs_headers {
                    if let Some(headers) = &audio_format.flac_headers {
                        info!(
                            "Prepending {} bytes of FLAC headers to {} bytes of track data",
                            headers.len(),
                            track_bytes.len()
                        );
                        let mut result = headers.clone();
                        result.extend_from_slice(&track_bytes);
                        return Ok(result);
                    }
                }
                return Ok(track_bytes);
            }
        }

        // Non-CUE/FLAC: read entire file
        info!("Reading full file from storage: {}", source_path);
        let file_data = match storage_profile.location {
            crate::db::StorageLocation::Local => tokio::fs::read(source_path).await?,
            crate::db::StorageLocation::Cloud => {
                let storage = create_storage_reader(storage_profile)
                    .await
                    .map_err(PlaybackError::cloud)?;
                storage
                    .download(source_path)
                    .await
                    .map_err(PlaybackError::cloud)?
            }
        };

        if storage_profile.encrypted {
            self.encryption_service
                .decrypt(&file_data)
                .map_err(PlaybackError::decrypt)
        } else {
            Ok(file_data)
        }
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

/// Seektable entry for frame-accurate seeking.
#[derive(Debug, Clone, serde::Deserialize)]
struct SeekEntry {
    sample: u64,
    byte: u64,
}

/// Find frame-aligned byte offset using seektable.
///
/// Returns (byte_offset, sample_offset) where:
/// - byte_offset: frame-aligned position in the file
/// - sample_offset: samples to skip after decoding to reach exact target
pub fn find_frame_boundary_for_seek(
    seek_time: std::time::Duration,
    sample_rate: u32,
    seektable_json: &str,
) -> Option<(u64, u64)> {
    let entries: Vec<SeekEntry> = serde_json::from_str(seektable_json).ok()?;
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

/// Calculate starting position for track playback.
///
/// With frame-accurate byte positions from import, the track audio starts at
/// the correct position (<93ms precision, imperceptible). Only pregap handling
/// is needed here:
/// - Direct selection (play, next, previous): skip pregap, start at INDEX 01
/// - Natural transition (auto-advance): play pregap, start at INDEX 00
pub fn calculate_start_position(
    pregap_ms: Option<i64>,
    is_natural_transition: bool,
) -> std::time::Duration {
    if is_natural_transition {
        // Natural transition: start at INDEX 00 (position 0), play the pregap
        std::time::Duration::ZERO
    } else {
        // Direct selection: skip to INDEX 01 (skip the pregap)
        std::time::Duration::from_millis(pregap_ms.unwrap_or(0).max(0) as u64)
    }
}

/// Validate a seek position against the decoded audio duration.
///
/// IMPORTANT: This must use `decoded_duration` (actual PCM length including pregap),
/// NOT `track_duration` (metadata duration excluding pregap).
///
/// The UI sends seek positions that include pregap offset, so validation must
/// compare against the full decoded audio length.
pub fn validate_seek_position(
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
pub enum SeekValidationError {
    PastEnd {
        requested: std::time::Duration,
        max_seekable: std::time::Duration,
    },
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_direct_selection_skips_pregap() {
        // When directly selecting a track with 3-second pregap,
        // playback should start at 3000ms (skipping the pregap)
        let pregap_ms = Some(3000i64);
        let is_natural_transition = false;

        let start_pos = calculate_start_position(pregap_ms, is_natural_transition);

        assert_eq!(
            start_pos,
            std::time::Duration::from_millis(3000),
            "Direct selection should skip pregap and start at INDEX 01"
        );
    }

    #[test]
    fn test_natural_transition_plays_pregap() {
        // When naturally transitioning to a track with 3-second pregap,
        // playback should start at 0ms (playing the pregap, showing negative time)
        let pregap_ms = Some(3000i64);
        let is_natural_transition = true;

        let start_pos = calculate_start_position(pregap_ms, is_natural_transition);

        assert_eq!(
            start_pos,
            std::time::Duration::ZERO,
            "Natural transition should start at INDEX 00 to play pregap"
        );
    }

    #[test]
    fn test_direct_selection_no_pregap() {
        // When directly selecting a track without pregap,
        // playback should start at 0ms
        let pregap_ms = None;
        let is_natural_transition = false;

        let start_pos = calculate_start_position(pregap_ms, is_natural_transition);

        assert_eq!(
            start_pos,
            std::time::Duration::ZERO,
            "Direct selection without pregap should start at 0"
        );
    }

    #[test]
    fn test_natural_transition_no_pregap() {
        // When naturally transitioning to a track without pregap,
        // playback should start at 0ms (same as direct selection)
        let pregap_ms = None;
        let is_natural_transition = true;

        let start_pos = calculate_start_position(pregap_ms, is_natural_transition);

        assert_eq!(
            start_pos,
            std::time::Duration::ZERO,
            "Natural transition without pregap should start at 0"
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

    // Smart seek tests for SparseStreamingBuffer integration
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
    fn test_smart_seek_within_buffer() {
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
    fn test_smart_seek_past_buffer() {
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
    fn test_smart_seek_multiple_ranges() {
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
