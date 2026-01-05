use crate::db::DbTrack;
use crate::encryption::EncryptionService;
use crate::library::LibraryManager;
use crate::playback::cpal_output::AudioOutput;
use crate::playback::progress::{PlaybackProgress, PlaybackProgressHandle};
use crate::playback::{
    create_streaming_buffer, create_streaming_pair, PcmSource, PlaybackError, StreamingPcmSource,
};
use crate::storage::{
    create_storage_reader, download_encrypted_to_streaming_buffer, download_to_streaming_buffer,
    download_to_streaming_buffer_with_range,
};
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
                    audio_output,
                    stream: None,
                    current_pcm_source: None,
                    next_pcm_source: None,
                    next_track_id: None,
                    next_duration: None,
                    next_pregap_ms: None,
                    current_streaming_source: None,
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

        match &storage_profile {
            None => {
                self.play_track_streaming_local(track_id, track, is_natural_transition)
                    .await;
            }
            Some(profile) => {
                self.play_track_streaming(track_id, track, profile, is_natural_transition)
                    .await;
            }
        }
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

        // Clear streaming source if active (cancellation happens via buffer)
        self.current_streaming_source.take();

        self.current_track = None;
        self.current_pcm_source = None;
        self.current_position = None;
        self.current_duration = None;
        self.next_pcm_source = None;
        self.next_track_id = None;
        self.next_duration = None;
        self.audio_output
            .send_command(crate::playback::cpal_output::AudioCommand::Stop);
        let _ = self.progress_tx.send(PlaybackProgress::StateChanged {
            state: PlaybackState::Stopped,
        });
    }
    async fn seek(&mut self, position: std::time::Duration) {
        // Check if we're using streaming playback
        if self.current_streaming_source.is_some() {
            // For streaming playback, seeking requires restarting the entire pipeline.
            // For now, we don't support seeking in streaming mode - just log and return.
            info!(
                "Seek not fully supported for streaming playback. Requested position: {:?}",
                position
            );

            // Update the displayed position even though we can't actually seek
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

    /// Play a track using streaming playback from local source_path.
    ///
    /// Used when no storage profile is set (files played directly from disk).
    async fn play_track_streaming_local(
        &mut self,
        track_id: &str,
        track: DbTrack,
        is_natural_transition: bool,
    ) {
        info!(
            "Playing track (streaming local): {} (natural_transition: {})",
            track_id, is_natural_transition
        );

        let _ = self.progress_tx.send(PlaybackProgress::StateChanged {
            state: PlaybackState::Loading {
                track_id: track_id.to_string(),
            },
        });

        // Get audio format for CUE/FLAC metadata
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

        let pregap_ms = audio_format.as_ref().and_then(|af| af.pregap_ms);

        // Find the audio file with source_path
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

        // Determine byte range for CUE/FLAC if applicable
        let (start_byte, end_byte) = if let Some(af) = &audio_format {
            if let (Some(start), Some(end)) = (af.start_byte_offset, af.end_byte_offset) {
                (Some(start as u64), Some(end as u64))
            } else {
                (None, None)
            }
        } else {
            (None, None)
        };

        // Check if we need to prepend FLAC headers
        let flac_headers = audio_format
            .as_ref()
            .filter(|af| af.needs_headers)
            .and_then(|af| af.flac_headers.clone());

        // Create streaming infrastructure
        let buffer = create_streaming_buffer();

        // Spawn local file reader task
        let read_buffer = buffer.clone();
        let read_path = source_path.clone();
        tokio::spawn(async move {
            use tokio::io::{AsyncReadExt, AsyncSeekExt};

            let mut file = match tokio::fs::File::open(&read_path).await {
                Ok(f) => f,
                Err(e) => {
                    error!("Failed to open file {}: {}", read_path, e);
                    read_buffer.cancel();
                    return;
                }
            };

            // If we have FLAC headers to prepend, add them first
            if let Some(headers) = flac_headers {
                read_buffer.append(&headers);
            }

            // Seek to start position if needed
            let start = start_byte.unwrap_or(0);
            if start > 0 {
                if let Err(e) = file.seek(std::io::SeekFrom::Start(start)).await {
                    error!("Failed to seek: {}", e);
                    read_buffer.cancel();
                    return;
                }
            }

            let end = end_byte;
            let mut pos = start;
            let mut chunk = vec![0u8; 65536];

            loop {
                if read_buffer.is_cancelled() {
                    return;
                }

                let to_read = if let Some(end) = end {
                    chunk.len().min((end - pos) as usize)
                } else {
                    chunk.len()
                };

                if to_read == 0 {
                    break;
                }

                match file.read(&mut chunk[..to_read]).await {
                    Ok(0) => break, // EOF
                    Ok(n) => {
                        read_buffer.append(&chunk[..n]);
                        pos += n as u64;
                    }
                    Err(e) => {
                        error!("Read error: {}", e);
                        break;
                    }
                }
            }

            read_buffer.mark_eof();
        });

        // Create ring buffer pair
        let (mut sink, source) = create_streaming_pair(44100, 2);

        // Spawn decoder thread
        let decoder_buffer = buffer.clone();
        std::thread::spawn(move || {
            if let Err(e) = crate::audio_codec::decode_audio_streaming(decoder_buffer, &mut sink) {
                error!("Streaming decode failed: {}", e);
            }
        });

        // Wait for initial buffering
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;

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

        let progress_tx = self.progress_tx.clone();
        let track_id_owned = track_id.to_string();
        let current_position_for_listener = self.current_position_shared.clone();

        tokio::spawn(async move {
            loop {
                tokio::select! {
                    Some(position) = position_rx_async.recv() => {
                        *current_position_for_listener.lock().unwrap() = Some(position);
                        let _ = progress_tx.send(PlaybackProgress::PositionUpdate {
                            position,
                            track_id: track_id_owned.clone(),
                        });
                    }
                    Some(()) = completion_rx_async.recv() => {
                        info!("Streaming track completed: {}", track_id_owned);
                        let _ = progress_tx.send(PlaybackProgress::TrackCompleted {
                            track_id: track_id_owned.clone(),
                        });
                        break;
                    }
                    else => break,
                }
            }
        });

        if let Some(next_track_id) = self.queue.front().cloned() {
            self.preload_next_track(&next_track_id).await;
        }

        info!("Streaming local playback started for track: {}", track_id);
    }

    /// Play a track using streaming playback with a storage profile.
    ///
    /// This method streams audio from storage, enabling playback to start
    /// before the full file is downloaded. Uses a pipeline:
    /// 1. Download task fills StreamingAudioBuffer
    /// 2. Decoder thread reads from buffer, outputs to ring buffer
    /// 3. cpal callback pulls from ring buffer
    async fn play_track_streaming(
        &mut self,
        track_id: &str,
        track: DbTrack,
        storage_profile: &crate::db::DbStorageProfile,
        is_natural_transition: bool,
    ) {
        info!(
            "Playing track (streaming): {} (natural_transition: {})",
            track_id, is_natural_transition
        );

        let _ = self.progress_tx.send(PlaybackProgress::StateChanged {
            state: PlaybackState::Loading {
                track_id: track_id.to_string(),
            },
        });

        // Get audio format for CUE/FLAC metadata
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

        let pregap_ms = audio_format.as_ref().and_then(|af| af.pregap_ms);

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

        let audio_file = match files
            .iter()
            .find(|f| f.format.to_lowercase() == "flac" && f.source_path.is_some())
        {
            Some(f) => f,
            None => {
                error!("No FLAC file found for release {}", track.release_id);
                self.stop().await;
                return;
            }
        };

        let source_path = match &audio_file.source_path {
            Some(p) => p.clone(),
            None => {
                error!("No source path for audio file");
                self.stop().await;
                return;
            }
        };

        // Create streaming infrastructure
        let buffer = create_streaming_buffer();

        // Determine byte range for CUE/FLAC if applicable
        let (start_byte, end_byte) = if let Some(af) = &audio_format {
            if let (Some(start), Some(end)) = (af.start_byte_offset, af.end_byte_offset) {
                (Some(start as u64), Some(end as u64))
            } else {
                (None, None)
            }
        } else {
            (None, None)
        };

        // Spawn download task
        let storage = match create_storage_reader(storage_profile).await {
            Ok(s) => s,
            Err(e) => {
                error!("Failed to create storage reader: {}", e);
                self.stop().await;
                return;
            }
        };

        let download_buffer = buffer.clone();
        let download_path = source_path.clone();
        let is_encrypted = storage_profile.encrypted;
        let encryption_service = self.encryption_service.clone();

        tokio::spawn(async move {
            let result = if is_encrypted {
                download_encrypted_to_streaming_buffer(
                    storage,
                    &download_path,
                    download_buffer,
                    &encryption_service,
                    start_byte.unwrap_or(0),
                    end_byte,
                )
                .await
            } else if let (Some(start), Some(end)) = (start_byte, end_byte) {
                download_to_streaming_buffer_with_range(
                    storage,
                    &download_path,
                    download_buffer,
                    start,
                    Some(end),
                )
                .await
            } else {
                download_to_streaming_buffer(storage, &download_path, download_buffer, None).await
            };

            if let Err(e) = result {
                error!("Streaming download failed: {:?}", e);
            }
        });

        // Create ring buffer pair
        // Use default sample rate/channels, will be updated when decoder reports actual values
        let (mut sink, source) = create_streaming_pair(44100, 2);

        // Spawn decoder thread
        let decoder_buffer = buffer.clone();
        std::thread::spawn(move || {
            if let Err(e) = crate::audio_codec::decode_audio_streaming(decoder_buffer, &mut sink) {
                error!("Streaming decode failed: {}", e);
            }
        });

        // Wait a bit for initial buffering before creating stream
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;

        let source = Arc::new(Mutex::new(source));

        // Get actual audio info from the source
        let (source_sample_rate, source_channels) = {
            let guard = source.lock().unwrap();
            (guard.sample_rate(), guard.channels())
        };

        // Calculate start position
        let start_position = calculate_start_position(pregap_ms, is_natural_transition);

        // Clean up old stream
        if let Some(stream) = self.stream.take() {
            drop(stream);
        }

        // Create channels for position/completion
        let (position_tx, position_rx) = mpsc::channel();
        let (completion_tx, completion_rx) = mpsc::channel();
        let (position_tx_async, mut position_rx_async) = tokio_mpsc::unbounded_channel();
        let (completion_tx_async, mut completion_rx_async) = tokio_mpsc::unbounded_channel();

        // Bridge sync channels to async
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

        // Create streaming audio stream
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

        // Get duration from track metadata
        let track_duration = track
            .duration_ms
            .map(|ms| std::time::Duration::from_millis(ms as u64))
            .unwrap_or(std::time::Duration::from_secs(300)); // Default 5 min if unknown

        self.current_duration = Some(track_duration);
        // For streaming, we don't know decoded duration until complete
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

        // Spawn completion listener
        let progress_tx = self.progress_tx.clone();
        let track_id_owned = track_id.to_string();
        let current_position_for_listener = self.current_position_shared.clone();

        tokio::spawn(async move {
            loop {
                tokio::select! {
                    Some(position) = position_rx_async.recv() => {
                        *current_position_for_listener.lock().unwrap() = Some(position);
                        let _ = progress_tx.send(PlaybackProgress::PositionUpdate {
                            position,
                            track_id: track_id_owned.clone(),
                        });
                    }
                    Some(()) = completion_rx_async.recv() => {
                        info!("Streaming track completed: {}", track_id_owned);
                        let _ = progress_tx.send(PlaybackProgress::TrackCompleted {
                            track_id: track_id_owned.clone(),
                        });
                        break;
                    }
                    else => break,
                }
            }
        });

        // Preload next track if available
        if let Some(next_track_id) = self.queue.front().cloned() {
            self.preload_next_track(&next_track_id).await;
        }

        info!("Streaming playback started for track: {}", track_id);
    }
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
}
