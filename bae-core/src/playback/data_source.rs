//! Unified data source abstraction for audio playback.
//!
//! Provides a common interface for reading audio bytes from:
//! - Local files (non-storage releases, or storage releases with local backend)
//! - Cloud storage (storage releases with cloud backend)

use crate::playback::progress::{emit_progress, PlaybackProgress};
use crate::playback::sparse_buffer::{ReaderDemand, SharedSparseBuffer, SparseStreamingBuffer};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Weak};
use std::time::Instant;
use tokio::sync::mpsc as tokio_mpsc;
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

/// Reads audio data into a sparse buffer for streaming playback.
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
    /// of the whole file. On an opaque home the cloud reader decrypts each
    /// window as it lands; on a browsable one it reads the bytes verbatim.
    fn start_reading(
        self: Box<Self>,
        buffer: SharedSparseBuffer,
        progress_tx: tokio_mpsc::UnboundedSender<PlaybackProgress>,
    );
}

/// Configuration for reading audio data.
#[derive(Debug, Clone)]
pub struct AudioReadConfig {
    /// Path to the audio file (local path or cloud key)
    pub path: String,
    pub source_size: u64,
}

/// Reads from local filesystem.
///
/// Used for:
/// - Non-storage releases (files at original import location)
/// - Storage releases with local backend
pub struct LocalReader {
    config: AudioReadConfig,
}

impl LocalReader {
    pub fn new(config: AudioReadConfig) -> Self {
        Self { config }
    }
}

fn fail_audio_read(
    progress_tx: &tokio_mpsc::UnboundedSender<PlaybackProgress>,
    buffer: &SharedSparseBuffer,
    log_message: String,
    message: String,
) {
    // A buffer that is already cancelled means teardown ran before this read
    // returned: the user switched away while a reader was parked in an in-flight
    // `read().await` past its top-of-loop cancel check. The Err it eventually
    // returns belongs to the abandoned track. Emitting PlaybackError here would
    // halt the track the user just started, so suppress it and exit cancelled.
    if buffer.is_cancelled() {
        debug!("ignoring read failure on a cancelled buffer: {log_message}");
        return;
    }
    error!("{}", log_message);
    emit_progress(
        progress_tx,
        PlaybackProgress::PlaybackError {
            reason: crate::ui::PlaybackErrorReason::internal(message),
        },
    );
    buffer.cancel();
}

/// Report a fill failure, upgrading the fill's `Weak` to reach the buffer. If
/// the buffer is already gone, the track was torn down before this error
/// surfaced -- there's nothing to halt, so just log and drop it.
fn report_fill_failure(
    progress_tx: &tokio_mpsc::UnboundedSender<PlaybackProgress>,
    buffer: &Weak<SparseStreamingBuffer>,
    log_message: String,
    message: String,
) {
    match buffer.upgrade() {
        Some(buf) => fail_audio_read(progress_tx, &buf, log_message, message),
        None => debug!("ignoring fill failure on a dropped buffer: {log_message}"),
    }
}

/// Hand back the fill-wake signal and a weak buffer ref, consuming the caller's
/// strong ref. The fill follows the buffer's real users (decoder + cache) via
/// the `Weak` and must not keep the buffer alive itself, so the strong ref is
/// dropped here. Both readers prepare their fill this way. The file size is
/// already set at the buffer's construction, so there's nothing else to wire.
fn prepare_fill(buffer: SharedSparseBuffer) -> (Arc<Notify>, Weak<SparseStreamingBuffer>) {
    let wake = buffer.fill_wake_handle();
    (wake, Arc::downgrade(&buffer))
}

impl AudioDataReader for LocalReader {
    fn start_reading(
        self: Box<Self>,
        buffer: SharedSparseBuffer,
        progress_tx: tokio_mpsc::UnboundedSender<PlaybackProgress>,
    ) {
        // The file size is recorded at the buffer's construction; this reader
        // only needs the path (the fill reads bytes through `fetch`).
        let AudioReadConfig { path, .. } = self.config;
        let (wake, buffer) = prepare_fill(buffer);

        tokio::spawn(async move {
            use tokio::io::{AsyncReadExt, AsyncSeekExt};

            let file = match tokio::fs::File::open(&path).await {
                Ok(f) => f,
                Err(e) => {
                    let message = format!("Failed to open file {path}: {e}");
                    report_fill_failure(&progress_tx, &buffer, message.clone(), message);
                    return;
                }
            };
            // One handle, seeked per fetch. The fill loop awaits each fetch in
            // turn, so the lock is never actually contended.
            let file = tokio::sync::Mutex::new(file);

            let result = fill_buffer_on_demand(buffer.clone(), wake, |src_off, len| {
                let file = &file;
                async move {
                    let mut f = file.lock().await;
                    f.seek(std::io::SeekFrom::Start(src_off))
                        .await
                        .map_err(|e| format!("seek to {src_off}: {e}"))?;
                    let mut buf = vec![0u8; len as usize];
                    f.read_exact(&mut buf)
                        .await
                        .map_err(|e| format!("read {len} at {src_off}: {e}"))?;
                    Ok(buf)
                }
            })
            .await;

            if let Err(e) = result {
                let message = format!("Read error in file {path}: {e}");
                report_fill_failure(&progress_tx, &buffer, message.clone(), message);
            }
        });
    }
}

/// Create the playback reader for a release file. coven owns locality: one reader
/// streams every range through [`coven::CovenHandle::open_blob_stream`], which
/// resolves *where the bytes are* per read — the user's own file (a Local
/// user-provided blob's external ref), coven's local store (a Local host-provided
/// blob), `storage/pinned`/`storage/cache` on a Remote hit, or the cloud on a
/// Remote miss (decrypting under the home's at-rest cipher). A read failure (a
/// vanished external file, or a Remote blob with no cloud home) surfaces through
/// the fill, so this is infallible to build.
pub fn create_audio_reader(
    library_manager: &crate::library::LibraryManager,
    file_id: &str,
    cloud_path: Option<&str>,
    source_size: u64,
    arbiter: Arc<FetchArbiter>,
    prefetch_byte: Option<u64>,
    prefetch_file_end: bool,
) -> Box<dyn AudioDataReader> {
    let blob = coven::BlobRef {
        namespace: crate::sync::RELEASE_FILES_NAMESPACE.to_string(),
        id: file_id.to_string(),
        scope: coven::BlobScope::Master,
        cloud_path: cloud_path.map(str::to_string),
        provenance: coven::Provenance::UserProvided,
        fill: coven::CacheFill::CacheLazy,
    };
    Box::new(CovenBlobReader {
        handle: library_manager.handle().clone(),
        blob,
        source_size,
        arbiter,
        prefetch_byte,
        prefetch_file_end,
    })
}

/// Streams a release file's plaintext through coven's locality-aware ranged read.
/// The single playback reader: coven resolves each window to the external file,
/// the local store, the cache, or the cloud (decrypting as needed), so playback
/// never branches on locality.
pub struct CovenBlobReader {
    handle: coven::CovenHandle,
    blob: coven::BlobRef,
    source_size: u64,
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

impl AudioDataReader for CovenBlobReader {
    fn start_reading(
        self: Box<Self>,
        buffer: SharedSparseBuffer,
        progress_tx: tokio_mpsc::UnboundedSender<PlaybackProgress>,
    ) {
        let CovenBlobReader {
            handle,
            blob,
            source_size,
            arbiter,
            prefetch_byte,
            prefetch_file_end,
        } = *self;
        let buffer_id = buffer.id();

        // Prefetch the track's start window in parallel with the demuxer's header
        // probe. The decoder reads the header at byte 0, then seeks to the track;
        // that seek is a second serial round-trip over a cloud home. Since the
        // stored `start_byte` is the exact seektable landing (the playback seek
        // lands there — a by-byte FLAC seek jumps straight to it, and APE
        // sample-seeks its index to it), pull that window now, concurrent with the
        // probe, so the seek lands on buffered bytes.
        if let Some(start) = prefetch_byte {
            if start < source_size {
                spawn_window_prefetch(
                    handle.clone(),
                    blob.clone(),
                    arbiter.clone(),
                    &buffer,
                    source_size,
                    start,
                    "track-start",
                );
            }
        }

        // APE reads the file's end when its demuxer opens (its index lives in the
        // tail), so prefetch that window too — otherwise the open stalls on a
        // serial ranged fetch of the end. Skipped when the file is a single window
        // (the fill covers it).
        if prefetch_file_end && source_size > CLOUD_STREAM_READ_SIZE {
            spawn_window_prefetch(
                handle.clone(),
                blob.clone(),
                arbiter.clone(),
                &buffer,
                source_size,
                source_size - CLOUD_STREAM_READ_SIZE,
                "file-end",
            );
        }

        let (wake, buffer) = prepare_fill(buffer);

        tokio::spawn(async move {
            info!("CovenBlobReader: buffer={buffer_id} source_size={source_size}");
            // This reader's running fetch total and the wall-clock origin, so
            // each window's log shows how many bytes have been pulled from coven
            // and how long into the stream -- the visibility into start-up
            // bandwidth and into a preload competing with the playing track.
            let fetched = AtomicU64::new(0);
            let started = Instant::now();
            let result = fill_buffer_on_demand(buffer.clone(), wake, |src_off, len| {
                let fut = handle.open_blob_stream(&blob, source_size, src_off, len);
                let arbiter = &arbiter;
                let fetched = &fetched;
                async move {
                    // The playing track fetches immediately; a preload waits
                    // while the playing track has a fetch in flight so it can't
                    // slow the current track's start.
                    let foreground = arbiter.is_foreground(buffer_id);
                    let data = arbiter.run_gated(foreground, fut).await;
                    let data = data.map_err(|e| e.to_string())?;
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
                let error = e.to_string();
                report_fill_failure(
                    &progress_tx,
                    &buffer,
                    format!("Blob read failed: {error}"),
                    error,
                );
            }
        });
    }
}

// The fill fetches one window at a time through coven's `open_blob_stream`, which
// resolves locality per call (an external-ref lookup + a file open, or a cloud
// range read). A larger window means fewer such calls per second of playback, so
// the per-call overhead stays well under the playback CPU budget; 4 MiB keeps the
// readahead and per-track memory modest while cutting the call rate.
const CLOUD_STREAM_READ_SIZE: u64 = coven::CHUNK_SIZE as u64 * 64;

/// The minimum the fill keeps buffered ahead of a reader whose track ceiling
/// isn't set yet -- the brief probe phase before the decoder reads the header
/// and seeks to the track's start. One window: the demuxer reads only the front
/// metadata (header + seektable, ~100 KB for a FLAC) before it seeks, so keeping
/// more than a window ahead of byte 0 just speculatively fetches front bytes the
/// decoder abandons the instant it seeks away -- a wasted ranged fetch (~1 s over
/// a cloud home) on every track that doesn't start at byte 0. Once the decoder
/// seeks and sets its real ceiling (the track's end byte), read-ahead is bounded
/// by that instead and reaches the rest of the track.
const MIN_READAHEAD: u64 = CLOUD_STREAM_READ_SIZE;

/// How far behind a reader's position buffered bytes are retained before being
/// evicted. A margin for the decoder's brief backward reads; everything older is
/// dropped so memory stays bounded to a window around the playhead.
const KEEP_BEHIND: u64 = CLOUD_STREAM_READ_SIZE;

/// Fetch one window starting at `off` up front, in parallel with the demuxer's
/// header probe, and append it to `buffer` so a later read finds it buffered
/// instead of paying a serial round-trip over a cloud home. Arbiter-gated exactly
/// like the fill: the playing track fetches immediately; a preload yields while
/// the playing track has a fetch in flight, so preloading the next track can't
/// steal the current track's startup bandwidth. Best-effort — a failed prefetch
/// just leaves the fill to fetch the window on demand. `what` labels the window
/// (track-start / file-end) in the log.
fn spawn_window_prefetch(
    handle: coven::CovenHandle,
    blob: coven::BlobRef,
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
            .run_gated(
                foreground,
                handle.open_blob_stream(&blob, source_size, off, window),
            )
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

/// The next region to fetch, given each live reader's demand ascending by
/// position: serve the lowest-position reader first so the playing track stays
/// fed before a further-ahead reader (e.g. a gapless preload). With a track
/// ceiling set, the reader is fetched up to it -- the rest of the current track,
/// and no further, so a short track doesn't over-fetch into the next one. Without
/// one (during probe, before the decoder positions the reader), the fill keeps
/// `MIN_READAHEAD` ahead so the ring can't underrun. Returns `None` when every
/// reader is buffered up to its target -- the fill then idles.
///
/// There is no backfill: the buffer only holds the current track(s) around the
/// live readers, never the whole album. A backward seek into an evicted region
/// re-publishes a demand there and the fill re-fetches that window.
fn pick_window_gap(
    buffer: &SharedSparseBuffer,
    windows: &[ReaderDemand],
    total: u64,
) -> Option<(u64, u64)> {
    for demand in windows {
        let ceil = match demand.ceiling {
            Some(c) => c.min(total),
            None => (demand.pos + MIN_READAHEAD).min(total),
        };
        if let Some(gap) = buffer.next_gap(demand.pos, ceil) {
            return Some(gap);
        }
    }
    None
}

/// Fill the buffer from the source, following the readers: fetch the window each
/// reader needs next (read-ahead), evict what they have passed, and idle when
/// every read-ahead window is full -- resuming when a reader advances or seeks.
/// The buffer addresses the file directly (buffer offset == source byte offset)
/// and `fetch(off, len)` returns exactly `len` plaintext source bytes.
///
/// Holds only a `Weak`: the fill follows the buffer's real users (the decoder's
/// readers and the service's shared-buffer cache) and never keeps an abandoned
/// buffer alive. When the last user drops it, the upgrade fails (or the buffer's
/// `Drop` wakes a parked fill) and the loop exits. Until then: playback starts
/// after the first window, memory stays bounded to a window around the playhead,
/// and a seek into an evicted region re-fetches just that window. The strong ref
/// is released before every `await` so the buffer can be freed while we wait; no
/// lock is held across `fetch().await`.
async fn fill_buffer_on_demand<F, Fut>(
    buffer: Weak<SparseStreamingBuffer>,
    wake: Arc<Notify>,
    fetch: F,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>>
where
    F: Fn(u64, u64) -> Fut,
    Fut: std::future::Future<Output = Result<Vec<u8>, String>>,
{
    // The file size is fixed at the buffer's construction.
    let total = match buffer.upgrade() {
        Some(buf) => buf.get_total_size(),
        None => {
            debug!("fill_buffer_on_demand: buffer dropped before start, nothing to fill");
            return Ok(());
        }
    };

    loop {
        // Acquire a strong ref for this iteration's synchronous work only.
        let Some(buf) = buffer.upgrade() else {
            // The buffer's last real user is gone: nothing left to fill for.
            debug!("fill_buffer_on_demand: buffer dropped, stopping");
            return Ok(());
        };

        // Full cancel = track change / stop: the writer abandons the buffer.
        if buf.is_cancelled() {
            debug!("fill_buffer_on_demand: buffer cancelled, stopping");
            return Ok(());
        }

        // Each live reader's demand, ascending by position. `first()` is the
        // eviction floor (the most-behind reader); the whole set drives
        // read-ahead. One query serves both.
        let windows = buf.demand_windows();

        // Free bytes every reader has already passed so memory stays bounded to
        // the current track(s) around the playhead.
        if let Some(floor) = windows.first() {
            buf.evict_before(floor.pos.saturating_sub(KEEP_BEHIND));
        }

        let gap = pick_window_gap(&buf, &windows, total);
        // Release the strong ref before any await so the buffer can be freed
        // (and this fill woken via its `Drop`) while we wait.
        drop(buf);

        let Some((gap_start, gap_end)) = gap else {
            // Every reader's read-ahead window is full: park until a reader
            // advances, seeks, the buffer is cancelled, or it is dropped. The
            // wake stores a permit, so a signal during this check isn't lost.
            wake.notified().await;
            continue;
        };

        let window = CLOUD_STREAM_READ_SIZE.min(gap_end - gap_start);
        let data = fetch(gap_start, window)
            .await
            .map_err(|e| -> Box<dyn std::error::Error + Send + Sync> { e.into() })?;

        if data.len() != window as usize {
            return Err(format!(
                "Source read returned {} bytes for requested {window} at {gap_start}",
                data.len()
            )
            .into());
        }

        // Re-acquire to store the window; if the buffer vanished during the
        // fetch, drop the bytes and exit.
        let Some(buf) = buffer.upgrade() else {
            debug!(
                "fill_buffer_on_demand: buffer dropped during fetch, \
                 discarding {window} bytes at {gap_start}"
            );
            return Ok(());
        };
        buf.append_at(gap_start, &data);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::playback::sparse_buffer::create_sparse_buffer;
    use std::io::Write;
    use std::sync::Mutex;
    use std::time::Duration;
    use tempfile::NamedTempFile;
    use tokio::time::timeout;

    /// Ordered record of `read_range(start, end)` calls, shared with a test.
    type ReadLog = Arc<Mutex<Vec<(u64, u64)>>>;

    /// Per-offset gate: a range read parks until its `start` is released. Used
    /// to prove a seek's window is fetched and unblocks the reader while other
    /// windows are still held — every read blocks by default; the test releases
    /// only the offsets it wants served.
    struct ReadGate {
        released: Mutex<std::collections::HashSet<u64>>,
        notify: tokio::sync::Notify,
    }

    impl ReadGate {
        fn new() -> Self {
            Self {
                released: Mutex::new(std::collections::HashSet::new()),
                notify: tokio::sync::Notify::new(),
            }
        }

        async fn wait_for(&self, start: u64) {
            loop {
                // Arm the notification before checking so a release between the
                // check and the await isn't lost.
                let notified = self.notify.notified();
                if self.released.lock().unwrap().contains(&start) {
                    return;
                }
                notified.await;
            }
        }

        fn release(&self, start: u64) {
            self.released.lock().unwrap().insert(start);
            self.notify.notify_waiters();
        }
    }

    /// Drain a sparse buffer to the end, returning everything read.
    fn drain(buffer: &SharedSparseBuffer) -> Vec<u8> {
        let mut reader = buffer.new_reader();
        let mut out = Vec::new();
        let mut chunk = vec![0u8; 1024];
        loop {
            match reader.read(&mut chunk) {
                Some(0) | None => break,
                Some(n) => out.extend_from_slice(&chunk[..n]),
            }
        }
        out
    }

    async fn drain_async(buffer: SharedSparseBuffer) -> Vec<u8> {
        tokio::task::spawn_blocking(move || drain(&buffer))
            .await
            .expect("drain task")
    }

    async fn next_playback_error(
        progress_rx: &mut tokio_mpsc::UnboundedReceiver<PlaybackProgress>,
        context: &str,
    ) -> String {
        use crate::ui::{PlaybackErrorReason, UiError};
        match progress_rx.recv().await.expect(context) {
            PlaybackProgress::PlaybackError {
                reason:
                    PlaybackErrorReason::Diagnostic {
                        error: UiError::Diagnostic { detail, .. },
                    },
            } => detail,
            other => panic!("expected diagnostic playback error, got: {other:?}"),
        }
    }

    fn full_file_config(path: impl Into<String>, source_size: u64) -> AudioReadConfig {
        AudioReadConfig {
            path: path.into(),
            source_size,
        }
    }

    const WINDOW: u64 = CLOUD_STREAM_READ_SIZE;

    /// A preload reader's fetch waits while the playing track has a fetch in
    /// flight, and proceeds once the playing track goes idle. This is the gate
    /// that keeps preloading the next track from taking bandwidth from the track
    /// the user just started. Sabotage — make `await_foreground_idle` return
    /// immediately — and the waiter finishes while the playing track is still
    /// fetching, failing the `!is_finished` assertion.
    #[tokio::test]
    async fn preload_fetch_yields_while_the_playing_track_is_fetching() {
        let arbiter = FetchArbiter::new();
        let playing = 1u64;
        let preload = 2u64;

        arbiter.set_foreground(playing);
        assert!(arbiter.is_foreground(playing));
        assert!(!arbiter.is_foreground(preload));

        // The playing track has a fetch in flight.
        arbiter.begin_foreground_fetch();

        // A preload waiting for the foreground to idle must not proceed yet.
        let waiting = arbiter.clone();
        let waiter = tokio::spawn(async move { waiting.await_foreground_idle().await });
        tokio::task::yield_now().await;
        assert!(
            !waiter.is_finished(),
            "a preload must wait while the playing track is fetching"
        );

        // Once the playing track's fetch finishes, the preload proceeds.
        arbiter.end_foreground_fetch();
        timeout(Duration::from_secs(5), waiter)
            .await
            .expect("a preload must unblock once the playing track goes idle")
            .expect("waiter task");
    }

    /// Spawn the production `fill_buffer_on_demand` loop over an in-memory
    /// `blob`, filling `buffer`. The fetch serves `blob[start..end]` directly —
    /// source offsets map identity into the buffer — so the seek-ordering and
    /// eviction assertions read against plaintext offsets. Records every
    /// `(start, end)` fetch into the returned log and, when `gate` is `Some`,
    /// blocks each fetch until its `start` is released. This drives the same fill
    /// loop the readers use, isolated from the encrypt/cloud fetch source so the
    /// loop's demand and eviction behavior is what's under test.
    fn start_recording_fill(
        blob: Vec<u8>,
        gate: Option<Arc<ReadGate>>,
        buffer: &SharedSparseBuffer,
    ) -> ReadLog {
        let read_log: ReadLog = Arc::new(Mutex::new(Vec::new()));
        let blob = Arc::new(blob);
        let (wake, weak) = prepare_fill(buffer.clone());
        let log = read_log.clone();
        tokio::spawn(async move {
            // Normal teardown (the buffer cancelled or its weak ref dropped)
            // returns `Ok(())`; only a real fetch failure or short read returns
            // `Err`. So `expect` here fails the test on a genuine fill error
            // without firing on clean teardown.
            fill_buffer_on_demand(weak, wake, move |start, len| {
                let blob = blob.clone();
                let log = log.clone();
                let gate = gate.clone();
                async move {
                    let end = start + len;
                    log.lock().unwrap().push((start, end));
                    if let Some(gate) = &gate {
                        gate.wait_for(start).await;
                    }
                    Ok(blob[start as usize..end as usize].to_vec())
                }
            })
            .await
            .expect("recording fill failed");
        });
        read_log
    }

    /// A seek fetches only the demanded window — never the skipped prefix. With
    /// the reader parked at `6*WINDOW` before the fill runs, the fill fetches
    /// that window (and its read-ahead) and never touches `[WINDOW, 6*WINDOW)`:
    /// there is no backfill, so opening a late track doesn't drag the whole
    /// earlier prefix down. A sequential fill would fetch `[WINDOW, 2*WINDOW)`
    /// long before the target.
    #[tokio::test]
    async fn seek_fetches_only_the_demanded_window_not_the_skipped_prefix() {
        let source_size = 8 * WINDOW;
        let blob: Vec<u8> = (0..source_size).map(|i| (i % 251) as u8).collect();

        let buffer = create_sparse_buffer(source_size);
        // Register the seek demand BEFORE the fill loop starts so the first fetch
        // is demand-driven (this is the seek case: the reader already exists).
        let mut reader = buffer.new_reader();
        assert!(reader.seek(6 * WINDOW));

        let read_log = start_recording_fill(blob, None, &buffer);

        // Read one byte at the seek target from the real blocking unit.
        let one = tokio::task::spawn_blocking(move || {
            let mut b = [0u8; 1];
            let n = reader.read(&mut b);
            // Hold the reader until after the read so its demand stays registered.
            drop(reader);
            n
        })
        .await
        .expect("seek read task");
        assert_eq!(one, Some(1), "the seek target byte must be served");

        // Stop the fill, then snapshot the fetch order.
        buffer.cancel();
        let log = read_log.lock().unwrap().clone();

        assert!(
            log.iter().any(|&(start, _)| start >= 6 * WINDOW),
            "a window at or past the seek target must have been fetched; log: {log:?}"
        );
        assert!(
            !log.iter()
                .any(|&(start, _)| (WINDOW..6 * WINDOW).contains(&start)),
            "no window in the skipped prefix [{}, {}) may be fetched — there is no \
             backfill; log: {:?}",
            WINDOW,
            6 * WINDOW,
            log,
        );
    }

    /// The parallel prefetch removes the serial fetch at the track's start byte:
    /// with the landing window already buffered (the prefetch completed), the fill
    /// serves the seek target without fetching it; with a cold buffer the fill must
    /// fetch that window. This is the whole point of pulling the start byte in
    /// parallel with the header probe -- the decoder's seek lands on buffered bytes
    /// instead of a second round-trip.
    #[tokio::test]
    async fn prefetched_start_window_removes_the_seek_target_fetch() {
        let source_size = 8 * WINDOW;
        let blob: Vec<u8> = (0..source_size).map(|i| (i % 251) as u8).collect();
        let start_byte = 3 * WINDOW; // a deep track's landing

        // Read one byte at `start_byte` and return the fill's fetch log.
        async fn seek_and_log(
            buffer: SharedSparseBuffer,
            blob: Vec<u8>,
            start_byte: u64,
        ) -> Vec<(u64, u64)> {
            let mut reader = buffer.new_reader();
            assert!(reader.seek(start_byte));
            let read_log = start_recording_fill(blob, None, &buffer);
            let served = tokio::task::spawn_blocking(move || {
                let mut b = [0u8; 1];
                let n = reader.read(&mut b);
                drop(reader);
                n
            })
            .await
            .expect("seek read task");
            assert_eq!(served, Some(1), "the seek target byte must be served");
            // Let the fill settle so its read-ahead fetches are recorded.
            for _ in 0..100 {
                tokio::task::yield_now().await;
            }
            buffer.cancel();
            let log = read_log.lock().unwrap().clone();
            log
        }

        // With the landing window already buffered (the prefetch completed), the
        // fill must not fetch it.
        let prefetched = create_sparse_buffer(source_size);
        prefetched.append_at(
            start_byte,
            &blob[start_byte as usize..(start_byte + WINDOW) as usize],
        );
        let with = seek_and_log(prefetched, blob.clone(), start_byte).await;
        assert!(
            !with.iter().any(|&(start, _)| start == start_byte),
            "prefetched: the fill must not re-fetch the already-buffered start \
             window; log: {with:?}"
        );

        // Cold buffer: the fill must fetch the start window to serve the seek.
        let cold = create_sparse_buffer(source_size);
        let without = seek_and_log(cold, blob, start_byte).await;
        assert!(
            without.iter().any(|&(start, _)| start == start_byte),
            "cold: the fill must fetch the start window; log: {without:?}"
        );
    }

    /// Playing a file front-to-back recovers every byte, yet the buffer never
    /// holds more than a window around the playhead: the fill evicts what the
    /// reader has passed instead of accumulating the whole file in RAM. Sabotage
    /// check — drop the `evict_before` call and the buffer grows to the full
    /// `source_size`, failing the bound.
    #[tokio::test]
    async fn forward_play_evicts_so_memory_stays_bounded() {
        let source_size = 16 * WINDOW;
        let blob: Vec<u8> = (0..source_size).map(|i| (i % 251) as u8).collect();

        let buffer = create_sparse_buffer(source_size);
        let _read_log = start_recording_fill(blob.clone(), None, &buffer);

        let out = drain_async(buffer.clone()).await;
        assert_eq!(out, blob, "front-to-back play must recover the whole file");

        // No ceiling is set on this reader, so it fetches the cushion ahead and
        // evicts behind: the whole file passes through but only a window's worth
        // is retained.
        let bound = MIN_READAHEAD + KEEP_BEHIND + 2 * WINDOW;
        let buffered = buffer.total_buffered();
        assert!(
            buffered <= bound,
            "buffer must stay windowed: held {buffered} bytes, bound {bound}, \
             whole file {source_size}"
        );
    }

    /// A reader with a track ceiling set has the fill buffer the whole track
    /// ahead of it -- up to the ceiling -- not just the fixed cushion, and it
    /// stops at the ceiling rather than fetching the rest of the file. This is
    /// the "keep the whole current track" behavior: a single-file album fetches
    /// its current track's byte range; a per-track file sets the ceiling to the
    /// whole file. Sabotage -- ignore the ceiling and fetch only the cushion --
    /// and the reader stalls well short of the ceiling, failing the wait.
    #[tokio::test]
    async fn a_ceiling_fetches_the_whole_track_then_stops() {
        let source_size = 16 * WINDOW;
        let blob: Vec<u8> = (0..source_size).map(|i| (i % 251) as u8).collect();

        // The track ends at 12 windows -- past the cushion (MIN_READAHEAD, 4
        // windows) and short of the 16-window file.
        let ceiling = 12 * WINDOW;
        let buffer = create_sparse_buffer(source_size);
        let mut reader = buffer.new_reader();
        reader.set_readahead_ceiling(ceiling);
        assert!(reader.seek(0));
        let _read_log = start_recording_fill(blob, None, &buffer);

        // The reader sits at 0 (held, not draining); the fill should buffer up to
        // the ceiling. Yield to let the (current-thread) fill run.
        let mut reached = false;
        for _ in 0..2000 {
            if buffer.is_buffered(ceiling - 1) {
                reached = true;
                break;
            }
            tokio::task::yield_now().await;
        }
        assert!(
            reached,
            "the fill must buffer up to the track ceiling ({}W), not stop at the \
             {}W cushion",
            ceiling / WINDOW,
            MIN_READAHEAD / WINDOW,
        );
        assert!(
            !buffer.is_buffered(ceiling + WINDOW),
            "the fill must stop at the ceiling, not fetch the rest of the file"
        );

        drop(reader);
        buffer.cancel();
    }

    /// A backward seek into an already-evicted region re-fetches just that
    /// window and serves the read. The fill is still alive after the forward
    /// pass, so evicted bytes are recoverable on demand — proven by the window
    /// being fetched a second time and the re-read returning the correct bytes.
    #[tokio::test]
    async fn backward_seek_into_evicted_region_refetches_the_window() {
        let source_size = 16 * WINDOW;
        let blob: Vec<u8> = (0..source_size).map(|i| (i % 251) as u8).collect();

        let buffer = create_sparse_buffer(source_size);
        let read_log = start_recording_fill(blob.clone(), None, &buffer);

        let reader_buffer = buffer.clone();
        let (n, got) = tokio::task::spawn_blocking(move || {
            let mut reader = reader_buffer.new_reader();
            // Play forward well past the second window so [WINDOW, 2*WINDOW) is
            // evicted (it is more than KEEP_BEHIND behind the playhead).
            let mut sink = vec![0u8; 10 * WINDOW as usize];
            let mut filled = 0;
            while filled < sink.len() {
                match reader.read(&mut sink[filled..]) {
                    Some(0) => panic!(
                        "unexpected EOF at {filled} reading a {}-byte file",
                        16 * WINDOW
                    ),
                    None => panic!("unexpected cancel at {filled}"),
                    Some(read) => filled += read,
                }
            }
            // Seek back into the evicted window and read it again.
            assert!(reader.seek(WINDOW));
            let mut b = [0u8; 32];
            let n = reader.read(&mut b);
            (n, b)
        })
        .await
        .expect("forward-then-back task");

        let n = n.expect("the backward-seek read returned data, not EOF/cancel");
        let target = WINDOW as usize;
        assert_eq!(
            &got[..n],
            &blob[target..target + n],
            "re-fetched bytes must match the file at the seek target"
        );

        buffer.cancel();
        let fetches_at_window = read_log
            .lock()
            .unwrap()
            .iter()
            .filter(|&&(start, _)| start == WINDOW)
            .count();
        assert!(
            fetches_at_window >= 2,
            "the window at {WINDOW} must be fetched twice (once forward, once \
             after it was evicted and sought back into); fetches: {fetches_at_window}"
        );
    }

    /// Spawned through the production `start_reading` path, the fill holds only
    /// a `Weak` (it never keeps the buffer alive) and parks when no reader has a
    /// demand. Dropping the buffer's last strong ref frees it, and the buffer's
    /// `Drop` wakes the parked fill so the task exits — releasing its clone of
    /// the wake handle. Sabotage either half — `start_reading` keeping a strong
    /// ref, or removing the `Drop` wake — and an assertion fails. Yields, not a
    /// timer, advance the single-threaded test runtime.
    #[tokio::test]
    async fn start_reading_fill_follows_the_buffer_and_exits_when_it_is_dropped() {
        let source_size = 4 * WINDOW;
        let blob: Vec<u8> = (0..source_size).map(|i| (i % 251) as u8).collect();
        let mut temp_file = NamedTempFile::new().unwrap();
        temp_file.write_all(&blob).unwrap();
        temp_file.flush().unwrap();

        let buffer = create_sparse_buffer(source_size);
        // Our own clone of the wake handle; the fill takes another via
        // `start_reading`, and the buffer holds the third.
        let wake = buffer.fill_wake_handle();
        let reader = Box::new(LocalReader::new(full_file_config(
            temp_file.path().to_str().expect("temp path is UTF-8"),
            source_size,
        )));
        let (progress_tx, _progress_rx) = tokio_mpsc::unbounded_channel();
        reader.start_reading(buffer.clone(), progress_tx);
        let weak = Arc::downgrade(&buffer);

        // Let the spawned fill run to its park. With no reader it fetches nothing
        // and holds only a weak ref, leaving the test as the buffer's sole owner.
        for _ in 0..20 {
            tokio::task::yield_now().await;
        }
        assert_eq!(
            Arc::strong_count(&buffer),
            1,
            "start_reading's fill must hold only a Weak, not keep the buffer alive"
        );

        // Drop the last strong ref: the buffer frees and its Drop wakes the
        // parked fill, which upgrades to None and exits, dropping its wake clone.
        drop(buffer);
        assert!(weak.upgrade().is_none(), "the buffer must be freed");
        for _ in 0..20 {
            tokio::task::yield_now().await;
        }
        assert_eq!(
            Arc::strong_count(&wake),
            1,
            "the parked fill task must exit when the buffer is dropped, not leak"
        );
    }

    /// A reader blocked at the seek target unblocks as soon as that target's
    /// window lands — it does not wait for the prefix to download. Every read is
    /// gated; only the demanded window is released, yet the read returns while
    /// `[WINDOW, 6*WINDOW)` are still unfetched. A purely sequential fill would
    /// park on the front window (offset 0, never released) and never reach the
    /// target, so the reader would hang.
    #[tokio::test]
    async fn blocked_reader_unblocks_on_demanded_window_not_sequential_catch_up() {
        let source_size = 8 * WINDOW;
        let blob: Vec<u8> = (0..source_size).map(|i| (i % 251) as u8).collect();
        let gate = Arc::new(ReadGate::new());

        let buffer = create_sparse_buffer(source_size);
        let mut reader = buffer.new_reader();
        assert!(reader.seek(6 * WINDOW));
        let read_log = start_recording_fill(blob, Some(gate.clone()), &buffer);

        // Release ONLY the demanded window; everything else stays parked.
        gate.release(6 * WINDOW);

        let read_handle = tokio::task::spawn_blocking(move || {
            let mut b = [0u8; 1];
            let n = reader.read(&mut b);
            drop(reader);
            n
        });

        let outcome = timeout(Duration::from_secs(10), read_handle).await;

        // Whatever happened, unblock every parked reader/fill so their threads
        // exit (a reader stuck on the condvar can't otherwise be cancelled) — and
        // snapshot the read log before threads can fetch anything more.
        let log = read_log.lock().unwrap().clone();
        buffer.cancel();
        gate.release(0);

        let served = outcome
            .expect("a blocked reader must unblock once its demanded window lands, not hang")
            .expect("read task");
        assert_eq!(served, Some(1), "the demanded window served the read");

        // The skipped prefix [WINDOW, 6*WINDOW) must still be unfetched — the
        // reader did not wait for a sequential crawl up to the target.
        assert!(
            !log.iter()
                .any(|&(start, _)| (WINDOW..6 * WINDOW).contains(&start)),
            "no window in [{}, {}) should be fetched yet; read order: {:?}",
            WINDOW,
            6 * WINDOW,
            log,
        );
    }

    #[tokio::test]
    async fn test_local_file_reader_full_file() {
        // Create temp file with test data
        let mut temp_file = NamedTempFile::new().unwrap();
        let test_data = b"Hello, this is test audio data for streaming!";
        temp_file.write_all(test_data).unwrap();
        temp_file.flush().unwrap();

        let config = full_file_config(
            temp_file.path().to_str().expect("temp path is UTF-8"),
            test_data.len() as u64,
        );

        let reader = Box::new(LocalReader::new(config));
        let buffer = create_sparse_buffer(test_data.len() as u64);

        let (progress_tx, _progress_rx) = tokio_mpsc::unbounded_channel();
        reader.start_reading(buffer.clone(), progress_tx);
        let result = drain_async(buffer.clone()).await;

        assert_eq!(result, test_data);
    }

    /// A deep seek into a large local file is served through the shared
    /// demand-driven fill loop: a reader parked at `6*WINDOW` before the loop
    /// starts reads the exact file bytes at that offset (not the prefix). The
    /// byte pattern is position-dependent, so wrong-offset fill would mismatch.
    #[tokio::test]
    async fn local_seek_into_large_file_serves_target_via_shared_fill() {
        let source_size = 8 * WINDOW;
        let data: Vec<u8> = (0..source_size).map(|i| (i % 251) as u8).collect();
        let mut temp_file = NamedTempFile::new().unwrap();
        temp_file.write_all(&data).unwrap();
        temp_file.flush().unwrap();

        let config = full_file_config(
            temp_file.path().to_str().expect("temp path is UTF-8"),
            source_size,
        );
        let reader = Box::new(LocalReader::new(config));
        let buffer = create_sparse_buffer(source_size);

        // Register the seek demand before the fill loop starts (the seek case).
        let mut seek_reader = buffer.new_reader();
        assert!(seek_reader.seek(6 * WINDOW));

        let (progress_tx, _progress_rx) = tokio_mpsc::unbounded_channel();
        reader.start_reading(buffer.clone(), progress_tx);

        let chunk_len = 4096usize;
        let read = tokio::task::spawn_blocking(move || {
            let mut out = vec![0u8; chunk_len];
            let n = seek_reader.read(&mut out);
            drop(seek_reader);
            (n, out)
        });
        let (n, out) = timeout(Duration::from_secs(10), read)
            .await
            .expect("a deep local seek must be served, not hang")
            .expect("seek read task");

        let n = n.expect("seek read returned data");
        let target = 6 * WINDOW as usize;
        assert_eq!(
            &out[..n],
            &data[target..target + n],
            "the bytes at the seek target must match the file at that offset"
        );
    }

    #[tokio::test]
    async fn test_local_file_reader_nonexistent_file() {
        let mut temp_file = NamedTempFile::new().unwrap();
        let test_data = b"deleted before read";
        temp_file.write_all(test_data).unwrap();
        temp_file.flush().unwrap();
        let path = temp_file
            .path()
            .to_str()
            .expect("temp path is UTF-8")
            .to_string();
        drop(temp_file);

        let config = full_file_config(path, test_data.len() as u64);

        let reader = Box::new(LocalReader::new(config));
        let buffer = create_sparse_buffer(test_data.len() as u64);

        let (progress_tx, mut progress_rx) = tokio_mpsc::unbounded_channel();
        reader.start_reading(buffer.clone(), progress_tx);
        let result = drain_async(buffer.clone()).await;

        // Buffer should be cancelled (error case)
        assert!(
            buffer.is_cancelled(),
            "Buffer should be cancelled for nonexistent file"
        );
        assert!(result.is_empty());
        let error = next_playback_error(
            &mut progress_rx,
            "local open failure should emit playback error",
        )
        .await;
        assert!(
            error.contains("Failed to open file"),
            "expected open failure, got: {error}"
        );
    }

    #[tokio::test]
    async fn test_local_file_reader_read_error_reports_error() {
        let dir = tempfile::tempdir().unwrap();
        let config = full_file_config(dir.path().to_str().expect("temp path is UTF-8"), 1);

        let reader = Box::new(LocalReader::new(config));
        let buffer = create_sparse_buffer(1);

        let (progress_tx, mut progress_rx) = tokio_mpsc::unbounded_channel();
        reader.start_reading(buffer.clone(), progress_tx);
        let result = drain_async(buffer.clone()).await;

        assert!(buffer.is_cancelled());
        assert!(result.is_empty());
        let error = next_playback_error(
            &mut progress_rx,
            "local read failure should emit playback error",
        )
        .await;
        assert!(
            error.contains("Read error in file"),
            "expected read failure, got: {error}"
        );
    }
}
