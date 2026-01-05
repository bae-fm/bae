//! Streaming audio byte buffer for incremental decoding.
//!
//! This buffer sits between the download layer and the FFmpeg AVIO decoder.
//! - Download thread appends bytes as they arrive
//! - AVIO read callback reads bytes, blocking if necessary
//! - Supports marking EOF when download completes
//! - Supports seeking within buffered data

use std::sync::{Arc, Condvar, Mutex};

/// Streaming buffer for audio bytes.
///
/// Thread-safe: download thread appends, decoder thread reads.
pub struct StreamingAudioBuffer {
    inner: Mutex<BufferInner>,
    /// Condition variable for blocking reads
    data_available: Condvar,
}

struct BufferInner {
    /// Accumulated audio bytes
    data: Vec<u8>,
    /// Current read position
    read_pos: usize,
    /// True when all data has been appended (download complete)
    eof: bool,
    /// True when buffer has been cancelled (e.g., seek or stop)
    cancelled: bool,
}

#[allow(dead_code)] // Methods will be used when PlaybackService streaming is wired up
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

    /// Create a new buffer with pre-allocated capacity.
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

    /// Append bytes to the buffer. Called by download thread.
    pub fn append(&self, bytes: &[u8]) {
        let mut inner = self.inner.lock().unwrap();
        inner.data.extend_from_slice(bytes);
        // Wake up any blocked readers
        self.data_available.notify_all();
    }

    /// Mark that all data has been appended (download complete).
    pub fn mark_eof(&self) {
        let mut inner = self.inner.lock().unwrap();
        inner.eof = true;
        // Wake up any blocked readers so they can see EOF
        self.data_available.notify_all();
    }

    /// Cancel the buffer (e.g., on seek or stop).
    /// Wakes up blocked readers which will return 0/error.
    pub fn cancel(&self) {
        let mut inner = self.inner.lock().unwrap();
        inner.cancelled = true;
        self.data_available.notify_all();
    }

    /// Read bytes into the provided buffer. Called by AVIO read callback.
    ///
    /// - Blocks if no data is available and EOF hasn't been reached
    /// - Returns number of bytes read (0 at EOF)
    /// - Returns None if cancelled
    pub fn read(&self, buf: &mut [u8]) -> Option<usize> {
        let mut inner = self.inner.lock().unwrap();

        loop {
            if inner.cancelled {
                return None;
            }

            let available = inner.data.len() - inner.read_pos;

            if available > 0 {
                // Data available - read it
                let to_read = available.min(buf.len());
                let start = inner.read_pos;
                buf[..to_read].copy_from_slice(&inner.data[start..start + to_read]);
                inner.read_pos += to_read;
                return Some(to_read);
            } else if inner.eof {
                // No more data and EOF reached
                return Some(0);
            } else {
                // Wait for more data
                inner = self.data_available.wait(inner).unwrap();
            }
        }
    }

    /// Try to read without blocking. Returns None if would block.
    pub fn try_read(&self, buf: &mut [u8]) -> Option<usize> {
        let mut inner = self.inner.lock().unwrap();

        if inner.cancelled {
            return None;
        }

        let available = inner.data.len() - inner.read_pos;

        if available > 0 {
            let to_read = available.min(buf.len());
            let start = inner.read_pos;
            buf[..to_read].copy_from_slice(&inner.data[start..start + to_read]);
            inner.read_pos += to_read;
            Some(to_read)
        } else if inner.eof {
            Some(0)
        } else {
            None // Would block
        }
    }

    /// Seek to a position within the buffered data.
    ///
    /// Returns the new position, or None if the position is not yet buffered.
    /// For SEEK_END, pass a negative offset from `total_size`.
    pub fn seek(&self, pos: u64) -> Option<u64> {
        let mut inner = self.inner.lock().unwrap();

        let pos = pos as usize;
        if pos <= inner.data.len() {
            inner.read_pos = pos;
            Some(pos as u64)
        } else {
            // Position not yet buffered - can't seek there yet
            None
        }
    }

    /// Get current read position.
    pub fn position(&self) -> u64 {
        let inner = self.inner.lock().unwrap();
        inner.read_pos as u64
    }

    /// Get total bytes buffered so far.
    pub fn len(&self) -> usize {
        let inner = self.inner.lock().unwrap();
        inner.data.len()
    }

    /// Check if buffer is empty.
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Check if EOF has been marked.
    pub fn is_eof(&self) -> bool {
        let inner = self.inner.lock().unwrap();
        inner.eof
    }

    /// Get number of bytes available to read (without blocking).
    pub fn available(&self) -> usize {
        let inner = self.inner.lock().unwrap();
        inner.data.len() - inner.read_pos
    }

    /// Reset the buffer for reuse (e.g., after seek to new position).
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

/// Thread-safe handle to a streaming buffer.
pub type SharedStreamingBuffer = Arc<StreamingAudioBuffer>;

/// Create a new shared streaming buffer.
pub fn create_streaming_buffer() -> SharedStreamingBuffer {
    Arc::new(StreamingAudioBuffer::new())
}

/// Create a new shared streaming buffer with pre-allocated capacity.
#[allow(dead_code)]
pub fn create_streaming_buffer_with_capacity(capacity: usize) -> SharedStreamingBuffer {
    Arc::new(StreamingAudioBuffer::with_capacity(capacity))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::thread;
    use std::time::Duration;

    #[test]
    fn test_basic_append_read() {
        let buffer = StreamingAudioBuffer::new();

        buffer.append(b"hello");
        buffer.mark_eof();

        let mut buf = [0u8; 10];
        let read = buffer.read(&mut buf).unwrap();
        assert_eq!(read, 5);
        assert_eq!(&buf[..5], b"hello");

        // EOF - should return 0
        let read = buffer.read(&mut buf).unwrap();
        assert_eq!(read, 0);
    }

    #[test]
    fn test_partial_reads() {
        let buffer = StreamingAudioBuffer::new();

        buffer.append(b"hello world");
        buffer.mark_eof();

        let mut buf = [0u8; 5];

        let read = buffer.read(&mut buf).unwrap();
        assert_eq!(read, 5);
        assert_eq!(&buf[..5], b"hello");

        let read = buffer.read(&mut buf).unwrap();
        assert_eq!(read, 5);
        assert_eq!(&buf[..5], b" worl");

        let read = buffer.read(&mut buf).unwrap();
        assert_eq!(read, 1);
        assert_eq!(&buf[..1], b"d");
    }

    #[test]
    fn test_blocking_read() {
        let buffer = Arc::new(StreamingAudioBuffer::new());
        let buffer_clone = buffer.clone();

        // Spawn reader thread that will block
        let reader = thread::spawn(move || {
            let mut buf = [0u8; 10];
            buffer_clone.read(&mut buf).unwrap()
        });

        // Give reader time to block
        thread::sleep(Duration::from_millis(50));

        // Append data - should unblock reader
        buffer.append(b"data");
        buffer.mark_eof();

        let read = reader.join().unwrap();
        assert_eq!(read, 4);
    }

    #[test]
    fn test_seek() {
        let buffer = StreamingAudioBuffer::new();

        buffer.append(b"0123456789");

        // Seek to position 5
        let pos = buffer.seek(5).unwrap();
        assert_eq!(pos, 5);

        let mut buf = [0u8; 5];
        let read = buffer.try_read(&mut buf).unwrap();
        assert_eq!(read, 5);
        assert_eq!(&buf[..5], b"56789");
    }

    #[test]
    fn test_seek_beyond_buffer() {
        let buffer = StreamingAudioBuffer::new();

        buffer.append(b"hello");

        // Seek beyond buffered data - should fail
        let result = buffer.seek(100);
        assert!(result.is_none());
    }

    #[test]
    fn test_cancel() {
        let buffer = Arc::new(StreamingAudioBuffer::new());
        let buffer_clone = buffer.clone();

        // Spawn reader that will block
        let reader = thread::spawn(move || {
            let mut buf = [0u8; 10];
            buffer_clone.read(&mut buf)
        });

        thread::sleep(Duration::from_millis(50));

        // Cancel - should unblock reader with None
        buffer.cancel();

        let result = reader.join().unwrap();
        assert!(result.is_none());
    }

    #[test]
    fn test_try_read_would_block() {
        let buffer = StreamingAudioBuffer::new();

        let mut buf = [0u8; 10];
        let result = buffer.try_read(&mut buf);
        assert!(result.is_none()); // Would block
    }

    #[test]
    fn test_reset() {
        let buffer = StreamingAudioBuffer::new();

        buffer.append(b"old data");
        buffer.mark_eof();

        buffer.reset();

        assert_eq!(buffer.len(), 0);
        assert_eq!(buffer.position(), 0);
        assert!(!buffer.is_eof());
    }
}
