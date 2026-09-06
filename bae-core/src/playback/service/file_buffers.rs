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
}

impl FileBuffers {
    pub(super) fn new() -> Self {
        Self {
            cached: HashMap::new(),
            retired: Vec::new(),
            arbiter: FetchArbiter::new(),
        }
    }

    /// Designate this track's buffer as the foreground for fetch priority, so
    /// its reader fetches immediately and a next-track preload's reader yields
    /// to it. Called wherever a track becomes the current one.
    pub(super) fn mark_foreground(&self, prepared: &PlaybackPreparedTrack) {
        if let Some(segment) = prepared.segments.first() {
            self.arbiter.set_foreground(segment.buffer.id());
        }
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
        self.retired
            .iter()
            .map(|prepared| prepared.track_info.track_id.as_str())
            .collect()
    }

    /// Seed the cache the way a prepare would, for tests that drive release and
    /// eviction without a resolvable track behind the file.
    pub(super) fn cache_for_test(&mut self, file_id: &str, buffer: SharedSparseBuffer) {
        self.cached.insert(file_id.to_string(), buffer);
    }
}

/// Resolve `track_id` and back each of its segments with the buffer its bytes
/// stream from: the cached one when that file is already being fetched,
/// otherwise a fresh buffer whose fill task starts here.
///
/// Not a `FileBuffers` method: the prepared track it builds carries those
/// buffers onward into the slot, and an owner hands its retained state to no
/// caller. Kept in this module so the only reach into the cache stays here.
pub(super) async fn prepare_track_for_playback(
    library_manager: &LibraryManager,
    track_id: &str,
    file_buffers: &mut FileBuffers,
    command_tx: &tokio_mpsc::UnboundedSender<PlaybackCommand>,
) -> Result<PlaybackPreparedTrack, PlaybackError> {
    let (resolved, track_info) = library_manager
        .resolve_track_audio_and_info(track_id)
        .await
        .map_err(PlaybackError::database)?;
    ensure_resolved_audio_format(track_id, &resolved)?;

    let is_ape = resolved.content_type == crate::util::content_type::ContentType::Ape;
    let mut prepared_segments = Vec::with_capacity(resolved.segments.len());
    for segment in &resolved.segments {
        // A cached buffer is live by construction: buffers are cancelled only
        // when they leave the cache (`release` / `cancel_all`), so its fill task
        // is still serving demand.
        let buffer = match file_buffers.cached.get(&segment.file_id) {
            Some(cached) => {
                info!("Reusing cached file buffer");
                cached.clone()
            }
            None => {
                let buffer = create_sparse_buffer(segment.file_size);
                let reader = create_audio_reader(
                    library_manager,
                    &segment.file_id,
                    file_buffers.arbiter.clone(),
                    segment.span.start_byte,
                    is_ape,
                );
                reader.start_reading(
                    buffer.clone(),
                    playback_fill_error_handler(command_tx.clone(), buffer.id()),
                );
                file_buffers
                    .cached
                    .insert(segment.file_id.clone(), buffer.clone());
                buffer
            }
        };
        prepared_segments.push(PreparedAudioSegment {
            role: segment.role.clone(),
            file_id: segment.file_id.clone(),
            buffer,
            span: segment.span,
        });
    }

    // Read the replay-gain mode once, here, and pass it down — rather than a
    // config lookup buried inside `finalize_playback_track`.
    let replay_gain_mode = library_manager.get_config().replay_gain_mode;

    Ok(finalize_playback_track(
        resolved,
        track_info,
        prepared_segments,
        replay_gain_mode,
    ))
}

/// The playback shape of a fill-error handler: a failed byte fill reports itself
/// to the command loop, naming the buffer it failed on. The loop is the only
/// place that knows whether that buffer feeds the current track or a preloaded
/// next, which is what decides whether the failure halts playback. (The fill
/// itself cancels the buffer right after, unblocking the decoder.)
fn playback_fill_error_handler(
    command_tx: tokio_mpsc::UnboundedSender<PlaybackCommand>,
    buffer_id: u64,
) -> crate::playback::data_source::FillErrorHandler {
    Box::new(move |error| {
        dispatch_command(
            &command_tx,
            PlaybackCommand::ReadFailed { buffer_id, error },
        );
    })
}
