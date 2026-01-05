//! Streaming audio byte buffer for incremental downloads.
//!
//! `StreamingAudioBuffer` is a thread-safe, growable byte buffer that allows:
//! - A producer (downloader) to append bytes as they arrive
//! - A consumer (FFmpeg AVIO) to read bytes with blocking waits
//!
//! This enables streaming playback to start before the full file is downloaded.

use std::sync::{Arc, Condvar, Mutex};

/// Internal buffer state protected by mutex
struct BufferInner {
    /// The actual byte data
    data: Vec<u8>,
    /// Current read position
    read_pos: usize,
    /// Whether EOF has been signaled (no more data coming)
    eof: bool,
    /// Whether the buffer has been cancelled
    cancelled: bool,
}

/// Thread-safe streaming buffer for audio bytes.
///
/// Supports:
/// - `append()`: Producer adds downloaded bytes
/// - `read()`: Consumer reads bytes (blocks if data not yet available)
/// - `seek()`: Consumer seeks to position (for FFmpeg seeking)
/// - `mark_eof()`: Signal that download is complete
/// - `cancel()`: Abort the stream
pub struct StreamingAudioBuffer {
    inner: Mutex<BufferInner>,
    /// Condition variable to wake blocked readers
    data_available: Condvar,
}

impl StreamingAudioBuffer {
    /// Create a new empty streaming buffer.
    pub fn new() -> Self {
        Self {
            inner: Mutex::new(BufferInner {
                data: Vec::new(),
                read_pos: 0,
                eof: false,
                cancelled: false,
            }),
            data_available: Condvar::new(),
        }
    }

    /// Create a new streaming buffer with pre-allocated capacity.
    pub fn with_capacity(capacity: usize) -> Self {
        Self {
            inner: Mutex::new(BufferInner {
                data: Vec::with_capacity(capacity),
                read_pos: 0,
                eof: false,
                cancelled: false,
            }),
            data_available: Condvar::new(),
        }
    }

    /// Append bytes to the buffer (producer side).
    ///
    /// Wakes any blocked readers.
    pub fn append(&self, bytes: &[u8]) {
        let mut inner = self.inner.lock().unwrap();
        inner.data.extend_from_slice(bytes);
        self.data_available.notify_all();
    }

    /// Signal that no more data will be appended (EOF).
    pub fn mark_eof(&self) {
        let mut inner = self.inner.lock().unwrap();
        inner.eof = true;
        self.data_available.notify_all();
    }

    /// Cancel the stream, unblocking any waiting readers.
    pub fn cancel(&self) {
        let mut inner = self.inner.lock().unwrap();
        inner.cancelled = true;
        self.data_available.notify_all();
    }

    /// Read bytes from the buffer (consumer side).
    ///
    /// Blocks until data is available, EOF, or cancelled.
    /// Returns `Some(n)` with bytes read, or `None` if cancelled.
    /// Returns `Some(0)` on EOF with no more data.
    pub fn read(&self, buf: &mut [u8]) -> Option<usize> {
        let mut inner = self.inner.lock().unwrap();

        loop {
            if inner.cancelled {
                return None;
            }

            let available = inner.data.len().saturating_sub(inner.read_pos);

            if available > 0 {
                let to_read = buf.len().min(available);
                buf[..to_read]
                    .copy_from_slice(&inner.data[inner.read_pos..inner.read_pos + to_read]);
                inner.read_pos += to_read;
                return Some(to_read);
            }

            if inner.eof {
                return Some(0); // EOF
            }

            // Wait for more data
            inner = self.data_available.wait(inner).unwrap();
        }
    }

    /// Try to read bytes without blocking.
    ///
    /// Returns the number of bytes read (may be 0 if no data available).
    /// Returns `None` if cancelled.
    pub fn try_read(&self, buf: &mut [u8]) -> Option<usize> {
        let mut inner = self.inner.lock().unwrap();

        if inner.cancelled {
            return None;
        }

        let available = inner.data.len().saturating_sub(inner.read_pos);
        if available == 0 {
            return Some(0);
        }

        let to_read = buf.len().min(available);
        buf[..to_read].copy_from_slice(&inner.data[inner.read_pos..inner.read_pos + to_read]);
        inner.read_pos += to_read;
        Some(to_read)
    }

    /// Seek to a position in the buffer.
    ///
    /// Returns the new position, or `None` if cancelled or position invalid.
    /// Note: Can only seek within already-downloaded data.
    pub fn seek(&self, pos: u64) -> Option<u64> {
        let mut inner = self.inner.lock().unwrap();

        if inner.cancelled {
            return None;
        }

        let pos = pos as usize;
        if pos > inner.data.len() {
            // Can't seek past downloaded data
            return None;
        }

        inner.read_pos = pos;
        Some(pos as u64)
    }

    /// Get current read position.
    pub fn position(&self) -> u64 {
        let inner = self.inner.lock().unwrap();
        inner.read_pos as u64
    }

    /// Get total bytes in buffer.
    pub fn len(&self) -> usize {
        let inner = self.inner.lock().unwrap();
        inner.data.len()
    }

    /// Check if buffer is empty.
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Check if EOF has been signaled.
    pub fn is_eof(&self) -> bool {
        let inner = self.inner.lock().unwrap();
        inner.eof
    }

    /// Check if cancelled.
    pub fn is_cancelled(&self) -> bool {
        let inner = self.inner.lock().unwrap();
        inner.cancelled
    }

    /// Get available bytes (total - read position).
    pub fn available(&self) -> usize {
        let inner = self.inner.lock().unwrap();
        inner.data.len().saturating_sub(inner.read_pos)
    }

    /// Reset the buffer for reuse (e.g., after seek in cloud file).
    pub fn reset(&self) {
        let mut inner = self.inner.lock().unwrap();
        inner.data.clear();
        inner.read_pos = 0;
        inner.eof = false;
        inner.cancelled = false;
    }
}

impl Default for StreamingAudioBuffer {
    fn default() -> Self {
        Self::new()
    }
}

/// Shared streaming buffer wrapped in Arc for multi-threaded use.
pub type SharedStreamingBuffer = Arc<StreamingAudioBuffer>;

/// Create a new shared streaming buffer.
pub fn create_streaming_buffer() -> SharedStreamingBuffer {
    Arc::new(StreamingAudioBuffer::new())
}

/// Create a new shared streaming buffer with pre-allocated capacity.
pub fn create_streaming_buffer_with_capacity(capacity: usize) -> SharedStreamingBuffer {
    Arc::new(StreamingAudioBuffer::with_capacity(capacity))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::thread;
    use std::time::Duration;

    #[test]
    fn test_append_and_read() {
        let buffer = StreamingAudioBuffer::new();

        buffer.append(b"Hello, ");
        buffer.append(b"World!");

        let mut buf = [0u8; 13];
        let n = buffer.read(&mut buf).unwrap();
        assert_eq!(n, 13);
        assert_eq!(&buf, b"Hello, World!");
    }

    #[test]
    fn test_partial_read() {
        let buffer = StreamingAudioBuffer::new();
        buffer.append(b"Hello, World!");

        let mut buf = [0u8; 5];
        let n = buffer.read(&mut buf).unwrap();
        assert_eq!(n, 5);
        assert_eq!(&buf, b"Hello");

        let n = buffer.read(&mut buf).unwrap();
        assert_eq!(n, 5);
        assert_eq!(&buf, b", Wor");
    }

    #[test]
    fn test_eof() {
        let buffer = StreamingAudioBuffer::new();
        buffer.append(b"data");
        buffer.mark_eof();

        let mut buf = [0u8; 10];
        let n = buffer.read(&mut buf).unwrap();
        assert_eq!(n, 4);

        // Next read should return 0 (EOF)
        let n = buffer.read(&mut buf).unwrap();
        assert_eq!(n, 0);
    }

    #[test]
    fn test_cancel() {
        let buffer = StreamingAudioBuffer::new();
        buffer.cancel();

        let mut buf = [0u8; 10];
        let result = buffer.read(&mut buf);
        assert!(result.is_none());
    }

    #[test]
    fn test_seek() {
        let buffer = StreamingAudioBuffer::new();
        buffer.append(b"0123456789");

        // Read first 5 bytes
        let mut buf = [0u8; 5];
        buffer.read(&mut buf).unwrap();
        assert_eq!(&buf, b"01234");

        // Seek back to start
        let pos = buffer.seek(0).unwrap();
        assert_eq!(pos, 0);

        // Read again
        let n = buffer.read(&mut buf).unwrap();
        assert_eq!(n, 5);
        assert_eq!(&buf, b"01234");

        // Seek to middle
        buffer.seek(7).unwrap();
        let n = buffer.read(&mut buf).unwrap();
        assert_eq!(n, 3);
        assert_eq!(&buf[..3], b"789");
    }

    #[test]
    fn test_seek_past_end() {
        let buffer = StreamingAudioBuffer::new();
        buffer.append(b"short");

        let result = buffer.seek(100);
        assert!(result.is_none());
    }

    #[test]
    fn test_blocking_read() {
        let buffer = create_streaming_buffer();
        let buffer_clone = buffer.clone();

        let reader = thread::spawn(move || {
            let mut buf = [0u8; 5];
            let n = buffer_clone.read(&mut buf).unwrap();
            (n, buf)
        });

        // Give reader time to block
        thread::sleep(Duration::from_millis(10));

        // Append data
        buffer.append(b"Hello");

        let (n, buf) = reader.join().unwrap();
        assert_eq!(n, 5);
        assert_eq!(&buf, b"Hello");
    }

    #[test]
    fn test_cancel_unblocks_reader() {
        let buffer = create_streaming_buffer();
        let buffer_clone = buffer.clone();

        let reader = thread::spawn(move || {
            let mut buf = [0u8; 10];
            buffer_clone.read(&mut buf)
        });

        // Give reader time to block
        thread::sleep(Duration::from_millis(10));

        // Cancel
        buffer.cancel();

        let result = reader.join().unwrap();
        assert!(result.is_none());
    }

    #[test]
    fn test_try_read() {
        let buffer = StreamingAudioBuffer::new();

        // Try read on empty buffer
        let mut buf = [0u8; 10];
        let n = buffer.try_read(&mut buf).unwrap();
        assert_eq!(n, 0);

        // Add data and try read
        buffer.append(b"data");
        let n = buffer.try_read(&mut buf).unwrap();
        assert_eq!(n, 4);
        assert_eq!(&buf[..4], b"data");
    }

    #[test]
    fn test_reset() {
        let buffer = StreamingAudioBuffer::new();
        buffer.append(b"old data");
        buffer.mark_eof();

        buffer.reset();

        assert_eq!(buffer.len(), 0);
        assert!(!buffer.is_eof());
        assert_eq!(buffer.position(), 0);
    }
}
