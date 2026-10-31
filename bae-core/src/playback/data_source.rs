//! Unified data source abstraction for audio playback.
//!
//! Provides a common interface for reading audio bytes from:
//! - Local files (non-storage releases, or storage releases with local backend)
//! - Cloud storage (storage releases with cloud backend)

use crate::playback::PlaybackError;
use bae_audio::sparse_buffer::{SharedSparseBuffer, SourceReadError};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Instant;
use tokio::sync::Notify;
use tracing::{debug, error, info};

/// How much a fill fetches at a time: 64 of coven's chunks, 4 MiB. The buffer
/// keeps one window ahead of a reader that hasn't set its ceiling and one
/// behind the most-behind reader, and import spaces a FLAC's scrub points so a
/// seek to one reads from a single window (an MP3's 10 seconds fit in one at
/// any bitrate).
pub(crate) const FILL_WINDOW_SIZE: u64 = coven::CHUNK_SIZE as u64 * 64;

/// Prioritizes audio byte fetches so the track the user is waiting to hear gets
/// the bandwidth. One foreground track is designated at a time -- the playing
/// track -- and its fetches run immediately. A next-track preload's fetches wait
/// while the foreground track has a fetch in flight, so preloading the next
/// track can't take bandwidth from the current track's start. One arbiter is
/// shared by every reader through the playback service; the service designates
/// the foreground via [`FetchArbiter::set_foreground`] whenever a track becomes
/// current.
pub struct FetchArbiter {
    /// The [`bae_audio::sparse_buffer::SparseStreamingBuffer::id`] of the
    /// foreground (playing) track, or `u64::MAX` for none.
    foreground_buffer: AtomicU64,
    /// Count of foreground fetches currently awaiting bytes. A preload fetch
    /// waits while this is non-zero.
    foreground_inflight: AtomicU64,
    /// Wakes waiting preload fetches when the foreground goes idle or changes.
    idle: Notify,
}

impl FetchArbiter {
    pub fn new() -> Arc<Self> {
        Arc::new(Self {
            foreground_buffer: AtomicU64::new(u64::MAX),
            foreground_inflight: AtomicU64::new(0),
            idle: Notify::new(),
        })
    }

    /// Designate `buffer_id` as the foreground (playing) track. Wakes any
    /// waiting preload in case the previous foreground is gone.
    pub fn set_foreground(&self, buffer_id: u64) {
        self.foreground_buffer.store(buffer_id, Ordering::Release);
        self.idle.notify_waiters();
    }

    fn is_foreground(&self, buffer_id: u64) -> bool {
        self.foreground_buffer.load(Ordering::Acquire) == buffer_id
    }

    fn begin_foreground_fetch(&self) {
        self.foreground_inflight.fetch_add(1, Ordering::AcqRel);
    }

    fn end_foreground_fetch(&self) {
        if self.foreground_inflight.fetch_sub(1, Ordering::AcqRel) == 1 {
            self.idle.notify_waiters();
        }
    }

    /// Wait until no foreground fetch is in flight. Arms the wake before reading
    /// the count so a foreground fetch finishing between the check and the await
    /// isn't missed.
    async fn await_foreground_idle(&self) {
        loop {
            let waited = self.idle.notified();
            if self.foreground_inflight.load(Ordering::Acquire) == 0 {
                return;
            }
            waited.await;
        }
    }

    /// Run one fetch `fut` under the gate. A foreground (playing) fetch runs
    /// immediately, tracked so preloads yield to it; a preload fetch waits until
    /// no foreground fetch is in flight, then runs. `foreground` is the caller's
    /// `is_foreground(buffer_id)`, taken once so the caller can reuse it (a log
    /// label). Keeping the begin/end accounting here means it can't drift between
    /// the fill and the prefetch.
    async fn run_gated<F: std::future::Future>(&self, foreground: bool, fut: F) -> F::Output {
        if foreground {
            self.begin_foreground_fetch();
            let out = fut.await;
            self.end_foreground_fetch();
            out
        } else {
            self.await_foreground_idle().await;
            fut.await
        }
    }
}

/// Invoked at most once, when the fill fails, after the buffer has been failed
/// with the same error — so blocked readers have already unblocked with it and
/// a decoder reports it as its own outcome. Playback needs the handler anyway:
/// only its command loop knows whether the buffer feeds the playing track or a
/// preload, which decides whether playback halts. Loudness and save only log
/// here; their decode error carries the cause.
pub type FillErrorHandler = Box<dyn FnOnce(Arc<PlaybackError>) + Send>;

/// Reads audio data into a sparse buffer for streaming decode.
///
/// Local reads serve bytes from disk; attached files are read through coven,
/// which verifies and decrypts their stored contents.
pub trait AudioDataReader: Send + 'static {
    /// Start reading data into the buffer.
    ///
    /// Spawns an async task that follows the readers: it fetches the window each
    /// reader needs next (read-ahead), evicts what they have passed, and idles
    /// when every read-ahead window is full -- so playback starts after one
    /// window and memory stays bounded to a window around the playhead instead
    /// of the whole file. Coven's opened file stream supplies verified plaintext
    /// for each requested window.
    ///
    /// Returns the fill task, which holds the source open until the buffer is
    /// stopped or dropped. A caller that must know the source is closed awaits
    /// [`FillTask::ended`] after stopping the buffer; one that need not drops it.
    fn start_reading(
        self: Box<Self>,
        buffer: SharedSparseBuffer,
        on_error: FillErrorHandler,
    ) -> FillTask;
}

/// The task filling a buffer from its source. It holds the source open until
/// the buffer is stopped or dropped; dropping this detaches it.
pub struct FillTask(tokio::task::JoinHandle<()>);

impl FillTask {
    /// Wait for the fill to end, which is when its source is closed. A panic
    /// in the fill resumes here.
    pub async fn ended(self) {
        if let Err(error) = self.0.await {
            if error.is_panic() {
                std::panic::resume_unwind(error.into_panic());
            }
        }
    }
}

/// Reads a file the caller names by path: a file being previewed, or an
/// import's sources before they are the library's.
pub struct LocalReader {
    path: std::path::PathBuf,
}

impl LocalReader {
    pub fn new(path: impl Into<std::path::PathBuf>) -> Self {
        Self { path: path.into() }
    }
}

/// Fail the buffer with a read error raised before its fill started (the
/// source would not open), then report it.
fn fail_audio_read(on_error: FillErrorHandler, buffer: &SharedSparseBuffer, error: PlaybackError) {
    let error = Arc::new(error);
    // A buffer that already stopped means teardown ran before this read
    // returned: the user switched away while the open was in flight. The error
    // belongs to a track that has already left the pipeline, so there is
    // nothing left to report.
    if !buffer.fail(SourceReadError::new(error.clone())) {
        debug!("ignoring read failure on a stopped buffer: {error}");
        return;
    }
    report_fill_failure(on_error, error);
}

/// Report a failure the buffer already carries (the fill failed it before
/// returning the error).
fn report_fill_failure(on_error: FillErrorHandler, error: Arc<PlaybackError>) {
    error!("audio read failed: {error}");
    on_error(error);
}

impl AudioDataReader for LocalReader {
    fn start_reading(
        self: Box<Self>,
        buffer: SharedSparseBuffer,
        on_error: FillErrorHandler,
    ) -> FillTask {
        let path = self.path;

        FillTask(tokio::spawn(async move {
            use tokio::io::{AsyncReadExt, AsyncSeekExt};

            let file = match tokio::fs::File::open(&path).await {
                Ok(f) => f,
                Err(e) => {
                    fail_audio_read(
                        on_error,
                        &buffer,
                        PlaybackError::io(format!("Failed to open {}", path.display()), e),
                    );
                    return;
                }
            };
            // One handle, seeked per fetch. The fill loop awaits each fetch in
            // turn, so the lock is never actually contended.
            let file = tokio::sync::Mutex::new(file);

            let result = buffer
                .fill_on_demand(FILL_WINDOW_SIZE, |src_off, len| {
                    let file = &file;
                    let path = &path;
                    async move {
                        let mut f = file.lock().await;
                        f.seek(std::io::SeekFrom::Start(src_off))
                            .await
                            .map_err(|e| {
                                PlaybackError::io(
                                    format!("Failed to seek {} to {src_off}", path.display()),
                                    e,
                                )
                            })?;
                        let mut buf = vec![0u8; len as usize];
                        f.read_exact(&mut buf).await.map_err(|e| {
                            PlaybackError::io(
                                format!(
                                    "Failed to read {len} bytes at {src_off} from {}",
                                    path.display()
                                ),
                                e,
                            )
                        })?;
                        Ok(buf)
                    }
                })
                .await;

            if let Err(e) = result {
                report_fill_failure(on_error, e);
            }
        }))
    }
}

/// Preserve the actionable storage failure when opening or reading audio.
fn playback_error_for_file_read(error: crate::db::DbError) -> PlaybackError {
    match error {
        crate::db::DbError::FileRead(coven::FileReadError::NoStorage) => {
            PlaybackError::SyncDisconnected
        }
        // Audio another device holds: a release nobody uploaded is on the
        // device that imported it, or made it local again.
        crate::db::DbError::FileRead(coven::FileReadError::DeviceOnly { .. }) => {
            PlaybackError::NotUploaded
        }
        crate::db::DbError::FileRead(
            coven::FileReadError::UserFileMissing { .. }
            | coven::FileReadError::UserFileChanged { .. },
        ) => PlaybackError::OriginalMissing,
        error => PlaybackError::internal(error.to_string()),
    }
}

/// Create a reader for a release file, read through coven.
pub fn create_audio_reader(
    library_manager: &crate::library::LibraryManager,
    file_id: &str,
    arbiter: Arc<FetchArbiter>,
    prefetch_byte: Option<u64>,
    prefetch_file_end: bool,
) -> Box<dyn AudioDataReader> {
    library_manager.create_audio_reader(file_id, arbiter, prefetch_byte, prefetch_file_end)
}

/// Streams a release file's plaintext from one opened source. The
/// single playback reader: coven resolves locality and proves the file's identity
/// when the stream opens, so playback never branches on locality and every window
/// after that costs only its own bytes.
pub struct CovenFileReader {
    db: crate::db::Database,
    file_id: String,
    arbiter: Arc<FetchArbiter>,
    /// The byte the playing track's audio starts at, when it's past the header
    /// (a track that seeks into the file). The reader fetches this window up
    /// front, in parallel with the demuxer's header probe, so the decoder's seek
    /// finds the bytes already buffered instead of paying a second serial
    /// round-trip. `None` for a track that starts at byte 0.
    prefetch_byte: Option<u64>,
    /// Whether to also prefetch the file's end window. `true` for APE, whose
    /// demuxer reads the tail (its mandatory index lives there) when it opens, so
    /// the open would otherwise stall on a serial ranged fetch. The shared album
    /// buffer keeps the window, so it's pulled once per image.
    prefetch_file_end: bool,
}

impl CovenFileReader {
    pub(crate) fn new(
        db: crate::db::Database,
        file_id: String,
        arbiter: Arc<FetchArbiter>,
        prefetch_byte: Option<u64>,
        prefetch_file_end: bool,
    ) -> Self {
        Self {
            db,
            file_id,
            arbiter,
            prefetch_byte,
            prefetch_file_end,
        }
    }
}

impl AudioDataReader for CovenFileReader {
    fn start_reading(
        self: Box<Self>,
        buffer: SharedSparseBuffer,
        on_error: FillErrorHandler,
    ) -> FillTask {
        let CovenFileReader {
            db,
            file_id,
            arbiter,
            prefetch_byte,
            prefetch_file_end,
        } = *self;
        let buffer_id = buffer.id();

        FillTask(tokio::spawn(async move {
            let stream = match db.open_release_file(&file_id).await {
                Ok(stream) => Arc::new(stream),
                Err(error) => {
                    fail_audio_read(on_error, &buffer, playback_error_for_file_read(error));
                    return;
                }
            };
            // The length coven proved when it opened the stream, not a number the
            // caller tracked: it bounds the prefetch windows below.
            let source_size = stream.plaintext_size();

            // Prefetch the track's start window in parallel with the demuxer's header
            // probe. The decoder reads the header at byte 0, then seeks to the track,
            // and the fill only fetches what a registered reader is already demanding —
            // so without this the seek blocks while the fill notices the new demand and
            // comes round to it. Since the stored `start_byte` is the exact seektable
            // landing (the playback seek lands there — a by-byte FLAC seek jumps
            // straight to it, and APE sample-seeks its index to it), pull that window
            // now, concurrent with the probe, so the seek lands on buffered bytes.
            if let Some(start) = prefetch_byte {
                if start < source_size {
                    spawn_window_prefetch(
                        stream.clone(),
                        arbiter.clone(),
                        &buffer,
                        source_size,
                        start,
                        "track-start",
                    );
                }
            }

            // APE reads the file's end when its demuxer opens (its index lives in the
            // tail), so prefetch that window too — otherwise the open stalls the same
            // way, waiting for the fill to come round to a tail nobody had demanded
            // yet. Skipped when the file is a single window (the fill covers it).
            if prefetch_file_end && source_size > CLOUD_STREAM_READ_SIZE {
                spawn_window_prefetch(
                    stream.clone(),
                    arbiter.clone(),
                    &buffer,
                    source_size,
                    source_size - CLOUD_STREAM_READ_SIZE,
                    "file-end",
                );
            }

            info!("CovenFileReader: buffer={buffer_id} source_size={source_size}");
            // This reader's running fetch total and the wall-clock origin, so each
            // window's log shows how many bytes have been read off the stream and how
            // long into it -- the visibility into start-up bandwidth and into a preload
            // competing with the playing track.
            let fetched = AtomicU64::new(0);
            let started = Instant::now();
            let result = buffer.fill_on_demand(FILL_WINDOW_SIZE, |src_off, len| {
                let fut = stream.read_at(src_off, len);
                let arbiter = &arbiter;
                let fetched = &fetched;
                async move {
                    // The playing track fetches immediately; a preload waits
                    // while the playing track has a fetch in flight so it can't
                    // slow the current track's start.
                    let foreground = arbiter.is_foreground(buffer_id);
                    debug!(
                        "fetch start buffer={buffer_id} {} off={src_off} len={len} {}ms",
                        if foreground { "playing" } else { "preload" },
                        started.elapsed().as_millis(),
                    );
                    let fetch_started = Instant::now();
                    let data = match arbiter.run_gated(foreground, fut).await {
                        Ok(data) => data,
                        Err(e) => {
                            debug!(
                                "fetch failed buffer={buffer_id} {} off={src_off} len={len} waited={}ms error={e}",
                                if foreground { "playing" } else { "preload" },
                                fetch_started.elapsed().as_millis(),
                            );
                            return Err(playback_error_for_file_read(e.into()));
                        }
                    };
                    let total =
                        fetched.fetch_add(data.len() as u64, Ordering::Relaxed) + data.len() as u64;
                    debug!(
                        "fetch buffer={buffer_id} {} off={src_off} len={len} total={total}B {}ms",
                        if foreground { "playing" } else { "preload" },
                        started.elapsed().as_millis(),
                    );
                    Ok(data)
                }
            })
            .await;

            if let Err(e) = result {
                report_fill_failure(on_error, e);
            }
        }))
    }
}

// The fill fetches one window at a time off the reader's opened `coven::FileStream`, where
// a read is a positioned read of an already-proven handle -- so a window costs its
// own bytes and nothing more, and the window size is purely a
// readahead-vs-memory choice rather than a way to amortize per-call overhead.
// 4 MiB keeps both modest.
const CLOUD_STREAM_READ_SIZE: u64 = FILL_WINDOW_SIZE;

/// Fetch one window starting at `off` up front, in parallel with the demuxer's
/// header probe, and append it to `buffer` so a later read finds it buffered
/// instead of waiting for the fill to notice a demand there. Arbiter-gated exactly
/// like the fill: the playing track fetches immediately; a preload yields while
/// the playing track has a fetch in flight, so preloading the next track can't
/// steal the current track's startup bandwidth. Best-effort — a failed prefetch
/// just leaves the fill to fetch the window on demand. `what` labels the window
/// (track-start / file-end) in the log.
fn spawn_window_prefetch(
    stream: Arc<coven::FileStream>,
    arbiter: Arc<FetchArbiter>,
    buffer: &SharedSparseBuffer,
    source_size: u64,
    off: u64,
    what: &'static str,
) {
    let buffer_id = buffer.id();
    let weak = Arc::downgrade(buffer);
    let window = CLOUD_STREAM_READ_SIZE.min(source_size - off);
    let started = Instant::now();
    tokio::spawn(async move {
        let foreground = arbiter.is_foreground(buffer_id);
        let result = arbiter
            .run_gated(foreground, stream.read_at(off, window))
            .await;
        match result {
            Ok(data) => {
                if let Some(buf) = weak.upgrade() {
                    debug!(
                        "prefetch buffer={buffer_id} {what} off={off} len={window} {}ms",
                        started.elapsed().as_millis()
                    );
                    buf.append_at(off, &data);
                }
            }
            Err(e) => debug!("prefetch buffer={buffer_id} {what} off={off} failed: {e}"),
        }
    });
}

#[cfg(test)]
#[path = "data_source_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "data_source_decode_tests.rs"]
mod decode_tests;
