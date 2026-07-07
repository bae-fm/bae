//! Sparse streaming buffer with range tracking.
//!
//! `SparseStreamingBuffer` stores audio bytes in potentially non-contiguous ranges,
//! allowing seeks to reuse already-buffered data even when seeking past the current
//! download position (which creates gaps that are filled later).
//!
//! Data storage and read cursors are decoupled:
//! - The buffer holds data and is shared (via Arc) across tracks from the same file.
//! - Each decoder gets its own `BufferReader` with an independent read position.
//! - Multiple readers can read from the same buffer concurrently without conflict.

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Condvar, Mutex};
use std::time::{Duration, Instant};
use tokio::sync::Notify;
use tracing::debug;

/// Hands each buffer a small process-unique id so the fill's per-window fetch
/// logs and the decoder's first-sample log can be tied to one track's stream.
static NEXT_BUFFER_ID: AtomicU64 = AtomicU64::new(0);
const READ_WAIT_LOG_AFTER: Duration = Duration::from_millis(250);
const READ_WAIT_LOG_EVERY: Duration = Duration::from_secs(1);

/// A contiguous range of buffered data.
#[derive(Debug, Clone)]
struct BufferedRange {
    /// Starting byte offset in the file.
    start: u64,
    /// The actual byte data.
    data: Vec<u8>,
}

impl BufferedRange {
    /// End offset (exclusive).
    fn end(&self) -> u64 {
        self.start + self.data.len() as u64
    }

    /// Check if this range contains the given position.
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

/// Internal state protected by mutex.
struct SparseInner {
    /// Buffered ranges, sorted by start offset, non-overlapping.
    ranges: Vec<BufferedRange>,
    /// Full cancel: stops both readers (data producers) and decoders (data consumers).
    /// Checked by readers via `is_cancelled()` to know when to stop writing.
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

/// Thread-safe sparse streaming buffer (data storage only).
///
/// Supports non-contiguous byte ranges for efficient seeking:
/// - `append_at()`: Add data at any offset, auto-merges adjacent ranges
/// - `new_reader()`: Create an independent read cursor over the data
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

    /// Append data at a specific byte offset.
    ///
    /// Automatically merges with adjacent or overlapping ranges.
    pub fn append_at(&self, offset: u64, bytes: &[u8]) {
        if bytes.is_empty() {
            return;
        }

        self.bytes_fetched
            .fetch_add(bytes.len() as u64, Ordering::Relaxed);

        let mut inner = self.inner.lock().unwrap();
        let new_end = offset + bytes.len() as u64;

        // Fast path: appending at the end of the last range (common case for sequential streaming)
        if let Some(last) = inner.ranges.last_mut() {
            if offset == last.end() {
                // Directly extend the last range - O(bytes.len()) not O(buffer_size)
                last.data.extend_from_slice(bytes);
                self.data_available.notify_all();
                return;
            }
        }

        // Slow path: need to find insertion point and possibly merge
        let mut insert_idx = inner.ranges.len();
        let mut merge_start_idx = None;
        let mut merge_end_idx = None;

        for (i, range) in inner.ranges.iter().enumerate() {
            // Check if new range should come before this one
            if insert_idx == inner.ranges.len() && offset <= range.start {
                insert_idx = i;
            }

            // Check for overlap or adjacency (can merge)
            if new_end >= range.start && offset <= range.end() {
                if merge_start_idx.is_none() {
                    merge_start_idx = Some(i);
                }
                merge_end_idx = Some(i);
            }
        }

        match (merge_start_idx, merge_end_idx) {
            (Some(start), Some(end)) => {
                // Merge with existing ranges
                let merged_start = inner.ranges[start].start.min(offset);
                let merged_end = inner.ranges[end].end().max(new_end);

                // Create new merged data
                let mut merged_data = vec![0u8; (merged_end - merged_start) as usize];

                // Copy existing ranges into merged buffer
                for range in &inner.ranges[start..=end] {
                    let dst_offset = (range.start - merged_start) as usize;
                    merged_data[dst_offset..dst_offset + range.data.len()]
                        .copy_from_slice(&range.data);
                }

                // Copy new data (overwrites any overlap)
                let dst_offset = (offset - merged_start) as usize;
                merged_data[dst_offset..dst_offset + bytes.len()].copy_from_slice(bytes);

                // Replace merged ranges with single range
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
                // No overlap, insert new range
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

    /// Check if a position is within any buffered range.
    #[cfg(test)]
    pub fn is_buffered(&self, pos: u64) -> bool {
        let inner = self.inner.lock().unwrap();
        inner.ranges.iter().any(|r| r.contains(pos))
    }

    /// Get contiguous bytes available from a position.
    ///
    /// Returns 0 if position is not buffered.
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

    /// A handle to the fill-wake signal the fill loop parks on. Handed to the
    /// fill at spawn so it can wait for work while holding only a `Weak` to the
    /// buffer -- the signal outlives the buffer, so the buffer's `Drop` wake
    /// still reaches a parked fill.
    pub fn fill_wake_handle(&self) -> Arc<Notify> {
        self.fill_wake.clone()
    }

    /// Full cancel: stops both readers (data producers) and decoders (data consumers).
    ///
    /// Used when stopping playback entirely. `read()` returns `None` and
    /// `is_cancelled()` returns `true`; an active fill sees the flag at its next
    /// loop top and exits (a parked fill exits when the buffer is dropped).
    pub fn cancel(&self) {
        let mut inner = self.inner.lock().unwrap();
        inner.cancelled = true;
        self.data_available.notify_all();
    }

    /// Clear the full-cancel state so the buffer can be reused.
    pub fn uncancel(&self) {
        let mut inner = self.inner.lock().unwrap();
        inner.cancelled = false;
    }

    /// Wake up all readers blocked in `read()`.
    /// Used when a per-decoder cancel token is set, to unblock readers
    /// so they can check the token on their next loop iteration.
    pub fn wake_readers(&self) {
        self.data_available.notify_all();
    }

    /// Check if fully cancelled (used by data writers to know when to stop).
    pub fn is_cancelled(&self) -> bool {
        let inner = self.inner.lock().unwrap();
        inner.cancelled
    }

    /// Number of separate ranges (for testing).
    #[cfg(test)]
    pub fn ranges_count(&self) -> usize {
        let inner = self.inner.lock().unwrap();
        inner.ranges.len()
    }

    /// Get total bytes buffered across all ranges.
    #[cfg(test)]
    pub fn total_buffered(&self) -> u64 {
        let inner = self.inner.lock().unwrap();
        inner.ranges.iter().map(|r| r.data.len() as u64).sum()
    }

    /// Get buffered byte ranges (for debugging/testing).
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

/// Independent read cursor over a `SparseStreamingBuffer`.
///
/// Each decoder gets its own reader. Multiple readers can read from the same
/// buffer concurrently — they share the data but have independent positions
/// and optional decoder cancel tokens.
pub struct BufferReader {
    buffer: SharedSparseBuffer,
    /// This reader's slot in `SparseInner::demands`. Removed on drop.
    id: u64,
    read_pos: u64,
    /// Optional external cancel token checked during read().
    /// Allows the playback service to cancel a specific reader without
    /// affecting other readers on the same buffer.
    cancel_token: Option<Arc<std::sync::atomic::AtomicBool>>,
    wait_started: Option<Instant>,
    last_wait_log: Option<Instant>,
}

impl BufferReader {
    /// Seek to a position.
    ///
    /// Returns true if successful. Does not require position to be buffered
    /// (read will block until data is available).
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

    /// Blocking read from current position.
    ///
    /// Waits until data is available at current position, then reads.
    /// Returns `None` if cancelled, `Some(0)` on EOF.
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

            // Check if current position is buffered
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

    /// Get current read position.
    pub fn get_read_pos(&self) -> u64 {
        self.read_pos
    }

    /// The file's total size in bytes (delegates to buffer).
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
mod tests {
    use super::*;
    use std::thread;
    use std::time::Duration;

    #[test]
    fn test_append_and_read_single_range() {
        let buffer = create_sparse_buffer(11);
        buffer.append_at(0, b"hello world");

        let mut reader = buffer.new_reader();
        let mut buf = [0u8; 5];
        reader.seek(0);
        assert_eq!(reader.read(&mut buf), Some(5));
        assert_eq!(&buf, b"hello");
    }

    #[test]
    fn test_is_buffered_single_range() {
        let buffer = SparseStreamingBuffer::new(10);
        buffer.append_at(0, b"0123456789");

        assert!(buffer.is_buffered(0));
        assert!(buffer.is_buffered(5));
        assert!(buffer.is_buffered(9));
        assert!(!buffer.is_buffered(10));
    }

    #[test]
    fn test_multiple_non_contiguous_ranges() {
        let buffer = SparseStreamingBuffer::new(104);
        buffer.append_at(0, b"aaaa"); // 0-3
        buffer.append_at(100, b"bbbb"); // 100-103

        assert!(buffer.is_buffered(2));
        assert!(!buffer.is_buffered(50));
        assert!(buffer.is_buffered(101));
        assert_eq!(buffer.ranges_count(), 2);
    }

    #[test]
    fn test_adjacent_ranges_merge() {
        let buffer = create_sparse_buffer(8);
        buffer.append_at(0, b"aaaa"); // 0-3
        buffer.append_at(4, b"bbbb"); // 4-7, should merge with above

        assert_eq!(buffer.ranges_count(), 1);
        assert!(buffer.is_buffered(5));

        // Verify data is correct after merge
        let mut reader = buffer.new_reader();
        reader.seek(0);
        let mut buf = [0u8; 8];
        assert_eq!(reader.read(&mut buf), Some(8));
        assert_eq!(&buf, b"aaaabbbb");
    }

    #[test]
    fn test_overlapping_ranges_merge() {
        let buffer = create_sparse_buffer(10);
        buffer.append_at(0, b"aaaaaa"); // 0-5
        buffer.append_at(4, b"bbbbbb"); // 4-9, overlaps

        assert_eq!(buffer.ranges_count(), 1);

        let mut reader = buffer.new_reader();
        reader.seek(0);
        let mut buf = [0u8; 10];
        assert_eq!(reader.read(&mut buf), Some(10));
        // First 4 bytes are 'a', bytes 4-9 are 'b'
        assert_eq!(&buf, b"aaaabbbbbb");
    }

    #[test]
    fn test_read_blocks_until_data_available() {
        let buffer = create_sparse_buffer(5);
        let buf_clone = buffer.clone();

        let handle = thread::spawn(move || {
            let mut reader = buf_clone.new_reader();
            let mut data = [0u8; 5];
            reader.seek(0);
            reader.read(&mut data)
        });

        thread::sleep(Duration::from_millis(10));
        buffer.append_at(0, b"hello");

        assert_eq!(handle.join().unwrap(), Some(5));
    }

    #[test]
    fn test_contiguous_bytes_from_position() {
        let buffer = SparseStreamingBuffer::new(24);
        buffer.append_at(0, b"0123456789"); // 0-9
        buffer.append_at(20, b"abcd"); // 20-23

        assert_eq!(buffer.contiguous_from(5), 5); // 5 bytes until end of first range
        assert_eq!(buffer.contiguous_from(20), 4); // 4 bytes in second range
        assert_eq!(buffer.contiguous_from(15), 0); // Not buffered
    }

    #[test]
    fn test_seek_and_read_from_different_ranges() {
        let buffer = create_sparse_buffer(106);
        buffer.append_at(0, b"first");
        buffer.append_at(100, b"second");

        let mut reader = buffer.new_reader();

        // Read from first range
        reader.seek(0);
        let mut buf = [0u8; 5];
        assert_eq!(reader.read(&mut buf), Some(5));
        assert_eq!(&buf, b"first");

        // Seek to second range and read
        reader.seek(100);
        let mut buf = [0u8; 6];
        assert_eq!(reader.read(&mut buf), Some(6));
        assert_eq!(&buf, b"second");
    }

    #[test]
    fn test_cancel_unblocks_reader() {
        // Size exceeds the seek target so the reader blocks (not EOF) until cancel.
        let buffer = create_sparse_buffer(100);
        let buf_clone = buffer.clone();

        let handle = thread::spawn(move || {
            let mut reader = buf_clone.new_reader();
            let mut data = [0u8; 5];
            reader.seek(50); // Position not buffered
            reader.read(&mut data)
        });

        thread::sleep(Duration::from_millis(10));
        buffer.cancel();

        assert_eq!(handle.join().unwrap(), None);
    }

    #[test]
    fn test_eof_with_total_size() {
        let buffer = create_sparse_buffer(8);
        buffer.append_at(0, b"all data");

        let mut reader = buffer.new_reader();

        // Read all data
        reader.seek(0);
        let mut buf = [0u8; 20];
        assert_eq!(reader.read(&mut buf), Some(8));

        // Reaching the total size is end-of-file.
        assert_eq!(reader.read(&mut buf), Some(0));
    }

    #[test]
    fn test_merge_three_ranges() {
        let buffer = SparseStreamingBuffer::new(12);
        buffer.append_at(0, b"aa"); // 0-1
        buffer.append_at(10, b"cc"); // 10-11
        assert_eq!(buffer.ranges_count(), 2);

        // Add range that bridges them
        buffer.append_at(2, b"bbbbbbbb"); // 2-9, should merge all into one

        assert_eq!(buffer.ranges_count(), 1);
        assert!(buffer.is_buffered(0));
        assert!(buffer.is_buffered(5));
        assert!(buffer.is_buffered(11));
    }

    #[test]
    fn test_get_ranges() {
        let buffer = SparseStreamingBuffer::new(104);
        buffer.append_at(0, b"aaaa");
        buffer.append_at(100, b"bbbb");

        let ranges = buffer.get_ranges();
        assert_eq!(ranges, vec![(0, 4), (100, 104)]);
    }

    #[test]
    fn test_total_buffered() {
        let buffer = SparseStreamingBuffer::new(106);
        buffer.append_at(0, b"aaaa"); // 4 bytes
        buffer.append_at(100, b"bbbbbb"); // 6 bytes

        assert_eq!(buffer.total_buffered(), 10);
    }

    #[test]
    fn test_multiple_readers_independent_positions() {
        let buffer = create_sparse_buffer(10);
        buffer.append_at(0, b"abcdefghij");

        let mut reader1 = buffer.new_reader();
        let mut reader2 = buffer.new_reader();

        // Reader 1 reads from start
        let mut buf1 = [0u8; 3];
        assert_eq!(reader1.read(&mut buf1), Some(3));
        assert_eq!(&buf1, b"abc");

        // Reader 2 seeks to middle and reads
        reader2.seek(5);
        let mut buf2 = [0u8; 3];
        assert_eq!(reader2.read(&mut buf2), Some(3));
        assert_eq!(&buf2, b"fgh");

        // Reader 1 continues from where it left off
        assert_eq!(reader1.read(&mut buf1), Some(3));
        assert_eq!(&buf1, b"def");
    }

    #[test]
    fn token_cancel_only_affects_one_reader() {
        let buffer = create_sparse_buffer(100);
        let cancel1 = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let cancel2 = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let mut reader2 = buffer.new_reader_with_cancel(cancel2);

        let reader_buffer = buffer.clone();
        let reader_cancel = cancel1.clone();
        let handle = thread::spawn(move || {
            let mut reader1 = reader_buffer.new_reader_with_cancel(reader_cancel);
            let mut buf = [0u8; 5];
            reader1.read(&mut buf)
        });

        thread::sleep(Duration::from_millis(10));
        cancel1.store(true, std::sync::atomic::Ordering::Release);
        buffer.wake_readers();

        assert_eq!(join_within(handle, Duration::from_millis(500)), Some(None));

        buffer.append_at(0, b"hello");
        let mut buf = [0u8; 5];
        assert_eq!(reader2.read(&mut buf), Some(5));
        assert_eq!(&buf, b"hello");
    }

    #[test]
    fn test_seek_returns_false_after_cancel() {
        let buffer = create_sparse_buffer(11);
        buffer.append_at(0, b"hello world");
        let mut reader = buffer.new_reader();

        // A normal seek succeeds and moves the cursor.
        assert!(reader.seek(6));

        // Once the buffer is cancelled the reader is shutting down, so seek is
        // refused rather than silently repositioning a dead cursor.
        buffer.cancel();
        assert!(
            !reader.seek(0),
            "seek must return false once the buffer is cancelled"
        );
    }

    /// Join `handle`, returning its value, or `None` if it hasn't finished within
    /// `timeout` (i.e. it's stuck) — lets a test fail instead of hanging.
    fn join_within<T: Send + 'static>(
        handle: std::thread::JoinHandle<T>,
        timeout: Duration,
    ) -> Option<T> {
        let deadline = std::time::Instant::now() + timeout;
        while std::time::Instant::now() < deadline {
            if handle.is_finished() {
                return handle.join().ok();
            }
            thread::sleep(Duration::from_millis(10));
        }
        None
    }

    #[test]
    fn next_gap_reports_boundaries_within_the_window() {
        let buffer = SparseStreamingBuffer::new(20);
        buffer.append_at(0, b"aaaa"); // [0, 4)
        buffer.append_at(10, b"bbbb"); // [10, 14)

        // Front gap between the cursor and the first range.
        assert_eq!(buffer.next_gap(0, 20), Some((4, 10)));
        // Starting inside the first range skips to the gap after it.
        assert_eq!(buffer.next_gap(2, 20), Some((4, 10)));
        // The window clamps the gap end to `limit`.
        assert_eq!(buffer.next_gap(4, 8), Some((4, 8)));
        // Starting inside the second range, past all data, is the trailing gap.
        assert_eq!(buffer.next_gap(12, 20), Some((14, 20)));
        // A fully buffered window has no gap.
        assert_eq!(buffer.next_gap(0, 4), None);
        assert_eq!(buffer.next_gap(10, 14), None);
        // An empty/inverted window has no gap.
        assert_eq!(buffer.next_gap(5, 5), None);
        assert_eq!(buffer.next_gap(8, 4), None);
    }

    #[test]
    fn next_gap_is_none_when_window_fully_buffered_across_merged_ranges() {
        let buffer = SparseStreamingBuffer::new(12);
        buffer.append_at(0, b"aaaa");
        buffer.append_at(4, b"bbbb"); // merges into [0, 8)
        assert_eq!(buffer.ranges_count(), 1);
        assert_eq!(buffer.next_gap(0, 8), None);
        assert_eq!(buffer.next_gap(2, 6), None);
        assert_eq!(buffer.next_gap(0, 12), Some((8, 12)));
    }

    #[test]
    fn read_publishes_demand_at_the_advanced_position() {
        let buffer = create_sparse_buffer(10);
        buffer.append_at(0, b"0123456789");

        let mut reader = buffer.new_reader();
        // A reader that hasn't read or sought yet has no registered demand, so
        // it doesn't pull a fetch toward byte 0 or pin the eviction floor there.
        assert!(buffer.demands_sorted().is_empty());

        let mut buf = [0u8; 4];
        assert_eq!(reader.read(&mut buf), Some(4));
        // After reading 4 bytes the demand has advanced to where the next read
        // will land, so the fill loop reads ahead of the playhead.
        assert_eq!(buffer.demands_sorted(), vec![4]);
    }

    #[test]
    fn seek_publishes_demand_at_the_target() {
        let buffer = create_sparse_buffer(10);
        buffer.append_at(0, b"0123456789");

        let mut reader = buffer.new_reader();
        assert!(reader.seek(7));
        assert_eq!(
            buffer.demands_sorted(),
            vec![7],
            "seek must publish the target as the reader's demand"
        );
    }

    #[test]
    fn two_readers_keep_independent_demands() {
        // Per-reader demand is the real shape: a CUE+APE album reads the playing
        // track's decoder and the gapless-preload decoder on ONE shared buffer at
        // DIFFERENT offsets. A single scalar (last-writer-wins) could not hold both
        // at once — this test asserts both demands coexist, which is exactly what a
        // scalar cannot express.
        let buffer = create_sparse_buffer(200);
        buffer.append_at(0, &[0u8; 200]);

        let mut playing = buffer.new_reader();
        let mut preload = buffer.new_reader();

        assert!(playing.seek(10));
        assert!(preload.seek(150));

        assert_eq!(
            buffer.demands_sorted(),
            vec![10, 150],
            "both readers' positions must be visible to the fill loop at once"
        );
        assert_eq!(buffer.demand_count(), 2);

        // Dropping the preload reader removes only its demand; the playing
        // track's stays so the fill loop keeps feeding it.
        drop(preload);
        assert_eq!(buffer.demands_sorted(), vec![10]);
        assert_eq!(buffer.demand_count(), 1);

        drop(playing);
        assert!(buffer.demands_sorted().is_empty());
        assert_eq!(buffer.demand_count(), 0);
    }

    #[test]
    fn demand_windows_carry_the_per_reader_ceiling() {
        let buffer = create_sparse_buffer(1000);
        buffer.append_at(0, &[0u8; 500]);

        let mut a = buffer.new_reader();
        let mut b = buffer.new_reader();
        assert!(a.seek(100));
        assert!(b.seek(300));
        a.set_readahead_ceiling(250);
        // `b` has no ceiling set -> None, so the fill falls back to its cushion.

        assert_eq!(
            buffer.demand_windows(),
            vec![
                ReaderDemand {
                    pos: 100,
                    ceiling: Some(250)
                },
                ReaderDemand {
                    pos: 300,
                    ceiling: None
                },
            ],
            "each reader carries its own position and ceiling, ascending by position"
        );

        // Dropping a reader removes both its demand and its ceiling.
        drop(a);
        assert_eq!(
            buffer.demand_windows(),
            vec![ReaderDemand {
                pos: 300,
                ceiling: None
            }]
        );
    }

    #[test]
    fn blocked_reader_publishes_demand_at_its_blocked_position() {
        // A reader that seeks past buffered data and blocks must leave its demand
        // at the blocked position so the fill loop knows where to fetch.
        // Size exceeds the seek target so the reader blocks (not EOF) until data lands.
        let buffer = create_sparse_buffer(1000);
        buffer.append_at(0, b"front");

        let buf = buffer.clone();
        let handle = thread::spawn(move || {
            let mut reader = buf.new_reader();
            assert!(reader.seek(500));
            let mut out = [0u8; 4];
            reader.read(&mut out)
        });

        // Let the reader reach its blocked wait, then inspect its published demand.
        thread::sleep(Duration::from_millis(50));
        assert_eq!(
            buffer.demands_sorted(),
            vec![500],
            "a blocked reader's demand must point at the position it's waiting for"
        );

        // Satisfy the demand and let the reader finish.
        buffer.append_at(500, b"late");
        assert_eq!(join_within(handle, Duration::from_secs(5)), Some(Some(4)));
    }

    #[test]
    fn evict_before_drops_passed_ranges_and_trims_a_straddling_one() {
        let buffer = create_sparse_buffer(24);
        buffer.append_at(0, b"0123456789"); // [0, 10)
        buffer.append_at(20, b"abcd"); // [20, 24)

        // Evicting mid-range trims its front; the later range is untouched.
        buffer.evict_before(5);
        assert_eq!(buffer.get_ranges(), vec![(5, 10), (20, 24)]);

        // The retained bytes are still correct after the front trim.
        let mut reader = buffer.new_reader();
        assert!(reader.seek(5));
        let mut buf = [0u8; 5];
        assert_eq!(reader.read(&mut buf), Some(5));
        assert_eq!(&buf, b"56789");
        drop(reader);

        // Evicting past a whole range drops it; a range starting exactly at the
        // floor is kept.
        buffer.evict_before(20);
        assert_eq!(buffer.get_ranges(), vec![(20, 24)]);
        buffer.evict_before(20);
        assert_eq!(buffer.get_ranges(), vec![(20, 24)]);
    }

    /// Appending a range that sits entirely before an existing one (a backward
    /// seek fetched an earlier window) inserts it ahead in the sorted range list
    /// rather than at the end. Both ranges stay independently readable.
    #[test]
    fn append_before_an_existing_range_inserts_in_sorted_order() {
        let buffer = create_sparse_buffer(104);
        buffer.append_at(100, b"bbbb"); // [100, 104)
        buffer.append_at(0, b"aaaa"); // [0, 4) — disjoint, belongs before the first

        assert_eq!(buffer.get_ranges(), vec![(0, 4), (100, 104)]);
        assert_eq!(buffer.ranges_count(), 2);

        let mut reader = buffer.new_reader();
        let mut buf = [0u8; 4];
        reader.seek(0);
        assert_eq!(reader.read(&mut buf), Some(4));
        assert_eq!(&buf, b"aaaa");
        reader.seek(100);
        assert_eq!(reader.read(&mut buf), Some(4));
        assert_eq!(&buf, b"bbbb");
    }

    /// `uncancel` clears the full-cancel flag so the buffer can be reused: a read
    /// that returned `None` while cancelled serves bytes again afterward.
    #[test]
    fn uncancel_clears_cancel_so_the_buffer_reads_again() {
        let buffer = create_sparse_buffer(5);
        buffer.append_at(0, b"hello");

        buffer.cancel();
        assert!(buffer.is_cancelled());
        let mut reader = buffer.new_reader();
        let mut buf = [0u8; 5];
        assert_eq!(reader.read(&mut buf), None, "a cancelled buffer reads None");

        buffer.uncancel();
        assert!(!buffer.is_cancelled());
        let mut reader = buffer.new_reader();
        reader.seek(0);
        assert_eq!(reader.read(&mut buf), Some(5));
        assert_eq!(&buf, b"hello");
    }

    /// An empty append is a no-op: no range is created and no bytes are counted
    /// as fetched, so it neither perturbs the range list nor the fetch tally.
    #[test]
    fn empty_append_is_a_noop() {
        let buffer = create_sparse_buffer(10);
        buffer.append_at(0, b"");
        assert_eq!(buffer.ranges_count(), 0);
        assert_eq!(buffer.bytes_fetched(), 0);

        // A subsequent real append still lands and is counted.
        buffer.append_at(0, b"data");
        assert_eq!(buffer.get_ranges(), vec![(0, 4)]);
        assert_eq!(buffer.bytes_fetched(), 4);
    }
}
