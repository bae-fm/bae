use super::*;
use crate::playback::sparse_buffer::create_sparse_buffer;
use std::io::Write;
use std::sync::Mutex;
use std::time::Duration;
use tempfile::NamedTempFile;
use tokio::sync::mpsc as tokio_mpsc;
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

/// A fill-error handler that forwards the error into a channel, plus the
/// receiving end, so a test can assert on the reported failure.
fn capturing_error_handler() -> (
    FillErrorHandler,
    tokio_mpsc::UnboundedReceiver<PlaybackError>,
) {
    let (tx, rx) = tokio_mpsc::unbounded_channel();
    (
        Box::new(move |error| {
            let _ = tx.send(error);
        }),
        rx,
    )
}

async fn next_fill_error(
    error_rx: &mut tokio_mpsc::UnboundedReceiver<PlaybackError>,
    context: &str,
) -> String {
    error_rx.recv().await.expect(context).to_string()
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

/// Spawn the production `fill_on_demand` loop over an in-memory
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
    let fill_buffer = buffer.clone();
    let log = read_log.clone();
    tokio::spawn(async move {
        // Normal teardown (the buffer cancelled or its weak ref dropped)
        // returns `Ok(())`; only a real fetch failure or short read returns
        // `Err`. So `expect` here fails the test on a genuine fill error
        // without firing on clean teardown.
        fill_buffer
            .fill_on_demand(move |start, len| {
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

/// A reader demanding at or past its read-ahead ceiling must still be
/// served. The ceiling bounds read-ahead within the reader's segment, but
/// raw-FLAC frame parsing needs lookahead past a segment's end (CUE
/// boundaries aren't frame-aligned), so a segment's decoder legitimately
/// blocks reading at its ceiling. A fill that treats the ceiling as a hard
/// stop starves that read forever — the gapless-preload deadlock on
/// single-file CUE albums.
#[tokio::test]
async fn read_at_the_ceiling_is_served_not_starved() {
    let source_size = 4 * WINDOW;
    let blob: Vec<u8> = (0..source_size).map(|i| (i % 251) as u8).collect();

    let buffer = create_sparse_buffer(source_size);
    // A segment reader positioned exactly at its segment's end: ceiling ==
    // pos, the shape a pregap decoder is in while reading the frame that
    // straddles the boundary.
    let mut reader = buffer.new_reader();
    reader.set_readahead_ceiling(WINDOW);
    assert!(reader.seek(WINDOW));

    let _read_log = start_recording_fill(blob, None, &buffer);

    let read_result = timeout(
        Duration::from_secs(5),
        tokio::task::spawn_blocking(move || {
            let mut b = [0u8; 1];
            let n = reader.read(&mut b);
            drop(reader);
            n
        }),
    )
    .await;
    // Unblock the parked reader before asserting, so a starved (failing)
    // run doesn't wedge the runtime's blocking pool on shutdown.
    buffer.cancel();
    let one = read_result
        .expect("a read at the ceiling must be served, not starved")
        .expect("read task");
    assert_eq!(one, Some(1));
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

/// The production fill holds only a `Weak` while parked. Dropping the
/// buffer's last strong ref frees it, and `Drop` wakes the parked fill so
/// its task exits. Removing either release makes this test time out.
#[tokio::test]
async fn fill_follows_the_buffer_and_exits_when_it_is_dropped() {
    let source_size = 4 * WINDOW;
    let blob: Vec<u8> = (0..source_size).map(|i| (i % 251) as u8).collect();
    let buffer = create_sparse_buffer(source_size);
    let weak = Arc::downgrade(&buffer);
    let fill = tokio::spawn(buffer.clone().fill_on_demand(move |start, len| {
        let bytes = blob[start as usize..(start + len) as usize].to_vec();
        async move { Ok(bytes) }
    }));

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
    // parked fill, which upgrades to None and exits.
    drop(buffer);
    assert!(weak.upgrade().is_none(), "the buffer must be freed");
    timeout(Duration::from_secs(5), fill)
        .await
        .expect("the parked fill task must exit when the buffer is dropped, not leak")
        .expect("the fill task must not panic")
        .expect("dropping the buffer is a normal fill exit");
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

    let reader = Box::new(LocalReader::new(
        temp_file.path().to_str().expect("temp path is UTF-8"),
    ));
    let buffer = create_sparse_buffer(test_data.len() as u64);

    reader.start_reading(buffer.clone(), Box::new(|_| {}));
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

    let reader = Box::new(LocalReader::new(
        temp_file.path().to_str().expect("temp path is UTF-8"),
    ));
    let buffer = create_sparse_buffer(source_size);

    // Register the seek demand before the fill loop starts (the seek case).
    let mut seek_reader = buffer.new_reader();
    assert!(seek_reader.seek(6 * WINDOW));

    reader.start_reading(buffer.clone(), Box::new(|_| {}));

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

    let reader = Box::new(LocalReader::new(path));
    let buffer = create_sparse_buffer(test_data.len() as u64);

    let (on_error, mut error_rx) = capturing_error_handler();
    reader.start_reading(buffer.clone(), on_error);
    let result = drain_async(buffer.clone()).await;

    // Buffer should be cancelled (error case)
    assert!(
        buffer.is_cancelled(),
        "Buffer should be cancelled for nonexistent file"
    );
    assert!(result.is_empty());
    let error = next_fill_error(
        &mut error_rx,
        "local open failure should report a fill error",
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

    let reader = Box::new(LocalReader::new(
        dir.path().to_str().expect("temp path is UTF-8"),
    ));
    let buffer = create_sparse_buffer(1);

    let (on_error, mut error_rx) = capturing_error_handler();
    reader.start_reading(buffer.clone(), on_error);
    let result = drain_async(buffer.clone()).await;

    assert!(buffer.is_cancelled());
    assert!(result.is_empty());
    let error = next_fill_error(
        &mut error_rx,
        "local read failure should report a fill error",
    )
    .await;
    // The source is a directory, which the OS refuses to serve as a file: a
    // POSIX open succeeds and the first read fails, while Windows refuses at
    // open. Either way the reader must report a fill error and cancel — the
    // phase the refusal lands in is the OS's, not the contract's.
    assert!(
        error.contains("Failed to read") || error.contains("Failed to open file"),
        "expected a read or open failure, got: {error}"
    );
}
