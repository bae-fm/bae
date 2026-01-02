use crate::db::DbTrack;
use crate::encryption::EncryptionService;
use crate::library::LibraryManager;
use crate::playback::cpal_output::AudioOutput;
use crate::playback::progress::{PlaybackProgress, PlaybackProgressHandle};
use crate::playback::{PcmSource, PlaybackError};
use crate::storage::create_storage_reader;
use cpal::traits::StreamTrait;
use std::collections::VecDeque;
use std::sync::{mpsc, Arc};
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
    current_duration: Option<std::time::Duration>,
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
    next_duration: Option<std::time::Duration>,
    next_pregap_ms: Option<i64>,
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
                    info!("Seek command received: {:?}", position);
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

        // Load audio format to get pregap_ms for CUE/FLAC tracks
        let pregap_ms = match self
            .library_manager
            .get_audio_format_by_track_id(track_id)
            .await
        {
            Ok(Some(audio_format)) => audio_format.pregap_ms,
            Ok(None) => None,
            Err(e) => {
                error!("Failed to get audio format: {}", e);
                None
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

        let pcm_source = match &storage_profile {
            None => {
                match self
                    .load_audio_from_source_path(track_id, &track.release_id)
                    .await
                {
                    Ok(data) => match Self::decode_flac_bytes(&data).await {
                        Ok(source) => source,
                        Err(e) => {
                            error!("Failed to decode FLAC: {}", e);
                            self.stop().await;
                            return;
                        }
                    },
                    Err(e) => {
                        error!("Failed to load audio from source path: {}", e);
                        self.stop().await;
                        return;
                    }
                }
            }
            Some(profile) => {
                match self
                    .load_audio_from_storage(track_id, &track.release_id, profile)
                    .await
                {
                    Ok(data) => match Self::decode_flac_bytes(&data).await {
                        Ok(source) => source,
                        Err(e) => {
                            error!("Failed to decode FLAC: {}", e);
                            self.stop().await;
                            return;
                        }
                    },
                    Err(e) => {
                        error!("Failed to load audio from storage: {}", e);
                        self.stop().await;
                        return;
                    }
                }
            }
        };
        info!(
            "Track decoded: {} samples, {}Hz",
            pcm_source.duration().as_millis(),
            pcm_source.sample_rate()
        );
        let track_duration = track
            .duration_ms
            .map(|ms| std::time::Duration::from_millis(ms as u64))
            .unwrap_or_else(|| panic!("Cannot play track {} without duration", track_id));
        info!("Track duration: {:?}", track_duration);
        self.play_track_with_source(
            track_id,
            track,
            pcm_source,
            track_duration,
            pregap_ms,
            is_natural_transition,
        )
        .await;
    }
    /// Decode raw FLAC bytes to PCM source
    async fn decode_flac_bytes(flac_data: &[u8]) -> Result<Arc<PcmSource>, PlaybackError> {
        if flac_data.len() < 4 || &flac_data[0..4] != b"fLaC" {
            return Err(PlaybackError::flac("Invalid FLAC header"));
        }
        let flac_data = flac_data.to_vec();
        let decoded = tokio::task::spawn_blocking(move || {
            crate::flac_decoder::decode_flac_range(&flac_data, None, None)
        })
        .await
        .map_err(PlaybackError::task)?
        .map_err(PlaybackError::flac)?;
        Ok(Arc::new(PcmSource::new(
            decoded.samples,
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
        let track_duration_for_completion = track_duration;
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

        // Load audio format to get pregap_ms for CUE/FLAC tracks
        let pregap_ms = match self
            .library_manager
            .get_audio_format_by_track_id(track_id)
            .await
        {
            Ok(Some(audio_format)) => audio_format.pregap_ms,
            Ok(None) => None,
            Err(e) => {
                error!("Failed to get audio format for preload: {}", e);
                None
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
                    Ok(data) => match Self::decode_flac_bytes(&data).await {
                        Ok(source) => source,
                        Err(e) => {
                            error!("Failed to decode FLAC for preload: {}", e);
                            return;
                        }
                    },
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
                    Ok(data) => match Self::decode_flac_bytes(&data).await {
                        Ok(source) => source,
                        Err(e) => {
                            error!("Failed to decode FLAC for preload: {}", e);
                            return;
                        }
                    },
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
        let track_duration = self
            .current_duration
            .expect("Cannot seek: track has no duration");
        if position > track_duration {
            error!(
                "Cannot seek past end of track: requested {}, track duration {}",
                position.as_secs_f64(),
                track_duration.as_secs_f64()
            );
            let _ = self.progress_tx.send(PlaybackProgress::SeekError {
                requested_position: position,
                track_duration,
            });
            return;
        }
        pcm_source.seek(position);
        trace!("Seek: PCM source seeked successfully, creating new channels");
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
                        trace!("Seek: Bridge received position update: {:?}", pos);
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
        let track_duration_for_completion = track_duration;
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
                    trace!("Seek: Listener received position update: {:?}", pos); *
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
            trace!("Seek: Sending Seeked event with position: {:?}", position);
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
    /// For CUE/FLAC imports, extracts the track's byte range and prepends FLAC headers.
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
        info!("Reading audio from: {}", source_path);
        let file_data = tokio::fs::read(source_path).await?;

        // Check if this is a CUE/FLAC track that needs byte range extraction
        if let Some(audio_format) = audio_format {
            if let (Some(start_offset), Some(end_offset)) =
                (audio_format.start_byte_offset, audio_format.end_byte_offset)
            {
                let start_byte = start_offset as usize;
                let end_byte = end_offset as usize;
                info!(
                    "CUE/FLAC: Extracting track bytes {}-{} from {} byte file",
                    start_byte,
                    end_byte,
                    file_data.len()
                );
                if end_byte > file_data.len() {
                    return Err(PlaybackError::flac(format!(
                        "Track byte range {}-{} exceeds file size {}",
                        start_byte,
                        end_byte,
                        file_data.len(),
                    )));
                }
                let track_bytes = file_data[start_byte..end_byte].to_vec();
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
        Ok(file_data)
    }
    /// Load audio from storage.
    ///
    /// Files are stored whole. For CUE/FLAC tracks, we use byte ranges to
    /// extract the track's portion.
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
        info!("Reading from storage path: {}", source_path);
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
        let file_data = if storage_profile.encrypted {
            if file_data.len() < 12 {
                return Err(PlaybackError::decrypt("File too small to contain nonce"));
            }
            let nonce = &file_data[..12];
            let ciphertext = &file_data[12..];
            self.encryption_service
                .decrypt(ciphertext, nonce)
                .map_err(PlaybackError::decrypt)?
        } else {
            file_data
        };
        // Check if this is a CUE/FLAC track that needs byte range extraction
        if let Some(audio_format) = audio_format {
            if let (Some(start_offset), Some(end_offset)) =
                (audio_format.start_byte_offset, audio_format.end_byte_offset)
            {
                let start_byte = start_offset as usize;
                let end_byte = end_offset as usize;
                info!(
                    "Extracting track bytes {}-{} from {} byte file",
                    start_byte,
                    end_byte,
                    file_data.len()
                );
                if end_byte > file_data.len() {
                    return Err(PlaybackError::flac(format!(
                        "Track byte range {}-{} exceeds file size {}",
                        start_byte,
                        end_byte,
                        file_data.len(),
                    )));
                }
                let track_bytes = file_data[start_byte..end_byte].to_vec();
                if audio_format.needs_headers {
                    if let Some(headers) = &audio_format.flac_headers {
                        let mut result = headers.clone();
                        result.extend_from_slice(&track_bytes);
                        return Ok(result);
                    }
                }
                return Ok(track_bytes);
            }
        }
        Ok(file_data)
    }
}

/// Calculate starting position for track playback.
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
}
