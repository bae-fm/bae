//! Unified data source abstraction for audio playback.
//!
//! Provides a common interface for reading audio bytes from:
//! - Local files (non-storage releases, or storage releases with local backend)
//! - Cloud storage (storage releases with cloud backend)

use crate::playback::sparse_buffer::{SharedSparseBuffer, SparseStreamingBuffer, FILL_WINDOW_SIZE};
#[cfg(test)]
use crate::playback::sparse_buffer::{KEEP_BEHIND, MIN_READAHEAD};
use crate::playback::PlaybackError;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Weak};
use std::time::Instant;
use tokio::sync::Notify;
use tracing::{debug, error, info};

/// Prioritizes audio byte fetches so the track the user is waiting to hear gets
/// the bandwidth. One foreground track is designated at a time -- the playing
/// track -- and its fetches run immediately. A next-track preload's fetches wait
/// while the foreground track has a fetch in flight, so preloading the next
/// track can't take bandwidth from the current track's start. One arbiter is
/// shared by every reader through the playback service; the service designates
/// the foreground via [`FetchArbiter::set_foreground`] whenever a track becomes
/// current.
pub struct FetchArbiter {
    /// The [`SparseStreamingBuffer::id`] of the foreground (playing) track, or
    /// `u64::MAX` for none.
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

/// Invoked at most once, when the fill fails; the buffer is cancelled right
/// after, so blocked readers unblock and the decoder surfaces the failure.
/// Playback emits a `PlaybackProgress::PlaybackError` here; loudness and save
/// log the cause and let their decode failure carry the outcome.
pub type FillErrorHandler = Box<dyn FnOnce(PlaybackError) + Send>;

/// Reads audio data into a sparse buffer for streaming decode.
///
/// Local reads serve plaintext bytes from disk; cloud reads apply the home's
/// at-rest cipher to each blob — decrypting under the library key on an opaque
/// home, or reading the verbatim bytes on a browsable one.
pub trait AudioDataReader: Send + 'static {
    /// Start reading data into the buffer.
    ///
    /// Spawns an async task that follows the readers: it fetches the window each
    /// reader needs next (read-ahead), evicts what they have passed, and idles
    /// when every read-ahead window is full -- so playback starts after one
    /// window and memory stays bounded to a window around the playhead instead
    /// of the whole file. A cloud blob's bytes are decrypted once, when the
    /// reader opens its stream (under the library key on an opaque home, verbatim
    /// on a browsable one); the windows are then read from that plaintext.
    fn start_reading(self: Box<Self>, buffer: SharedSparseBuffer, on_error: FillErrorHandler);
}

/// Reads from local filesystem.
///
/// Used for:
/// - Non-storage releases (files at original import location)
/// - Storage releases with local backend
pub struct LocalReader {
    path: String,
}

impl LocalReader {
    pub fn new(path: impl Into<String>) -> Self {
        Self { path: path.into() }
    }
}

fn fail_audio_read(on_error: FillErrorHandler, buffer: &SharedSparseBuffer, error: PlaybackError) {
    // A buffer that is already cancelled means teardown ran before this read
    // returned: the user switched away while a reader was parked in an in-flight
    // `read().await` past its top-of-loop cancel check. The Err it eventually
    // returns belongs to a track that has already left the pipeline, so there is
    // nothing left to report — exit cancelled.
    if buffer.is_cancelled() {
        debug!("ignoring read failure on a cancelled buffer: {error}");
        return;
    }
    error!("audio read failed: {error}");
    on_error(error);
    buffer.cancel();
}

/// Report a fill failure, upgrading the fill's `Weak` to reach the buffer. If
/// the buffer is already gone, the track was torn down before this error
/// surfaced -- there's nothing to halt, so just log and drop it.
fn report_fill_failure(
    on_error: FillErrorHandler,
    buffer: &Weak<SparseStreamingBuffer>,
    error: PlaybackError,
) {
    match buffer.upgrade() {
        Some(buf) => fail_audio_read(on_error, &buf, error),
        None => debug!("ignoring fill failure on a dropped buffer: {error}"),
    }
}

impl AudioDataReader for LocalReader {
    fn start_reading(self: Box<Self>, buffer: SharedSparseBuffer, on_error: FillErrorHandler) {
        let path = self.path;

        tokio::spawn(async move {
            use tokio::io::{AsyncReadExt, AsyncSeekExt};

            let weak_buffer = Arc::downgrade(&buffer);

            let file = match tokio::fs::File::open(&path).await {
                Ok(f) => f,
                Err(e) => {
                    report_fill_failure(
                        on_error,
                        &weak_buffer,
                        PlaybackError::io(format!("Failed to open file {path}: {e}")),
                    );
                    return;
                }
            };
            // One handle, seeked per fetch. The fill loop awaits each fetch in
            // turn, so the lock is never actually contended.
            let file = tokio::sync::Mutex::new(file);

            let result = buffer
                .fill_on_demand(|src_off, len| {
                    let file = &file;
                    let path = &path;
                    async move {
                        let mut f = file.lock().await;
                        f.seek(std::io::SeekFrom::Start(src_off))
                            .await
                            .map_err(|e| {
                                PlaybackError::io(format!(
                                    "Failed to seek {path} to {src_off}: {e}"
                                ))
                            })?;
                        let mut buf = vec![0u8; len as usize];
                        f.read_exact(&mut buf).await.map_err(|e| {
                            PlaybackError::io(format!(
                                "Failed to read {len} bytes at {src_off} from {path}: {e}"
                            ))
                        })?;
                        Ok(buf)
                    }
                })
                .await;

            if let Err(e) = result {
                report_fill_failure(on_error, &weak_buffer, e);
            }
        });
    }
}

/// Map a coven blob-read failure onto the playback error the UI can key on.
/// `NoCloudHome` — a Remote blob needed the cloud and no provider is
/// connected — is the "reconnect sync" state. `ExternalMissing` — the
/// user-provided source file is gone — is "still uploading" when a queued
/// upload for this file explains the missing source, and a plain diagnostic
/// otherwise (the user moved or deleted their files). Everything else is
/// un-enumerable for the UI and stays diagnostic with the coven error chain
/// as the opaque detail.
async fn playback_error_for_blob_read(
    db: &crate::db::Database,
    file_id: &str,
    error: coven::BlobCacheError,
) -> PlaybackError {
    match &error {
        coven::BlobCacheError::NoCloudHome => PlaybackError::SyncDisconnected,
        coven::BlobCacheError::ExternalMissing { .. } => {
            match db.has_pending_cloud_upload(file_id).await {
                Ok(true) => PlaybackError::UploadPending,
                Ok(false) => PlaybackError::internal(format!("Blob read failed: {error}")),
                // The read already failed; a broken outbox check must not
                // mask it (or be masked) — carry both.
                Err(db_error) => PlaybackError::internal(format!(
                    "Blob read failed: {error}; pending-upload check also failed: {db_error}"
                )),
            }
        }
        _ => PlaybackError::internal(format!("Blob read failed: {error}")),
    }
}

/// Create the playback reader for a release file. coven owns locality: the reader
/// opens one [`coven::BlobStream`] over the file, which resolves *where the bytes
/// are* once — the user's own file (a Local user-provided blob's external ref),
/// coven's local store (a Local host-provided blob), `storage/pinned`/`storage/cache`
/// on a Remote hit, or the cloud on a Remote miss (decrypting under the home's
/// at-rest cipher, and populating the cache) — and then serves every window off that
/// one proven handle. A failure to open (a vanished external file, or a Remote blob
/// with no cloud home) surfaces through the fill, so this is infallible to build.
///
/// Neither the blob's size nor its cloud path is passed in: coven derives both from
/// the live `release_files` row, and the opened stream reports the plaintext length
/// it actually proved.
pub fn create_audio_reader(
    library_manager: &crate::library::LibraryManager,
    file_id: &str,
    arbiter: Arc<FetchArbiter>,
    prefetch_byte: Option<u64>,
    prefetch_file_end: bool,
) -> Box<dyn AudioDataReader> {
    library_manager.create_audio_reader(file_id, arbiter, prefetch_byte, prefetch_file_end)
}

/// Streams a release file's plaintext off one opened [`coven::BlobStream`]. The
/// single playback reader: coven resolves locality and proves the blob's identity
/// when the stream opens, so playback never branches on locality and every window
/// after that costs only its own bytes.
pub struct CovenBlobReader {
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

impl CovenBlobReader {
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

impl AudioDataReader for CovenBlobReader {
    fn start_reading(self: Box<Self>, buffer: SharedSparseBuffer, on_error: FillErrorHandler) {
        let CovenBlobReader {
            db,
            file_id,
            arbiter,
            prefetch_byte,
            prefetch_file_end,
        } = *self;
        let buffer_id = buffer.id();

        tokio::spawn(async move {
            // Bind the exact-row blob reference once for this reader, from the live
            // `release_files` row; the stream opened from it is bound to that exact
            // row version, so a later row replacement can't redirect a read. A missing
            // row surfaces through the fill like any other read failure.
            let blob = match db
                .row_blob_ref(crate::sync::RELEASE_FILES_NAMESPACE, &file_id)
                .await
            {
                Ok(blob) => blob,
                Err(e) => {
                    fail_audio_read(
                        on_error,
                        &buffer,
                        PlaybackError::internal(format!("blob ref for {file_id}: {e}")),
                    );
                    return;
                }
            };

            // Open the stream once for this reader. coven binds the row's exact blob
            // reference and resolves its locality here; each positioned read then
            // verifies and returns only the requested local bytes or authenticated
            // remote chunks. Opening is also where a missing external file or
            // unavailable cloud home surfaces, so its failure takes the same
            // classification as a failed range read: that is what carries "reconnect
            // sync" and "still uploading" to the UI.
            let stream = match db.open_blob_stream(&blob).await {
                Ok(stream) => Arc::new(stream),
                Err(e) => {
                    let error = playback_error_for_blob_read(&db, blob.row_id(), e).await;
                    fail_audio_read(on_error, &buffer, error);
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

            info!("CovenBlobReader: buffer={buffer_id} source_size={source_size}");
            // This reader's running fetch total and the wall-clock origin, so each
            // window's log shows how many bytes have been read off the stream and how
            // long into it -- the visibility into start-up bandwidth and into a preload
            // competing with the playing track.
            let fetched = AtomicU64::new(0);
            let started = Instant::now();
            let weak_buffer = Arc::downgrade(&buffer);
            let result = buffer.fill_on_demand(|src_off, len| {
                let fut = stream.read_at(src_off, len);
                let arbiter = &arbiter;
                let fetched = &fetched;
                let db = &db;
                let blob = &blob;
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
                            return Err(playback_error_for_blob_read(db, blob.row_id(), e).await);
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
                report_fill_failure(on_error, &weak_buffer, e);
            }
        });
    }
}

// The fill fetches one window at a time off the reader's opened `BlobStream`, where
// a read is a positioned read of an already-proven handle -- so a window costs its
// own bytes and nothing more, and the window size is purely a
// readahead-vs-memory choice rather than a way to amortize per-call overhead.
// 4 MiB keeps both modest.
const CLOUD_STREAM_READ_SIZE: u64 = FILL_WINDOW_SIZE;

/// The minimum the fill keeps buffered ahead of a reader whose track ceiling
/// isn't set yet -- the brief probe phase before the decoder reads the header
/// and seeks to the track's start. One window: the demuxer reads only the front
/// metadata (header + seektable, ~100 KB for a FLAC) before it seeks, so keeping
/// more than a window ahead of byte 0 just speculatively fetches front bytes the
/// decoder abandons the instant it seeks away -- wasted reads and wasted buffer on
/// every track that doesn't start at byte 0. Once the decoder
/// seeks and sets its real ceiling (the track's end byte), read-ahead is bounded
/// by that instead and reaches the rest of the track.
/// Fetch one window starting at `off` up front, in parallel with the demuxer's
/// header probe, and append it to `buffer` so a later read finds it buffered
/// instead of waiting for the fill to notice a demand there. Arbiter-gated exactly
/// like the fill: the playing track fetches immediately; a preload yields while
/// the playing track has a fetch in flight, so preloading the next track can't
/// steal the current track's startup bandwidth. Best-effort — a failed prefetch
/// just leaves the fill to fetch the window on demand. `what` labels the window
/// (track-start / file-end) in the log.
fn spawn_window_prefetch(
    stream: Arc<coven::BlobStream>,
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
