//! Custom FFmpeg AVIO contexts and their callbacks.
//!
//! `StreamingAvioContext` reads from a sparse buffer whose reads block until
//! the bytes arrive, and `WriteAvioContext` accumulates encoded output. Each
//! pairs with the `unsafe extern "C"` read/write/seek callbacks FFmpeg invokes
//! directly.

use crate::playback::SharedSparseBuffer;
use std::io::{Seek, SeekFrom, Write};
use std::os::raw::{c_int, c_void};
use std::sync::Arc;
use tracing::warn;

// --- AVIO custom I/O implementation ---

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

/// The sparse-buffer reader a streaming decode reads from. Its `read` blocks
/// until the fill delivers the bytes.
pub(crate) struct StreamingAvioContext {
    pub(super) reader: std::sync::Mutex<crate::playback::sparse_buffer::BufferReader>,
    pub(super) buffer: SharedSparseBuffer, // kept for total_size queries
    /// Set by the playback service to stop this decoder alone, leaving the others
    /// on the same buffer running.
    pub(super) cancel_token: Arc<std::sync::atomic::AtomicBool>,
}

pub(crate) unsafe extern "C" fn streaming_avio_read_callback(
    opaque: *mut c_void,
    buf: *mut u8,
    buf_size: c_int,
) -> c_int {
    let ctx = &*(opaque as *const StreamingAvioContext);

    if ctx.cancel_token.load(std::sync::atomic::Ordering::Relaxed) {
        return ffmpeg_sys_next::AVERROR_EOF;
    }

    let output = std::slice::from_raw_parts_mut(buf, buf_size as usize);
    match ctx.reader.lock().unwrap().read(output) {
        Some(0) => ffmpeg_sys_next::AVERROR_EOF,
        Some(n) => n as c_int,
        None => {
            // The buffer was cancelled under us: set our own token too, so the
            // decode loop sees the cancellation rather than reading it as EOF.
            ctx.cancel_token
                .store(true, std::sync::atomic::Ordering::Relaxed);
            ffmpeg_sys_next::AVERROR_EOF
        }
    }
}

pub(crate) unsafe extern "C" fn streaming_avio_seek_callback(
    opaque: *mut c_void,
    offset: i64,
    whence: c_int,
) -> i64 {
    let ctx = &*(opaque as *const StreamingAvioContext);

    if whence == ffmpeg_sys_next::AVSEEK_SIZE as c_int {
        return ctx.buffer.get_total_size() as i64;
    }

    let mut reader = ctx.reader.lock().unwrap();
    let new_pos = match whence {
        0 => offset as u64,                                    // SEEK_SET
        1 => (reader.get_read_pos() as i64 + offset) as u64,   // SEEK_CUR
        2 => (reader.get_total_size() as i64 + offset) as u64, // SEEK_END
        _ => return -1,
    };

    if reader.seek(new_pos) {
        new_pos as i64
    } else {
        -1
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

    let seek_from = match whence {
        0 => match u64::try_from(offset) {
            Ok(offset) => SeekFrom::Start(offset),
            Err(e) => {
                warn!("AVIO write seek rejected negative SEEK_SET offset {offset}: {e}");
                return -1;
            }
        },
        1 => SeekFrom::Current(offset),
        2 => SeekFrom::End(offset),
        _ => return -1,
    };

    let pos = match sink.seek(seek_from).and_then(|pos| {
        i64::try_from(pos).map_err(|_| {
            std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                "seek position exceeds i64",
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
