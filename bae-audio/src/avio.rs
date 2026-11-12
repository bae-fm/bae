//! Custom FFmpeg AVIO contexts and their callbacks.
//!
//! `StreamingAvioContext` reads from a sparse buffer whose reads block until
//! the bytes arrive, `FileAvioContext` reads a file Rust opened, and
//! `WriteAvioContext` accumulates encoded output. Each pairs with the
//! `unsafe extern "C"` read/write/seek callbacks FFmpeg invokes directly.

use crate::sparse_buffer::{BufferStop, SharedSparseBuffer, SourceReadError};
use bae_util::task_failure::{caught_sync, Bug};
use std::io::{Read, Seek, SeekFrom, Write};
use std::os::raw::{c_int, c_void};
use std::sync::{Arc, OnceLock};
use tracing::{debug, warn};

// --- AVIO custom I/O implementation ---

/// FFmpeg's seek of `offset` from `whence` (SEEK_SET, SEEK_CUR or SEEK_END) as
/// the seek it asks of a reader or writer. `None`, logged against `what`
/// seeks, for a negative SEEK_SET offset and for any other whence.
fn seek_from(offset: i64, whence: c_int, what: &str) -> Option<SeekFrom> {
    match whence {
        0 => match u64::try_from(offset) {
            Ok(offset) => Some(SeekFrom::Start(offset)),
            Err(e) => {
                warn!("AVIO {what} seek rejected negative SEEK_SET offset {offset}: {e}");
                None
            }
        },
        1 => Some(SeekFrom::Current(offset)),
        2 => Some(SeekFrom::End(offset)),
        _ => None,
    }
}

pub(super) unsafe fn free_custom_avio_context(avio: *mut ffmpeg_sys_next::AVIOContext) {
    let mut avio = avio;
    ffmpeg_sys_next::av_freep(&mut (*avio).buffer as *mut *mut u8 as *mut c_void);
    ffmpeg_sys_next::avio_context_free(&mut avio);
}

pub(super) unsafe fn close_input_and_free_custom_avio(
    fmt_ctx: &mut *mut ffmpeg_sys_next::AVFormatContext,
    avio: *mut ffmpeg_sys_next::AVIOContext,
) {
    ffmpeg_sys_next::avformat_close_input(fmt_ctx);
    free_custom_avio_context(avio);
}

pub(super) unsafe fn free_format_and_custom_avio(
    fmt_ctx: *mut ffmpeg_sys_next::AVFormatContext,
    avio: *mut ffmpeg_sys_next::AVIOContext,
) {
    ffmpeg_sys_next::avformat_free_context(fmt_ctx);
    free_custom_avio_context(avio);
}

// --- Streaming AVIO over a sparse buffer ---

/// Why a streaming read gave FFmpeg an error code instead of bytes.
#[derive(Debug, Clone)]
pub(crate) enum ReadFailure {
    /// The buffer's source could not be read.
    Source(SourceReadError),
    /// The read itself panicked: a bug in bae's read path, which the decode's
    /// caller reports.
    Panicked(Bug),
}

impl From<ReadFailure> for crate::DecodeError {
    fn from(failure: ReadFailure) -> Self {
        match failure {
            ReadFailure::Source(error) => Self::SourceRead(error),
            ReadFailure::Panicked(failed) => Self::ReadPanicked(failed),
        }
    }
}

/// The sparse-buffer reader a streaming decode reads from. Its `read` blocks
/// until the fill delivers the bytes.
pub(crate) struct StreamingAvioContext {
    reader: std::sync::Mutex<crate::sparse_buffer::BufferReader>,
    buffer: SharedSparseBuffer, // kept for total_size queries
    /// Set by the playback service to stop this decoder alone, leaving the others
    /// on the same buffer running.
    cancel_token: Arc<std::sync::atomic::AtomicBool>,
    /// Why a read of this decoder's failed, if one did. FFmpeg only sees an
    /// I/O error code from the read callback, and what it makes of one depends
    /// on where it lands (an empty probe is "Invalid data found", a mid-stream
    /// one ends the packet loop like EOF), so the decode consults this to
    /// report the read's own cause instead.
    read_failure: OnceLock<ReadFailure>,
}

impl StreamingAvioContext {
    pub(super) fn new(
        buffer: SharedSparseBuffer,
        cancel_token: Arc<std::sync::atomic::AtomicBool>,
    ) -> Self {
        let reader = buffer.new_reader_with_cancel(cancel_token.clone());
        Self {
            reader: std::sync::Mutex::new(reader),
            buffer,
            cancel_token,
            read_failure: OnceLock::new(),
        }
    }

    pub(super) fn read_failure(&self) -> Option<ReadFailure> {
        self.read_failure.get().cloned()
    }

    pub(super) fn set_readahead_ceiling(&self, ceiling: u64) {
        match self.reader.lock() {
            Ok(reader) => reader.set_readahead_ceiling(ceiling),
            Err(_) => debug!(
                ceiling,
                "not setting the read-ahead ceiling: a panicked read stopped this decode's reader"
            ),
        }
    }

    /// Keep the first failure: every later read fails the same way, or
    /// follows from it.
    fn record(&self, failure: ReadFailure) {
        #[expect(
            clippy::let_underscore_must_use,
            reason = "the first failure is kept; a later one repeats it or follows from it"
        )]
        let _ = self.read_failure.set(failure);
    }

    /// Fill `output` from the reader: the byte count, or FFmpeg's EOF or I/O
    /// error code.
    fn read(&self, output: &mut [u8]) -> c_int {
        if self.cancel_token.load(std::sync::atomic::Ordering::Relaxed) {
            return ffmpeg_sys_next::AVERROR_EOF;
        }
        // A reader poisoned by a read that panicked: that panic is already
        // recorded as this decode's failure.
        let Ok(mut reader) = self.reader.lock() else {
            return ffmpeg_sys_next::AVERROR(ffmpeg_sys_next::EIO);
        };
        match reader.read(output) {
            Ok(0) => ffmpeg_sys_next::AVERROR_EOF,
            Ok(n) => n as c_int,
            Err(BufferStop::Cancelled) => {
                // The buffer was cancelled under us: set our own token too, so the
                // decode loop sees the cancellation rather than reading it as EOF.
                self.cancel_token.store(true, std::sync::atomic::Ordering::Relaxed);
                ffmpeg_sys_next::AVERROR_EOF
            }
            Err(BufferStop::Failed(error)) => {
                self.record(ReadFailure::Source(error));
                ffmpeg_sys_next::AVERROR(ffmpeg_sys_next::EIO)
            }
        }
    }

    /// Move the reader as FFmpeg's seek asks: the new position, or -1.
    fn seek(&self, offset: i64, whence: c_int) -> i64 {
        if whence == ffmpeg_sys_next::AVSEEK_SIZE as c_int {
            return self.buffer.get_total_size() as i64;
        }

        let Some(seek_from) = seek_from(offset, whence, "stream") else {
            return -1;
        };
        let Ok(mut reader) = self.reader.lock() else {
            return -1;
        };
        let new_pos = match seek_from {
            SeekFrom::Start(position) => Some(position),
            SeekFrom::Current(offset) => reader.get_read_pos().checked_add_signed(offset),
            SeekFrom::End(offset) => reader.get_total_size().checked_add_signed(offset),
        };
        let Some(new_pos) = new_pos else {
            warn!("AVIO stream seek rejected a position before the start: offset {offset}, whence {whence}");
            return -1;
        };

        if reader.seek(new_pos) {
            new_pos as i64
        } else {
            -1
        }
    }
}

/// FFmpeg's read callback. Unwinding out of it would abort the process, so a
/// panicking read is caught here, recorded as the decode's failure, and
/// answered with an I/O error.
pub(crate) unsafe extern "C" fn streaming_avio_read_callback(
    opaque: *mut c_void,
    buf: *mut u8,
    buf_size: c_int,
) -> c_int {
    let ctx = &*(opaque as *const StreamingAvioContext);
    let output = std::slice::from_raw_parts_mut(buf, buf_size as usize);
    caught_sync("read a streaming decode's bytes", || ctx.read(output)).unwrap_or_else(|failed| {
        ctx.record(ReadFailure::Panicked(failed));
        ffmpeg_sys_next::AVERROR(ffmpeg_sys_next::EIO)
    })
}

/// FFmpeg's seek callback, caught as the read callback is.
pub(crate) unsafe extern "C" fn streaming_avio_seek_callback(
    opaque: *mut c_void,
    offset: i64,
    whence: c_int,
) -> i64 {
    let ctx = &*(opaque as *const StreamingAvioContext);
    caught_sync("seek a streaming decode", || ctx.seek(offset, whence)).unwrap_or_else(|failed| {
        ctx.record(ReadFailure::Panicked(failed));
        -1
    })
}

// --- Reading AVIO over a file ---

/// A file FFmpeg reads through the handle Rust opened it with, so FFmpeg
/// opens nothing by name: a name that isn't UTF-8, or one FFmpeg's own file
/// protocol can't spell on Windows, opens like any other.
pub(super) struct FileAvioContext {
    file: std::fs::File,
    /// The first read of the file that failed, if one did. FFmpeg only sees
    /// an I/O error code from the read callback, and what it makes of one
    /// depends on where it lands (a probe that reads nothing is "Invalid data
    /// found"), so the probe consults this to report the read's own cause
    /// instead.
    read_error: Option<std::io::Error>,
}

impl FileAvioContext {
    pub(super) fn new(file: std::fs::File) -> Self {
        Self { file, read_error: None }
    }

    /// The first failed read of the file, taken: `None` when every read
    /// succeeded.
    pub(super) fn take_read_error(&mut self) -> Option<std::io::Error> {
        self.read_error.take()
    }
}

pub(super) unsafe extern "C" fn file_avio_read_callback(
    opaque: *mut c_void,
    buf: *mut u8,
    buf_size: c_int,
) -> c_int {
    let ctx = &mut *(opaque as *mut FileAvioContext);
    let output = std::slice::from_raw_parts_mut(buf, buf_size as usize);
    loop {
        match ctx.file.read(output) {
            Ok(0) => return ffmpeg_sys_next::AVERROR_EOF,
            Ok(n) => return n as c_int,
            Err(e) if e.kind() == std::io::ErrorKind::Interrupted => {}
            Err(e) => {
                warn!("AVIO file read failed: {e}");
                // Keep the first: every later read fails the same way, or
                // follows from it.
                ctx.read_error.get_or_insert(e);
                return ffmpeg_sys_next::AVERROR(ffmpeg_sys_next::EIO);
            }
        }
    }
}

pub(super) unsafe extern "C" fn file_avio_seek_callback(
    opaque: *mut c_void,
    offset: i64,
    whence: c_int,
) -> i64 {
    let ctx = &mut *(opaque as *mut FileAvioContext);
    let position = if whence == ffmpeg_sys_next::AVSEEK_SIZE as c_int {
        ctx.file.metadata().map(|metadata| metadata.len())
    } else {
        let Some(seek_from) = seek_from(offset, whence, "file") else {
            return -1;
        };
        ctx.file.seek(seek_from)
    };
    match position.map(i64::try_from) {
        Ok(Ok(position)) => position,
        Ok(Err(e)) => {
            warn!("AVIO file seek position exceeds i64: {e}");
            -1
        }
        Err(e) => {
            warn!("AVIO file seek failed for offset {offset}, whence {whence}: {e}");
            -1
        }
    }
}

// --- AVIO write context for encoding ---

/// The byte sink an encode's muxer writes into. The two shapes are the
/// encoder's two output types: a seekable sink (a file, an in-memory cursor)
/// lets the muxer seek back and patch its header (FLAC STREAMINFO, MP3
/// Xing/LAME, RIFF/FORM sizes); a streaming sink (a socket) cannot seek, and
/// no seek callback is installed over it, so FFmpeg takes the muxer's
/// streaming path.
pub(super) enum WriteAvioContext {
    Seekable(Box<dyn super::WriteSeek>),
    Streaming(Box<dyn Write + Send>),
}

impl WriteAvioContext {
    fn write_all(&mut self, buf: &[u8]) -> std::io::Result<()> {
        match self {
            Self::Seekable(sink) => sink.write_all(buf),
            Self::Streaming(sink) => sink.write_all(buf),
        }
    }

    pub(super) fn flush(&mut self) -> std::io::Result<()> {
        match self {
            Self::Seekable(sink) => sink.flush(),
            Self::Streaming(sink) => sink.flush(),
        }
    }
}

pub(super) unsafe extern "C" fn avio_write_callback(
    opaque: *mut c_void,
    buf: *const u8,
    buf_size: c_int,
) -> c_int {
    let ctx = &mut *(opaque as *mut WriteAvioContext);
    let size = buf_size as usize;
    let input = std::slice::from_raw_parts(buf, size);

    match ctx.write_all(input) {
        Ok(()) => buf_size,
        Err(e) => {
            warn!("AVIO write callback failed for {size} byte(s): {e}");
            -1
        }
    }
}

/// Lets a muxer seek back to patch its header (FLAC STREAMINFO, MP3 Xing).
/// Installed only over a seekable sink; the streaming arm can't be reached
/// (FFmpeg has no seek callback to call there) and answers -1 if it somehow is.
pub(super) unsafe extern "C" fn avio_write_seek_callback(
    opaque: *mut c_void,
    offset: i64,
    whence: c_int,
) -> i64 {
    let ctx = &mut *(opaque as *mut WriteAvioContext);
    let sink = match ctx {
        WriteAvioContext::Seekable(sink) => sink,
        WriteAvioContext::Streaming(_) => {
            warn!("AVIO seek invoked on a streaming (non-seekable) encode sink");
            return -1;
        }
    };

    // AVSEEK_SIZE asks for the size rather than seeking: report the end
    // position, then restore the cursor.
    if whence == ffmpeg_sys_next::AVSEEK_SIZE as c_int {
        let size = (|| -> std::io::Result<u64> {
            let pos = sink.stream_position()?;
            let end = sink.seek(SeekFrom::End(0))?;
            sink.seek(SeekFrom::Start(pos))?;
            Ok(end)
        })();
        return match size.map(i64::try_from) {
            Ok(Ok(len)) => len,
            Ok(Err(_)) | Err(_) => {
                warn!("AVIO write seek could not report the sink size");
                -1
            }
        };
    }

    let Some(seek_from) = seek_from(offset, whence, "write") else {
        return -1;
    };

    let pos = match sink.seek(seek_from).and_then(|pos| {
        i64::try_from(pos).map_err(|error| {
            std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                format!("seek position exceeds i64: {error}"),
            )
        })
    }) {
        Ok(pos) => pos,
        Err(e) => {
            warn!("AVIO write seek failed for offset {offset}, whence {whence}: {e}");
            return -1;
        }
    };
    pos
}

#[cfg(test)]
#[path = "avio_tests.rs"]
mod tests;
