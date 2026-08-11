//! Sparse streaming buffer with range tracking.
//!
//! `SparseStreamingBuffer` holds a file's bytes in possibly non-contiguous
//! ranges, so a seek past the fill position reuses what is already buffered and
//! the gap it opens is filled later.
//!
//! Storage and read cursors are separate: the buffer is shared (via `Arc`) across
//! every track that plays from the same file, and each decoder gets its own
//! `BufferReader` with an independent position. Readers never conflict.

use crate::playback::PlaybackError;
use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Condvar, Mutex};
use std::time::{Duration, Instant};
use tokio::sync::Notify;
use tracing::debug;

pub(crate) const FILL_WINDOW_SIZE: u64 = coven::CHUNK_SIZE as u64 * 64;
pub(crate) const MIN_READAHEAD: u64 = FILL_WINDOW_SIZE;
pub(crate) const KEEP_BEHIND: u64 = FILL_WINDOW_SIZE;

/// Hands each buffer a small process-unique id so the fill's per-window fetch
/// logs and the decoder's first-sample log can be tied to one track's stream.
static NEXT_BUFFER_ID: AtomicU64 = AtomicU64::new(0);
const READ_WAIT_LOG_AFTER: Duration = Duration::from_millis(250);
const READ_WAIT_LOG_EVERY: Duration = Duration::from_secs(1);

/// A contiguous range of buffered data.
#[derive(Debug, Clone)]
struct BufferedRange {
    /// Byte offset in the file where `data` starts.
    start: u64,
    data: Vec<u8>,
}

impl BufferedRange {
    /// Exclusive end offset.
    fn end(&self) -> u64 {
        self.start + self.data.len() as u64
    }

    fn contains(&self, pos: u64) -> bool {
        pos >= self.start && pos < self.end()
    }
}

/// One live reader's demand on the buffer: where it needs bytes next (`pos`,
/// updated on every `read()`/`seek()`) and how far ahead the fill should keep it
/// buffered (`ceiling`, the end byte of the track the reader is decoding -- the
/// whole file for a per-track release, the track's byte range for a single-file
/// album). `ceiling` is `None` until the decoder sets it, or for a non-decoder
/// reader, in which case the fill keeps a fixed minimum ahead of `pos` instead.
/// `demand_windows` hands the fill loop a sorted snapshot of these.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct ReaderDemand {
    pub pos: u64,
    pub ceiling: Option<u64>,
}

/// Update reader `id`'s read position in `demands`, preserving any ceiling
/// already set; registers the reader with no ceiling yet if it's the first time.
/// Takes `&mut demands` rather than `&mut SparseInner` so callers can update a
/// reader's position while holding a shared borrow of another field (the read
/// loop publishes its position mid-iteration over `ranges`).
fn publish_demand_pos(demands: &mut HashMap<u64, ReaderDemand>, id: u64, pos: u64) {
    demands
        .entry(id)
        .and_modify(|d| d.pos = pos)
        .or_insert(ReaderDemand { pos, ceiling: None });
}

fn pick_window_gap(
    buffer: &SparseStreamingBuffer,
    windows: &[ReaderDemand],
    total: u64,
) -> Option<(u64, u64)> {
    for demand in windows {
        let ceil = match demand.ceiling {
            Some(ceiling) if demand.pos < ceiling => ceiling.min(total),
            _ => (demand.pos + MIN_READAHEAD).min(total),
        };
        if let Some(gap) = buffer.next_gap(demand.pos, ceil) {
            return Some(gap);
        }
    }
    None
}

/// Internal state protected by mutex.
struct SparseInner {
    /// Buffered ranges, sorted by start offset, non-overlapping.
    ranges: Vec<BufferedRange>,
    /// Full cancel: stops both the fill (the producer) and the decoders (the
    /// consumers).
    cancelled: bool,
    /// Each live reader's current read position, keyed by reader id. The fill
    /// loop reads this to fetch the window each reader needs next (read-ahead)
    /// and to evict bytes every reader has passed. Per-reader, not a single
    /// scalar: a CUE+APE album runs the playing track's decoder and the
    /// gapless-preload decoder on one shared buffer at different offsets, and a
    /// scalar would let the preload's position overwrite the playing track's.
    /// A reader registers its demand on its first `read()`/`seek()` (or when its
    /// ceiling is set), not at construction, so a freshly created reader that
    /// hasn't positioned itself yet doesn't pin the eviction floor at 0.
    demands: HashMap<u64, ReaderDemand>,
}

/// Thread-safe sparse streaming buffer: data storage only, no fetching.
/// `append_at` adds data at any offset (merging adjacent ranges); `new_reader`
/// mints an independent read cursor over it.
pub struct SparseStreamingBuffer {
    inner: Mutex<SparseInner>,
    /// Total file size, known up front from the source size. Reaching it
    /// (`read_pos >= total_size`) is end-of-file: the decoder gets EOF by
    /// position, so the whole file never has to be buffered to signal the end.
    /// Immutable after construction, so it lives outside the mutex.
    total_size: u64,
    /// Notifies all waiting readers when new data arrives or state changes.
    data_available: Condvar,
    /// Wakes the async fill loop when a reader advances, seeks, drops, the
    /// buffer is cancelled, or the buffer itself is dropped. The fill parks here
    /// once every reader's read-ahead window is full and resumes when there's
    /// more to fetch (or evict) -- this is what keeps the fill following the
    /// playhead instead of marching to EOF. `Arc`, not owned, because the fill
    /// holds only a `Weak` to the buffer and must still be wakeable from the
    /// buffer's `Drop` (the signal that its last real user is gone) so it can
    /// observe the failed upgrade and exit instead of leaking.
    fill_wake: Arc<Notify>,
    /// Hands out a unique id per reader so each reader's demand has its own slot
    /// in `SparseInner::demands`.
    next_reader_id: AtomicU64,
    /// Process-unique id for this buffer, for log correlation (the fill's
    /// per-window fetch logs and the decoder's first-sample log carry it) and as
    /// the [`crate::playback::data_source::FetchArbiter`] foreground designation.
    id: u64,
    /// Total bytes appended into this buffer (every fill window that landed,
    /// including a window re-fetched after a backward seek). Lets the decoder
    /// report how much was fetched from coven to reach the first audio sample.
    bytes_fetched: AtomicU64,
    /// Every byte offset a reader was positioned at when `read()` served bytes,
    /// in call order. Lets a test see the demuxer's actual read pattern -- e.g.
    /// whether a seek jumped near the target (seektable) or read the file's end
    /// and bisected (binary search).
    #[cfg(test)]
    read_log: Mutex<Vec<u64>>,
}

impl SparseStreamingBuffer {
    /// Create a new empty sparse buffer over a file of `total_size` bytes.
    pub fn new(total_size: u64) -> Self {
        Self {
            inner: Mutex::new(SparseInner {
                ranges: Vec::new(),
                cancelled: false,
                demands: HashMap::new(),
            }),
            total_size,
            data_available: Condvar::new(),
            fill_wake: Arc::new(Notify::new()),
            next_reader_id: AtomicU64::new(0),
            id: NEXT_BUFFER_ID.fetch_add(1, Ordering::Relaxed),
            bytes_fetched: AtomicU64::new(0),
            #[cfg(test)]
            read_log: Mutex::new(Vec::new()),
        }
    }

    /// This buffer's process-unique id, used to correlate fetch/first-sample
    /// logs for one track and to designate the foreground track in the
    /// [`crate::playback::data_source::FetchArbiter`].
    pub fn id(&self) -> u64 {
        self.id
    }

    /// Total bytes appended into this buffer so far. Read by the decoder to
    /// report how much was fetched from coven before the first audio sample.
    pub fn bytes_fetched(&self) -> u64 {
        self.bytes_fetched.load(Ordering::Relaxed)
    }

    /// Test-only: the byte offsets `read()` served, in call order.
    #[cfg(test)]
    pub fn read_log(&self) -> Vec<u64> {
        self.read_log.lock().unwrap().clone()
    }

    /// Create a reader with an independent read position over this buffer.
    pub fn new_reader(self: &Arc<Self>) -> BufferReader {
        self.make_reader(None)
    }

    /// Create a reader with an external cancel token.
    pub fn new_reader_with_cancel(
        self: &Arc<Self>,
        cancel_token: Arc<std::sync::atomic::AtomicBool>,
    ) -> BufferReader {
        self.make_reader(Some(cancel_token))
    }

    /// Mint a reader id and build the `BufferReader`. The reader registers its
    /// demand on its first `read()`/`seek()`, not here -- a reader that hasn't
    /// positioned itself yet has no demand, so it neither pulls a fetch toward
    /// byte 0 nor pins the eviction floor there.
    fn make_reader(
        self: &Arc<Self>,
        cancel_token: Option<Arc<std::sync::atomic::AtomicBool>>,
    ) -> BufferReader {
        let id = self.next_reader_id.fetch_add(1, Ordering::Relaxed);
        BufferReader {
            buffer: self.clone(),
            id,
            read_pos: 0,
            cancel_token,
            wait_started: None,
            last_wait_log: None,
        }
    }

    /// Store `bytes` at `offset`, merging with any adjacent or overlapping range.
    pub fn append_at(&self, offset: u64, bytes: &[u8]) {
        if bytes.is_empty() {
            return;
        }

        self.bytes_fetched
            .fetch_add(bytes.len() as u64, Ordering::Relaxed);

        let mut inner = self.inner.lock().unwrap();
        let new_end = offset + bytes.len() as u64;

        // Fast path — sequential streaming lands here: extending the last range in
        // place is O(bytes.len()), not O(buffer_size).
        if let Some(last) = inner.ranges.last_mut() {
            if offset == last.end() {
                last.data.extend_from_slice(bytes);
                self.data_available.notify_all();
                return;
            }
        }

        // Slow path: find the insertion point, and the span of ranges this one
        // overlaps or abuts (which merge into one).
        let mut insert_idx = inner.ranges.len();
        let mut merge_start_idx = None;
        let mut merge_end_idx = None;

        for (i, range) in inner.ranges.iter().enumerate() {
            if insert_idx == inner.ranges.len() && offset <= range.start {
                insert_idx = i;
            }

            if new_end >= range.start && offset <= range.end() {
                if merge_start_idx.is_none() {
                    merge_start_idx = Some(i);
                }
                merge_end_idx = Some(i);
            }
        }

        match (merge_start_idx, merge_end_idx) {
            (Some(start), Some(end)) => {
                let merged_start = inner.ranges[start].start.min(offset);
                let merged_end = inner.ranges[end].end().max(new_end);

                let mut merged_data = vec![0u8; (merged_end - merged_start) as usize];

                for range in &inner.ranges[start..=end] {
                    let dst_offset = (range.start - merged_start) as usize;
                    merged_data[dst_offset..dst_offset + range.data.len()]
                        .copy_from_slice(&range.data);
                }

                // The new bytes go in last, overwriting any overlap.
                let dst_offset = (offset - merged_start) as usize;
                merged_data[dst_offset..dst_offset + bytes.len()].copy_from_slice(bytes);

                inner.ranges.drain(start..=end);
                inner.ranges.insert(
                    start,
                    BufferedRange {
                        start: merged_start,
                        data: merged_data,
                    },
                );
            }
            _ => {
                inner.ranges.insert(
                    insert_idx,
                    BufferedRange {
                        start: offset,
                        data: bytes.to_vec(),
                    },
                );
            }
        }

        self.data_available.notify_all();
    }

    /// Every live reader's current read position, ascending. Test-only -- the
    /// fill loop uses `demand_windows`, which carries each position's ceiling.
    #[cfg(test)]
    pub fn demands_sorted(&self) -> Vec<u64> {
        self.demand_windows().into_iter().map(|d| d.pos).collect()
    }

    /// Every live reader's demand, ascending by position. Walked
    /// lowest-position-first so the most-behind reader (the playing track) is fed
    /// before a further-ahead one (a gapless preload).
    pub fn demand_windows(&self) -> Vec<ReaderDemand> {
        let inner = self.inner.lock().unwrap();
        let mut windows: Vec<ReaderDemand> = inner.demands.values().copied().collect();
        windows.sort_unstable_by_key(|d| d.pos);
        windows
    }

    /// Drop buffered bytes below `pos`, freeing what every reader has already
    /// passed. A range entirely below `pos` is removed; a range straddling `pos`
    /// is trimmed at its front. This is what keeps the buffer bounded to a
    /// window around the live readers instead of accumulating the whole file.
    pub fn evict_before(&self, pos: u64) {
        let mut inner = self.inner.lock().unwrap();
        inner.ranges.retain_mut(|range| {
            if range.end() <= pos {
                false
            } else if range.start < pos {
                let cut = (pos - range.start) as usize;
                range.data.drain(..cut);
                range.start = pos;
                true
            } else {
                true
            }
        });
    }

    /// The first not-yet-buffered sub-range within `[from, limit)`, or `None`
    /// when that whole window is already buffered. The fill loop fetches this
    /// gap; `next_gap(from, limit).is_none()` is also how it decides a reader's
    /// read-ahead window is full and it can idle.
    pub fn next_gap(&self, from: u64, limit: u64) -> Option<(u64, u64)> {
        if from >= limit {
            return None;
        }
        let inner = self.inner.lock().unwrap();
        let mut cursor = from;
        for range in &inner.ranges {
            if range.end() <= cursor {
                continue; // entirely before the cursor
            }
            if range.start > cursor {
                // Gap from the cursor up to this range's start (clamped to limit).
                let gap_end = range.start.min(limit);
                if gap_end > cursor {
                    return Some((cursor, gap_end));
                }
            }
            // This range covers the cursor; advance past it.
            cursor = cursor.max(range.end());
            if cursor >= limit {
                return None;
            }
        }
        Some((cursor, limit))
    }

    #[cfg(test)]
    pub fn is_buffered(&self, pos: u64) -> bool {
        let inner = self.inner.lock().unwrap();
        inner.ranges.iter().any(|r| r.contains(pos))
    }

    /// Contiguous bytes buffered from `pos`; 0 if `pos` itself isn't buffered.
    #[cfg(test)]
    pub fn contiguous_from(&self, pos: u64) -> u64 {
        let inner = self.inner.lock().unwrap();
        for range in &inner.ranges {
            if range.contains(pos) {
                return range.end() - pos;
            }
        }
        0
    }

    /// The file's total size in bytes.
    pub fn get_total_size(&self) -> u64 {
        self.total_size
    }

    /// Wake the async fill loop to re-evaluate: fetch a newly opened read-ahead
    /// gap, evict behind an advanced playhead, observe a cancel, or notice (via
    /// a failed upgrade) that the buffer is gone. Readers call this when they
    /// advance, seek, or drop; the buffer's own `Drop` calls it too. `notify_one`
    /// stores a permit when the fill isn't currently parked, so a wake during the
    /// fill's check isn't lost.
    pub fn wake_fill(&self) {
        self.fill_wake.notify_one();
    }

    /// Fetch on demand until the buffer is cancelled or its last real user is
    /// dropped. The wake signal remains private: this owner clones it for the
    /// loop, then releases its own strong reference before every await.
    pub async fn fill_on_demand<F, Fut>(self: Arc<Self>, fetch: F) -> Result<(), PlaybackError>
    where
        F: Fn(u64, u64) -> Fut,
        Fut: std::future::Future<Output = Result<Vec<u8>, PlaybackError>>,
    {
        let wake = self.fill_wake.clone();
        let weak_buffer = Arc::downgrade(&self);
        drop(self);

        let total = match weak_buffer.upgrade() {
            Some(buffer) => buffer.get_total_size(),
            None => {
                debug!("fill_on_demand: buffer dropped before start, nothing to fill");
                return Ok(());
            }
        };

        loop {
            let Some(buffer) = weak_buffer.upgrade() else {
                debug!("fill_on_demand: buffer dropped, stopping");
                return Ok(());
            };
            if buffer.is_cancelled() {
                debug!("fill_on_demand: buffer cancelled, stopping");
                return Ok(());
            }

            let windows = buffer.demand_windows();
            if let Some(floor) = windows.first() {
                buffer.evict_before(floor.pos.saturating_sub(KEEP_BEHIND));
            }
            let gap = pick_window_gap(&buffer, &windows, total);
            drop(buffer);

            let Some((gap_start, gap_end)) = gap else {
                wake.notified().await;
                continue;
            };

            let window = FILL_WINDOW_SIZE.min(gap_end - gap_start);
            let data = fetch(gap_start, window).await?;
            if data.len() != window as usize {
                return Err(PlaybackError::internal(format!(
                    "Source read returned {} bytes for requested {window} at {gap_start}",
                    data.len()
                )));
            }

            let Some(buffer) = weak_buffer.upgrade() else {
                debug!(
                    "fill_on_demand: buffer dropped during fetch, discarding {window} bytes at {gap_start}"
                );
                return Ok(());
            };
            buffer.append_at(gap_start, &data);
        }
    }

    /// Full cancel, for stopping playback entirely: `read()` returns `None`,
    /// `is_cancelled()` returns `true`, and an active fill sees the flag at its
    /// next loop top and exits (a parked fill exits when the buffer is dropped).
    pub fn cancel(&self) {
        let mut inner = self.inner.lock().unwrap();
        inner.cancelled = true;
        self.data_available.notify_all();
    }

    /// Wake every reader blocked in `read()`, so one whose per-decoder cancel
    /// token was just set observes it on its next loop iteration.
    pub fn wake_readers(&self) {
        self.data_available.notify_all();
    }

    pub fn is_cancelled(&self) -> bool {
        let inner = self.inner.lock().unwrap();
        inner.cancelled
    }

    #[cfg(test)]
    pub fn ranges_count(&self) -> usize {
        let inner = self.inner.lock().unwrap();
        inner.ranges.len()
    }

    #[cfg(test)]
    pub fn total_buffered(&self) -> u64 {
        let inner = self.inner.lock().unwrap();
        inner.ranges.iter().map(|r| r.data.len() as u64).sum()
    }

    /// The buffered ranges as `(start, end)` pairs.
    #[cfg(test)]
    pub fn get_ranges(&self) -> Vec<(u64, u64)> {
        let inner = self.inner.lock().unwrap();
        inner.ranges.iter().map(|r| (r.start, r.end())).collect()
    }

    /// Number of live readers' demands currently registered.
    #[cfg(test)]
    pub fn demand_count(&self) -> usize {
        self.inner.lock().unwrap().demands.len()
    }
}

impl Drop for SparseStreamingBuffer {
    fn drop(&mut self) {
        // The buffer is dropped when its last real user (the decoder's readers
        // and the service's shared-buffer cache) is gone. The fill loop holds
        // only a `Weak`, so it doesn't keep the buffer alive -- but it may be
        // parked on the fill-wake signal. Wake it so it upgrades the `Weak`,
        // finds it gone, and exits instead of leaking a parked task.
        self.wake_fill();
    }
}

/// Shared sparse buffer wrapped in Arc.
pub type SharedSparseBuffer = Arc<SparseStreamingBuffer>;

/// Create a new shared sparse buffer over a file of `total_size` bytes.
pub fn create_sparse_buffer(total_size: u64) -> SharedSparseBuffer {
    Arc::new(SparseStreamingBuffer::new(total_size))
}

/// Independent read cursor over a `SparseStreamingBuffer`. Each decoder gets its
/// own; readers share the data but have their own positions and cancel tokens, so
/// they read the same buffer concurrently without conflict.
pub struct BufferReader {
    buffer: SharedSparseBuffer,
    /// This reader's slot in `SparseInner::demands`. Removed on drop.
    id: u64,
    read_pos: u64,
    /// Checked during `read()`, so the playback service can cancel one decoder's
    /// reader without touching the others on this buffer.
    cancel_token: Option<Arc<std::sync::atomic::AtomicBool>>,
    wait_started: Option<Instant>,
    last_wait_log: Option<Instant>,
}

impl BufferReader {
    /// Move the cursor to `pos`, which need not be buffered — the next `read()`
    /// blocks until the fill delivers it. `false` once the buffer is cancelled.
    pub fn seek(&mut self, pos: u64) -> bool {
        let mut inner = self.buffer.inner.lock().unwrap();
        if inner.cancelled {
            return false;
        }
        // Publish the new position as this reader's demand so the fill loop
        // fetches the seek target's window first, keeping any ceiling already set.
        // Wake the loop in case it was idle (fully buffered up to the old demand).
        publish_demand_pos(&mut inner.demands, self.id, pos);
        drop(inner);
        self.read_pos = pos;
        // A seek can jump into an evicted or never-fetched region, so wake the
        // fill to fetch the target's window. No `data_available` wake: a seek
        // adds no data, so a reader blocked waiting for bytes gains nothing from
        // it (and the seeking reader itself can't be blocked in `read()`).
        self.buffer.wake_fill();
        true
    }

    /// Read from the current position, blocking until bytes are there. `None` if
    /// cancelled, `Some(0)` at EOF.
    pub fn read(&mut self, buf: &mut [u8]) -> Option<usize> {
        let mut inner = self.buffer.inner.lock().unwrap();

        loop {
            let token_cancelled = self
                .cancel_token
                .as_ref()
                .is_some_and(|t| t.load(std::sync::atomic::Ordering::Relaxed));
            if inner.cancelled || token_cancelled {
                if let Some(started) = self.wait_started.take() {
                    let waited = started.elapsed();
                    if waited >= READ_WAIT_LOG_AFTER {
                        debug!(
                            buffer = self.buffer.id(),
                            reader = self.id,
                            pos = self.read_pos,
                            waited_ms = waited.as_millis(),
                            buffer_cancelled = inner.cancelled,
                            token_cancelled,
                            "sparse buffer reader cancelled while waiting for bytes"
                        );
                    }
                    self.last_wait_log = None;
                }
                return None;
            }

            // Publish where this reader needs bytes so the fill loop fetches
            // this position's read-ahead window and evicts behind it.
            let read_pos = self.read_pos;
            publish_demand_pos(&mut inner.demands, self.id, read_pos);

            for range in &inner.ranges {
                if range.contains(read_pos) {
                    if let Some(started) = self.wait_started.take() {
                        let waited = started.elapsed();
                        if waited >= READ_WAIT_LOG_AFTER {
                            debug!(
                                buffer = self.buffer.id(),
                                reader = self.id,
                                pos = read_pos,
                                waited_ms = waited.as_millis(),
                                read_bytes = buf
                                    .len()
                                    .min(range.data.len() - (read_pos - range.start) as usize),
                                bytes_fetched = self.buffer.bytes_fetched(),
                                "sparse buffer reader resumed with bytes"
                            );
                        }
                        self.last_wait_log = None;
                    }
                    let offset_in_range = (read_pos - range.start) as usize;
                    let available = range.data.len() - offset_in_range;
                    let to_read = buf.len().min(available);

                    buf[..to_read]
                        .copy_from_slice(&range.data[offset_in_range..offset_in_range + to_read]);
                    #[cfg(test)]
                    self.buffer.read_log.lock().unwrap().push(read_pos);
                    self.read_pos += to_read as u64;
                    // Advance the demand to where the next read will land so the
                    // loop keeps reading ahead of the playhead, not at it, and
                    // nudge the fill so it refills the read-ahead window we just
                    // drew down.
                    publish_demand_pos(&mut inner.demands, self.id, self.read_pos);
                    self.buffer.wake_fill();
                    return Some(to_read);
                }
            }

            // End of file: the cursor reached the known total size. The size is
            // required at construction, so reaching it is the only EOF; a cursor
            // in a not-yet-fetched gap below total waits for the fill to deliver
            // it.
            if read_pos >= self.buffer.total_size {
                return Some(0);
            }

            // Nothing buffered here yet: the demand published above tells the
            // fill where this reader is stuck; wake it, then wait for the bytes.
            let now = Instant::now();
            let started = *self.wait_started.get_or_insert(now);
            let waited = now.duration_since(started);
            if waited >= READ_WAIT_LOG_AFTER
                && self
                    .last_wait_log
                    .is_none_or(|last| now.duration_since(last) >= READ_WAIT_LOG_EVERY)
            {
                debug!(
                    buffer = self.buffer.id(),
                    reader = self.id,
                    pos = read_pos,
                    waited_ms = waited.as_millis(),
                    ranges = inner.ranges.len(),
                    demands = inner.demands.len(),
                    bytes_fetched = self.buffer.bytes_fetched(),
                    total_size = self.buffer.total_size,
                    "sparse buffer reader waiting for bytes"
                );
                self.last_wait_log = Some(now);
            }
            self.buffer.wake_fill();
            inner = self.buffer.data_available.wait(inner).unwrap();
        }
    }

    pub fn get_read_pos(&self) -> u64 {
        self.read_pos
    }

    pub fn get_total_size(&self) -> u64 {
        self.buffer.get_total_size()
    }

    /// Set how far ahead of itself the fill should keep this reader buffered --
    /// the byte offset of the end of the track it's playing. The decoder calls
    /// this once it knows the track's byte extent, so the fill buffers the rest
    /// of the current track rather than a fixed window. Wakes the fill so the new
    /// ceiling takes effect promptly.
    pub fn set_readahead_ceiling(&self, ceiling: u64) {
        self.buffer
            .inner
            .lock()
            .unwrap()
            .demands
            .entry(self.id)
            .and_modify(|d| d.ceiling = Some(ceiling))
            .or_insert(ReaderDemand {
                pos: self.read_pos,
                ceiling: Some(ceiling),
            });
        self.buffer.wake_fill();
    }
}

impl Drop for BufferReader {
    fn drop(&mut self) {
        // Remove this reader's demand so the fill loop stops fetching a dead
        // reader's read-ahead window, and wake it to re-evaluate -- the eviction
        // floor may rise once this reader is gone (e.g. a finished track's
        // decoder handing off to the gapless-preload reader ahead of it). No
        // `data_available` wake: a dropped reader adds no data, so other readers
        // blocked waiting for bytes gain nothing from it.
        let mut inner = self.buffer.inner.lock().unwrap();
        inner.demands.remove(&self.id);
        drop(inner);
        self.buffer.wake_fill();
    }
}

#[cfg(test)]
#[path = "sparse_buffer_tests.rs"]
mod tests;
