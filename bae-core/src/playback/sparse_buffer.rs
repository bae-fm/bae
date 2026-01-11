//! Sparse streaming buffer with range tracking.
//!
//! `SparseStreamingBuffer` stores audio bytes in potentially non-contiguous ranges,
//! allowing seeks to reuse already-buffered data even when seeking past the current
//! download position (which creates gaps that are filled later).

use std::sync::{Arc, Condvar, Mutex};

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

/// Internal state protected by mutex.
struct SparseInner {
    /// Buffered ranges, sorted by start offset, non-overlapping.
    ranges: Vec<BufferedRange>,
    /// Current read position.
    read_pos: u64,
    /// Total file size if known.
    total_size: Option<u64>,
    /// Whether all data has been received.
    eof: bool,
    /// Whether the buffer has been cancelled.
    cancelled: bool,
}

/// Thread-safe sparse streaming buffer.
///
/// Supports non-contiguous byte ranges for efficient seeking:
/// - `append_at()`: Add data at any offset, auto-merges adjacent ranges
/// - `is_buffered()`: Check if a position is available
/// - `seek()`: Move read position
/// - `read()`: Blocking read from current position
pub struct SparseStreamingBuffer {
    inner: Mutex<SparseInner>,
    data_available: Condvar,
}

impl SparseStreamingBuffer {
    /// Create a new empty sparse buffer.
    pub fn new() -> Self {
        Self {
            inner: Mutex::new(SparseInner {
                ranges: Vec::new(),
                read_pos: 0,
                total_size: None,
                eof: false,
                cancelled: false,
            }),
            data_available: Condvar::new(),
        }
    }

    /// Append data at a specific byte offset.
    ///
    /// Automatically merges with adjacent or overlapping ranges.
    pub fn append_at(&self, offset: u64, bytes: &[u8]) {
        if bytes.is_empty() {
            return;
        }

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

    /// Seek to a position.
    ///
    /// Returns true if successful. Does not require position to be buffered
    /// (read will block until data is available).
    pub fn seek(&self, pos: u64) -> bool {
        let mut inner = self.inner.lock().unwrap();
        if inner.cancelled {
            return false;
        }
        inner.read_pos = pos;
        true
    }

    /// Blocking read from current position.
    ///
    /// Waits until data is available at current position, then reads.
    /// Returns `None` if cancelled, `Some(0)` on EOF.
    pub fn read(&self, buf: &mut [u8]) -> Option<usize> {
        let mut inner = self.inner.lock().unwrap();

        loop {
            if inner.cancelled {
                return None;
            }

            // Check if current position is buffered
            let read_pos = inner.read_pos;
            for range in &inner.ranges {
                if range.contains(read_pos) {
                    let offset_in_range = (read_pos - range.start) as usize;
                    let available = range.data.len() - offset_in_range;
                    let to_read = buf.len().min(available);

                    buf[..to_read]
                        .copy_from_slice(&range.data[offset_in_range..offset_in_range + to_read]);
                    inner.read_pos += to_read as u64;
                    return Some(to_read);
                }
            }

            // Check for EOF
            if inner.eof {
                if let Some(total) = inner.total_size {
                    if read_pos >= total {
                        return Some(0);
                    }
                } else {
                    // EOF but position not buffered - might be a gap
                    // Check if we're past all data
                    let max_end = inner.ranges.iter().map(|r| r.end()).max().unwrap_or(0);
                    if read_pos >= max_end {
                        return Some(0);
                    }
                }
            }

            // Wait for more data
            inner = self.data_available.wait(inner).unwrap();
        }
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

    /// Set total file size (enables proper EOF detection).
    pub fn set_total_size(&self, size: u64) {
        let mut inner = self.inner.lock().unwrap();
        inner.total_size = Some(size);
    }

    /// Mark as complete (all data received).
    pub fn mark_eof(&self) {
        let mut inner = self.inner.lock().unwrap();
        inner.eof = true;
        self.data_available.notify_all();
    }

    /// Cancel the buffer (unblocks readers).
    pub fn cancel(&self) {
        let mut inner = self.inner.lock().unwrap();
        inner.cancelled = true;
        self.data_available.notify_all();
    }

    /// Check if cancelled.
    pub fn is_cancelled(&self) -> bool {
        let inner = self.inner.lock().unwrap();
        inner.cancelled
    }

    /// Get buffered byte ranges (for debugging/testing).
    #[cfg(test)]
    pub fn get_ranges(&self) -> Vec<(u64, u64)> {
        let inner = self.inner.lock().unwrap();
        inner.ranges.iter().map(|r| (r.start, r.end())).collect()
    }
}

impl Default for SparseStreamingBuffer {
    fn default() -> Self {
        Self::new()
    }
}

/// Shared sparse buffer wrapped in Arc.
pub type SharedSparseBuffer = Arc<SparseStreamingBuffer>;

/// Create a new shared sparse buffer.
pub fn create_sparse_buffer() -> SharedSparseBuffer {
    Arc::new(SparseStreamingBuffer::new())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::thread;
    use std::time::Duration;

    #[test]
    fn test_append_and_read_single_range() {
        let buffer = SparseStreamingBuffer::new();
        buffer.append_at(0, b"hello world");

        let mut buf = [0u8; 5];
        buffer.seek(0);
        assert_eq!(buffer.read(&mut buf), Some(5));
        assert_eq!(&buf, b"hello");
    }

    #[test]
    fn test_is_buffered_single_range() {
        let buffer = SparseStreamingBuffer::new();
        buffer.append_at(0, b"0123456789");

        assert!(buffer.is_buffered(0));
        assert!(buffer.is_buffered(5));
        assert!(buffer.is_buffered(9));
        assert!(!buffer.is_buffered(10));
    }

    #[test]
    fn test_multiple_non_contiguous_ranges() {
        let buffer = SparseStreamingBuffer::new();
        buffer.append_at(0, b"aaaa"); // 0-3
        buffer.append_at(100, b"bbbb"); // 100-103

        assert!(buffer.is_buffered(2));
        assert!(!buffer.is_buffered(50));
        assert!(buffer.is_buffered(101));
        assert_eq!(buffer.ranges_count(), 2);
    }

    #[test]
    fn test_adjacent_ranges_merge() {
        let buffer = SparseStreamingBuffer::new();
        buffer.append_at(0, b"aaaa"); // 0-3
        buffer.append_at(4, b"bbbb"); // 4-7, should merge with above

        assert_eq!(buffer.ranges_count(), 1);
        assert!(buffer.is_buffered(5));

        // Verify data is correct after merge
        buffer.seek(0);
        let mut buf = [0u8; 8];
        assert_eq!(buffer.read(&mut buf), Some(8));
        assert_eq!(&buf, b"aaaabbbb");
    }

    #[test]
    fn test_overlapping_ranges_merge() {
        let buffer = SparseStreamingBuffer::new();
        buffer.append_at(0, b"aaaaaa"); // 0-5
        buffer.append_at(4, b"bbbbbb"); // 4-9, overlaps

        assert_eq!(buffer.ranges_count(), 1);

        buffer.seek(0);
        let mut buf = [0u8; 10];
        assert_eq!(buffer.read(&mut buf), Some(10));
        // First 4 bytes are 'a', bytes 4-9 are 'b'
        assert_eq!(&buf, b"aaaabbbbbb");
    }

    #[test]
    fn test_read_blocks_until_data_available() {
        let buffer = Arc::new(SparseStreamingBuffer::new());
        let buf_clone = buffer.clone();

        let reader = thread::spawn(move || {
            let mut data = [0u8; 5];
            buf_clone.seek(0);
            buf_clone.read(&mut data)
        });

        thread::sleep(Duration::from_millis(10));
        buffer.append_at(0, b"hello");

        assert_eq!(reader.join().unwrap(), Some(5));
    }

    #[test]
    fn test_contiguous_bytes_from_position() {
        let buffer = SparseStreamingBuffer::new();
        buffer.append_at(0, b"0123456789"); // 0-9
        buffer.append_at(20, b"abcd"); // 20-23

        assert_eq!(buffer.contiguous_from(5), 5); // 5 bytes until end of first range
        assert_eq!(buffer.contiguous_from(20), 4); // 4 bytes in second range
        assert_eq!(buffer.contiguous_from(15), 0); // Not buffered
    }

    #[test]
    fn test_seek_and_read_from_different_ranges() {
        let buffer = SparseStreamingBuffer::new();
        buffer.append_at(0, b"first");
        buffer.append_at(100, b"second");

        // Read from first range
        buffer.seek(0);
        let mut buf = [0u8; 5];
        assert_eq!(buffer.read(&mut buf), Some(5));
        assert_eq!(&buf, b"first");

        // Seek to second range and read
        buffer.seek(100);
        let mut buf = [0u8; 6];
        assert_eq!(buffer.read(&mut buf), Some(6));
        assert_eq!(&buf, b"second");
    }

    #[test]
    fn test_cancel_unblocks_reader() {
        let buffer = Arc::new(SparseStreamingBuffer::new());
        let buf_clone = buffer.clone();

        let reader = thread::spawn(move || {
            let mut data = [0u8; 5];
            buf_clone.seek(50); // Position not buffered
            buf_clone.read(&mut data)
        });

        thread::sleep(Duration::from_millis(10));
        buffer.cancel();

        assert_eq!(reader.join().unwrap(), None);
    }

    #[test]
    fn test_eof_with_total_size() {
        let buffer = SparseStreamingBuffer::new();
        buffer.append_at(0, b"all data");
        buffer.set_total_size(8);
        buffer.mark_eof();

        // Read all data
        buffer.seek(0);
        let mut buf = [0u8; 20];
        assert_eq!(buffer.read(&mut buf), Some(8));

        // Next read should return EOF
        assert_eq!(buffer.read(&mut buf), Some(0));
    }

    #[test]
    fn test_merge_three_ranges() {
        let buffer = SparseStreamingBuffer::new();
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
        let buffer = SparseStreamingBuffer::new();
        buffer.append_at(0, b"aaaa");
        buffer.append_at(100, b"bbbb");

        let ranges = buffer.get_ranges();
        assert_eq!(ranges, vec![(0, 4), (100, 104)]);
    }

    #[test]
    fn test_total_buffered() {
        let buffer = SparseStreamingBuffer::new();
        buffer.append_at(0, b"aaaa"); // 4 bytes
        buffer.append_at(100, b"bbbbbb"); // 6 bytes

        assert_eq!(buffer.total_buffered(), 10);
    }
}
