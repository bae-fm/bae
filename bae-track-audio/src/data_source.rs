//! Unified data source abstraction for audio playback.
//!
//! Provides a common interface for reading audio bytes from:
//! - Local files (non-storage releases, or storage releases with local backend)
//! - Cloud storage (storage releases with cloud backend)

use crate::storage_wait::{FetchProgress, StorageWaitWatch, StorageWaits};
use crate::TrackReadError;
use bae_audio::sparse_buffer::{SharedSparseBuffer, SourceReadError};
use bae_diagnostics::Diagnostics;
use bae_util::task_failure::{caught, joined};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Instant;
use tokio::sync::Notify;
use tracing::{debug, error, info};

mod local;
pub use local::LocalReader;

/// How much a fill fetches at a time: 64 of coven's chunks, 4 MiB. The buffer
/// keeps one window ahead of a reader that hasn't set its ceiling, one behind
/// the most-behind reader, and the file's first window with one AVIO buffer
/// past it; import spaces a FLAC's scrub points so a seek to one reads from a
/// single window (an MP3's 10 seconds fit in one at any bitrate).
pub const FILL_WINDOW_SIZE: u64 = coven::CHUNK_SIZE as u64 * 64;

/// Prioritizes audio byte fetches so the track the user is waiting to hear gets
/// the bandwidth. One foreground track is designated at a time -- the playing
/// track -- and its fetches run immediately. A next-track preload's fetches wait
/// while the foreground track has a fetch in flight, so preloading the next
/// track can't take bandwidth from the current track's start. One arbiter is
/// shared by every reader through the playback service; the service designates
/// the foreground via [`FetchArbiter::set_foreground`] whenever a track becomes
/// current. The arbiter also hears which of the playing track's reads fetch
/// from storage, so it knows whether the track's load waits on storage.
pub struct FetchArbiter {
    /// The foreground (playing) track and the buffer it reads from, and the
    /// storage reads it waits on.
    waits: Arc<StorageWaits>,
    /// Count of foreground fetches currently awaiting bytes. A preload fetch
    /// waits while this is non-zero.
    foreground_inflight: AtomicU64,
    /// Wakes waiting preload fetches when the foreground goes idle or changes.
    idle: Notify,
}

impl FetchArbiter {
    pub fn new() -> Arc<Self> {
        Arc::new(Self {
            waits: Arc::new(StorageWaits::new()),
            foreground_inflight: AtomicU64::new(0),
            idle: Notify::new(),
        })
    }

    /// Designate `track_id`, reading from the buffers `buffer_ids`, one per
    /// file it reads, as the foreground (playing) track. Wakes any waiting
    /// preload in case the previous foreground is gone.
    pub fn set_foreground(&self, buffer_ids: Vec<u64>, track_id: &str) {
        self.waits.set_foreground(buffer_ids, track_id);
        self.idle.notify_waiters();
    }

    fn is_foreground(&self, buffer_id: u64) -> bool {
        self.waits.is_foreground(buffer_id)
    }

    /// Whether a fetch into `buffer_id` runs as the playing track's, for the
    /// tests of which buffers playback marks.
    #[cfg(any(test, feature = "test-utils"))]
    pub fn fetches_as_playing(&self, buffer_id: u64) -> bool {
        self.is_foreground(buffer_id)
    }

    /// The track a load the person waits on is for, or `None` once nothing
    /// loads: while it is the playing track, its load waits on storage while
    /// one of its reads fetches from there.
    pub fn set_waiting(&self, track_id: Option<&str>) {
        self.waits.set_waiting(track_id);
    }

    /// Hear the playing track while its load waits on storage.
    pub fn subscribe_storage_waits(&self) -> StorageWaitWatch {
        self.waits.subscribe()
    }

    /// Where a read into `buffer_id` hears from coven whether it fetches
    /// from storage, while it runs.
    fn read_progress(&self, buffer_id: u64) -> FetchProgress {
        self.waits.read(buffer_id)
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
pub type FillErrorHandler = Box<dyn FnOnce(Arc<TrackReadError>) + Send>;

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
    /// window and memory stays bounded to a window around the playhead, plus
    /// the file's first window and one AVIO buffer past it, instead of the
    /// whole file. Coven's opened file stream supplies verified plaintext for
    /// each requested window.
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
pub struct FillTask {
    task: tokio::task::JoinHandle<()>,
    diagnostics: Diagnostics,
}

impl FillTask {
    /// Wait for the fill to end, which is when its source is closed. A panic in
    /// the fill's work was reported and failed the buffer, so its readers have
    /// already stopped on it; one in reporting a failure is reported here.
    pub async fn ended(self) {
        if let Err(failed) = joined("report a buffer fill's failure", self.task).await {
            self.diagnostics.bug(failed);
        }
    }
}

/// Why a fill stopped short of its source's end.
enum FillFailure {
    /// The source would not open, so the buffer is not failed yet.
    Open(TrackReadError),
    /// A read failed; the fill already failed the buffer with it.
    Read(Arc<TrackReadError>),
}

/// Spawn the task running `fill` into `buffer`. A failure fails the buffer,
/// if the fill hasn't already, and goes to `on_error`. A panic in `fill` is
/// reported as a bug and fails the buffer the same way, so its readers stop
/// and `on_error` runs instead of every reader waiting for bytes that never
/// come. The task holds the buffer only weakly, as the fill does, so dropping
/// its last reader still ends the fill.
fn spawn_fill(
    diagnostics: Diagnostics,
    buffer: &SharedSparseBuffer,
    on_error: FillErrorHandler,
    fill: impl std::future::Future<Output = Result<(), FillFailure>> + Send + 'static,
) -> FillTask {
    let buffer = Arc::downgrade(buffer);
    let fill_diagnostics = diagnostics.clone();
    let task = tokio::spawn(async move {
        let error = match caught("fill a playback buffer", fill).await {
            Ok(Ok(())) => return,
            Ok(Err(FillFailure::Read(error))) => return report_fill_failure(on_error, error),
            Ok(Err(FillFailure::Open(error))) => error,
            Err(bug) => TrackReadError::task(fill_diagnostics.panicked(bug)),
        };
        match buffer.upgrade() {
            Some(buffer) => fail_audio_read(on_error, &buffer, error),
            None => debug!("ignoring read failure on a dropped buffer: {error}"),
        }
    });
    FillTask { task, diagnostics }
}

/// Fail the buffer with a read error raised outside its fill loop (the source
/// would not open, or the fill panicked), then report it.
fn fail_audio_read(on_error: FillErrorHandler, buffer: &SharedSparseBuffer, error: TrackReadError) {
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
fn report_fill_failure(on_error: FillErrorHandler, error: Arc<TrackReadError>) {
    error!("audio read failed: {error}");
    on_error(error);
}

/// Preserve the actionable storage failure when opening or reading release
/// file `file_id` from `db`: a file on another device only names that
/// device, as coven records it.
async fn read_error_for_file_read(
    db: &bae_db::Database,
    file_id: &str,
    error: bae_db::DbError,
) -> TrackReadError {
    match error {
        bae_db::DbError::FileRead(coven::FileReadError::NoStorage) => {
            TrackReadError::SyncDisconnected
        }
        // Audio another device holds: a release nobody uploaded is on the
        // device that imported it, or made it local again.
        bae_db::DbError::FileRead(coven::FileReadError::DeviceOnly { .. }) => {
            match db.release_file_other_device(file_id).await {
                Ok(device) => TrackReadError::OnOtherDevice(device),
                Err(error) => TrackReadError::internal(format!(
                    "release file {file_id} is on another device only, which couldn't be read: {error}"
                )),
            }
        }
        bae_db::DbError::FileRead(
            coven::FileReadError::UserFileMissing { .. }
            | coven::FileReadError::UserFileChanged { .. },
        ) => TrackReadError::OriginalMissing,
        error => TrackReadError::internal(error.to_string()),
    }
}

/// Streams a release file's plaintext from one opened source. The single
/// reader of release files: coven resolves locality and proves the file's
/// identity when the stream opens, so no reader branches on locality and every
/// window after that costs only its own bytes.
pub struct CovenFileReader {
    db: bae_db::Database,
    file_id: String,
    /// How playback reads the file, for a reader playback started; `None`
    /// for one that reads it start to finish with nobody waiting to hear it.
    playback: Option<PlaybackRead>,
    diagnostics: Diagnostics,
}

/// How playback reads a release file: its fetches gated by the arbiter that
/// puts the playing track's first, and the windows it fetches up front.
struct PlaybackRead {
    arbiter: Arc<FetchArbiter>,
    /// The byte the decode first reads past the file's header, when known
    /// (`StreamDecodeParams::first_read_byte`): a track's landing byte, or the
    /// scrub point a decode from a position starts at. The reader fetches
    /// this window up front, in parallel with the demuxer's header probe, so
    /// the decoder's jump finds the bytes already buffered instead of paying a
    /// second serial round-trip. `None` when the decode reads on from the
    /// header or only the demuxer knows where it lands.
    prefetch_byte: Option<u64>,
    /// Whether to also prefetch the file's end window. `true` for APE, whose
    /// demuxer reads the tail (its mandatory index lives there) when it opens, so
    /// the open would otherwise stall on a serial ranged fetch. The shared album
    /// buffer keeps the window, so it's pulled once per image.
    prefetch_file_end: bool,
}

impl CovenFileReader {
    /// A reader of `file_id` start to finish, for work nobody listens to
    /// while it reads — a save, a transcode, a decode for import.
    pub fn new(db: bae_db::Database, file_id: String, diagnostics: Diagnostics) -> Self {
        Self { db, file_id, playback: None, diagnostics }
    }

    /// A reader of `file_id` for playback: its fetches gated by `arbiter`,
    /// with the window at `prefetch_byte` and, when `prefetch_file_end`, the
    /// file's last window fetched up front.
    pub fn for_playback(
        db: bae_db::Database,
        file_id: String,
        arbiter: Arc<FetchArbiter>,
        prefetch_byte: Option<u64>,
        prefetch_file_end: bool,
        diagnostics: Diagnostics,
    ) -> Self {
        let playback = PlaybackRead { arbiter, prefetch_byte, prefetch_file_end };
        Self { db, file_id, playback: Some(playback), diagnostics }
    }
}

/// One window's read off `stream`: for playback, gated so the playing track
/// fetches immediately and a preload waits while the playing track has a
/// fetch in flight, and told to the arbiter's storage waits; otherwise read
/// as it comes. Says which kind of read it was, for the log.
async fn read_window(
    stream: &coven::FileStream,
    playback: Option<&PlaybackRead>,
    buffer_id: u64,
    offset: u64,
    len: u64,
) -> (Result<Vec<u8>, coven::FileReadError>, &'static str) {
    match playback {
        Some(playback) => {
            let read = playback.arbiter.read_progress(buffer_id);
            let foreground = playback.arbiter.is_foreground(buffer_id);
            let fetch = stream.read_at_reporting(offset, len, read.progress());
            let data = playback.arbiter.run_gated(foreground, fetch).await;
            (data, if foreground { "playing" } else { "preload" })
        }
        None => (stream.read_at(offset, len).await, "read"),
    }
}

impl AudioDataReader for CovenFileReader {
    fn start_reading(
        self: Box<Self>,
        buffer: SharedSparseBuffer,
        on_error: FillErrorHandler,
    ) -> FillTask {
        let CovenFileReader { db, file_id, playback, diagnostics } = *self;
        let buffer_id = buffer.id();
        let fill_buffer = buffer.clone();
        let prefetch_diagnostics = diagnostics.clone();

        spawn_fill(diagnostics, &buffer, on_error, async move {
            let buffer = fill_buffer;
            let stream = match db.open_release_file(&file_id).await {
                Ok(stream) => Arc::new(stream),
                Err(error) => {
                    return Err(FillFailure::Open(
                        read_error_for_file_read(&db, &file_id, error).await,
                    ))
                }
            };
            // The length coven proved when it opened the stream, not a number the
            // caller tracked: it bounds the prefetch windows below.
            let source_size = stream.plaintext_size();

            if let Some(playback) = &playback {
                let prefetch = |start: u64, what: &'static str| {
                    spawn_window_prefetch(
                        &prefetch_diagnostics,
                        stream.clone(),
                        playback.arbiter.clone(),
                        &buffer,
                        source_size,
                        start,
                        what,
                    )
                };

                // Prefetch the decode's first window in parallel with the
                // demuxer's header probe. The decoder reads the header at byte 0,
                // then jumps to where it starts, and the fill only fetches what a
                // registered reader is already demanding — so without this the
                // jump blocks while the fill notices the new demand and comes
                // round to it. The byte is exact (a track's stored landing, which
                // a by-byte FLAC seek jumps straight to and APE sample-seeks its
                // index to, or a stored scrub point), so pull that window now,
                // concurrent with the probe, so the jump lands on buffered bytes.
                if let Some(start) = playback.prefetch_byte {
                    if start < source_size {
                        prefetch(start, "decode-start");
                    }
                }

                // APE reads the file's end when its demuxer opens (its index lives in
                // the tail), so prefetch that window too — otherwise the open stalls
                // the same way, waiting for the fill to come round to a tail nobody
                // had demanded yet. Skipped when the file is a single window (the
                // fill covers it).
                if playback.prefetch_file_end && source_size > CLOUD_STREAM_READ_SIZE {
                    prefetch(source_size - CLOUD_STREAM_READ_SIZE, "file-end");
                }
            }

            info!("CovenFileReader: buffer={buffer_id} source_size={source_size}");
            // This reader's running fetch total and the wall-clock origin, so each
            // window's log shows how many bytes have been read off the stream and how
            // long into it -- the visibility into start-up bandwidth and into a preload
            // competing with the playing track.
            let fetched = AtomicU64::new(0);
            let started = Instant::now();
            buffer
                .fill_on_demand(FILL_WINDOW_SIZE, |src_off, len| {
                    let stream = &stream;
                    let playback = playback.as_ref();
                    let fetched = &fetched;
                    let db = &db;
                    let file_id = &file_id;
                    async move {
                        debug!(
                            "fetch start buffer={buffer_id} off={src_off} len={len} {}ms",
                            started.elapsed().as_millis(),
                        );
                        let fetch_started = Instant::now();
                        let (data, kind) =
                            read_window(stream, playback, buffer_id, src_off, len).await;
                        let data = match data {
                            Ok(data) => data,
                            Err(e) => {
                                debug!(
                                    "fetch failed buffer={buffer_id} {kind} off={src_off} len={len} waited={}ms error={e}",
                                    fetch_started.elapsed().as_millis(),
                                );
                                return Err(read_error_for_file_read(db, file_id, e.into()).await);
                            }
                        };
                        let total = fetched.fetch_add(data.len() as u64, Ordering::Relaxed)
                            + data.len() as u64;
                        debug!(
                            "fetch buffer={buffer_id} {kind} off={src_off} len={len} total={total}B {}ms",
                            started.elapsed().as_millis(),
                        );
                        Ok(data)
                    }
                })
                .await
                .map_err(FillFailure::Read)
        })
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
/// just leaves the fill to fetch the window on demand, and so does one that
/// panicked, which is reported. `what` labels the window (decode-start /
/// file-end) in the log.
fn spawn_window_prefetch(
    diagnostics: &Diagnostics,
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
    diagnostics.spawn_reported(
        &tokio::runtime::Handle::current(),
        "prefetch a playback buffer window",
        async move {
            let foreground = arbiter.is_foreground(buffer_id);
            let read = arbiter.read_progress(buffer_id);
            let result = arbiter
                .run_gated(foreground, stream.read_at_reporting(off, window, read.progress()))
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
        },
        // Nothing waits on a prefetch: the fill fetches the window on demand.
        |_| {},
    );
}

#[cfg(test)]
#[path = "data_source_tests.rs"]
mod tests;
