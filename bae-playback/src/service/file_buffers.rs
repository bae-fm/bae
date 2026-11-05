//! The byte buffers a track's audio is fetched into.
//!
//! One sparse buffer per release file, shared by every track that plays from
//! that file. [`FileBuffers`] is those buffers' single owner: it creates them as
//! tracks are prepared, hands the playing track's reader fetch priority over a
//! preload's, and cancels a buffer — stopping its on-demand fill task for good —
//! exactly when it leaves the cache. So a cached buffer is always live, and
//! preparing a track over one reuses it as-is.
//!
//! Release is deferred, which is why the retired tracks live here too: a track
//! leaving the live slot is retired with its buffers still alive, and they are
//! released only once its successor is prepared and reveals which files the two
//! share (a CUE album's tracks all play from one file, whose buffer and fill
//! task must survive the switch).

use super::*;
use bae_track_audio::data_source::AudioDataReader;

pub(super) struct FileBuffers {
    /// Full-file buffers keyed by release file id.
    cached: HashMap<String, SharedSparseBuffer>,
    /// Tracks removed from the live slot whose buffers remain available until
    /// the successor is prepared and reveals which files it reuses.
    retired: Vec<PlaybackPreparedTrack>,
    /// Prioritizes byte fetches across tracks: the current track's reader
    /// fetches immediately, a next-track preload's reader yields to it. Shared
    /// into every reader started below; the current track is designated
    /// foreground via `mark_foreground` whenever it becomes current.
    arbiter: Arc<FetchArbiter>,
    /// The current track's bytes and the buffer holding them; `None` while
    /// no track is current, or the current one plays on another device.
    playing: tokio::sync::watch::Sender<Option<PlayingBytes>>,
}

impl FileBuffers {
    /// Buffers whose readers `arbiter` prioritizes, telling `playing` the
    /// current track's bytes.
    pub(super) fn new(
        arbiter: Arc<FetchArbiter>,
        playing: tokio::sync::watch::Sender<Option<PlayingBytes>>,
    ) -> Self {
        Self { cached: HashMap::new(), retired: Vec::new(), arbiter, playing }
    }

    /// Designate this track's buffers — one per file it reads — as the
    /// foreground for fetch priority, so its readers fetch immediately and a
    /// next-track preload's reader yields to them, and follow the bytes it
    /// plays from INDEX 01. Called wherever a track becomes the current one.
    pub(super) fn mark_foreground(&self, prepared: &PlaybackPreparedTrack) {
        let buffer_ids = prepared.segments.iter().map(|segment| segment.buffer.id()).collect();
        self.arbiter.set_foreground(buffer_ids, &prepared.track_id);
        self.playing.send_replace(prepared.playing_bytes());
    }

    /// No track is current any more.
    pub(super) fn clear_playing(&self) {
        self.playing.send_replace(None);
    }

    /// The track a load the person waits on is for, or `None` once nothing
    /// loads: the playing track's reads from storage while it loads are the
    /// download its load waits on.
    pub(super) fn set_waiting(&self, track_id: Option<&str>) {
        self.arbiter.set_waiting(track_id);
    }

    /// Hold a track that has left the live slot. Its buffers stay cached and
    /// alive until `release_retired` learns which files the successor reuses.
    pub(super) fn retire(&mut self, prepared: PlaybackPreparedTrack) {
        self.retired.push(prepared);
    }

    /// Release every retired track's buffers, keeping the files named by
    /// `retained_file_ids` — the successor's — cached and alive.
    pub(super) fn release_retired(&mut self, retained_file_ids: &HashSet<&str>) {
        for prepared in std::mem::take(&mut self.retired) {
            self.release(&prepared, retained_file_ids);
        }
    }

    /// Release one track's file buffers as it leaves the pipeline. A file the
    /// retained track(s) still play stays cached and alive — its readers are
    /// woken so this track's cancelled decoder observes its token instead of
    /// staying parked on a read. The rest leave the cache and are cancelled,
    /// which stops their fill task and unblocks anything reading them.
    pub(super) fn release(
        &mut self,
        prepared: &PlaybackPreparedTrack,
        retained_file_ids: &HashSet<&str>,
    ) {
        for segment in &prepared.segments {
            if retained_file_ids.contains(segment.file_id.as_str()) {
                segment.buffer.wake_readers();
            } else {
                segment.buffer.cancel();
                self.cached.remove(&segment.file_id);
            }
        }
    }

    /// Cancel every cached buffer and empty the cache — the whole-pipeline
    /// teardown, where no track is left to keep a file alive for.
    pub(super) fn cancel_all(&mut self) {
        for buffer in self.cached.values() {
            buffer.cancel();
        }
        self.cached.clear();
    }
}

/// The window the tests observe and seed this state through, since nothing in
/// playback reads the cache or the retired list from outside.
#[cfg(test)]
impl FileBuffers {
    /// Whether this release file's buffer is cached, and so still alive.
    pub(super) fn holds(&self, file_id: &str) -> bool {
        self.cached.contains_key(file_id)
    }

    pub(super) fn retired_track_ids(&self) -> Vec<&str> {
        self.retired.iter().map(|prepared| prepared.track_id.as_str()).collect()
    }

    /// Seed the cache the way a prepare would, for tests that drive release and
    /// eviction without a resolvable track behind the file.
    pub(super) fn cache_for_test(&mut self, file_id: &str, buffer: SharedSparseBuffer) {
        self.cached.insert(file_id.to_string(), buffer);
    }
}

/// Resolve `track_id`, back each of its segments with the buffer its bytes
/// stream from, and give its decode from `start`: the cached buffer when that
/// file is already being fetched, otherwise a fresh one whose fill task starts
/// here, fetching first the window that decode reads first.
///
/// Not a `FileBuffers` method: the prepared track it builds carries those
/// buffers onward into the slot, and an owner hands its retained state to no
/// caller. Kept in this module so the only reach into the cache stays here.
pub(super) async fn prepare_track_for_playback(
    store: &bae_store::Store,
    database: &bae_db::Database,
    diagnostics: &Diagnostics,
    track_id: &str,
    start: TrackStart,
    file_buffers: &mut FileBuffers,
    command_tx: &tokio_mpsc::UnboundedSender<PlaybackCommand>,
) -> Result<(PlaybackPreparedTrack, StreamDecodeParams), PlaybackError> {
    let resolved = ResolvedTrackAudio::resolve(database, track_id).await?;
    // Read the replay-gain mode once, here, and pass it down — rather than a
    // preferences read buried inside `finalize_playback_track`. Read before
    // any buffer is made, so nothing fails between caching a buffer and
    // starting its fill.
    let replay_gain_mode = bae_preferences::preferences(store).await?.replay_gain_mode;

    let mut prepared_segments = Vec::with_capacity(resolved.segments.len());
    // The fresh buffers, each with the file its fill reads.
    let mut fresh = Vec::new();
    for segment in &resolved.segments {
        // A cached buffer is live by construction: buffers are cancelled only
        // when they leave the cache (`release` / `cancel_all`), and a fresh one
        // has its fill started below before this returns, so its fill task is
        // still serving demand.
        let buffer = match file_buffers.cached.get(&segment.file_id) {
            Some(cached) => {
                info!("Reusing cached file buffer");
                cached.clone()
            }
            None => {
                let buffer = create_sparse_buffer(segment.file_size);
                file_buffers.cached.insert(segment.file_id.clone(), buffer.clone());
                fresh.push((segment.file_id.clone(), buffer.clone()));
                buffer
            }
        };
        prepared_segments.push(PreparedAudioSegment::of(segment, buffer));
    }

    let prepared = finalize_playback_track(
        track_id.to_string(),
        &resolved,
        prepared_segments,
        replay_gain_mode,
    );
    let decode = prepared.decode_from(start);
    let is_ape = resolved.content_type == bae_util::content_type::ContentType::Ape;
    for (file_id, buffer) in fresh {
        let reader = Box::new(bae_track_audio::data_source::CovenFileReader::for_playback(
            database.clone(),
            file_id,
            file_buffers.arbiter.clone(),
            decode.first_read_byte(buffer.id()),
            is_ape,
            diagnostics.clone(),
        ));
        reader.start_reading(
            buffer.clone(),
            playback_fill_error_handler(command_tx.clone(), buffer.id()),
        );
    }
    Ok((prepared, decode))
}

/// The playback shape of a fill-error handler: a failed byte fill reports itself
/// to the command loop, naming the buffer it failed on. The loop is the only
/// place that knows whether that buffer feeds the current track or a preloaded
/// next, which is what decides whether the failure halts playback. (The fill
/// has already failed the buffer with the same error, unblocking the decoder.)
fn playback_fill_error_handler(
    command_tx: tokio_mpsc::UnboundedSender<PlaybackCommand>,
    buffer_id: u64,
) -> bae_track_audio::data_source::FillErrorHandler {
    Box::new(move |error| {
        dispatch_command(&command_tx, PlaybackCommand::ReadFailed { buffer_id, error });
    })
}

#[cfg(test)]
#[path = "file_buffers_tests.rs"]
mod tests;
