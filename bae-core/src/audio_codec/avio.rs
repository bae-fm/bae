//! Custom FFmpeg AVIO contexts and their callbacks.
//!
//! `AvioContext` reads from an in-memory byte slice, `StreamingAvioContext`
//! reads from a `SparseBuffer` that blocks until bytes arrive, and
//! `WriteAvioContext` accumulates encoded output. Each pairs with the
//! `unsafe extern "C"` read/write/seek callbacks FFmpeg invokes directly.

use crate::playback::SharedSparseBuffer;
use std::os::raw::{c_int, c_void};
use std::ptr;
use std::sync::Arc;

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

/// Context for AVIO callbacks - holds the buffer and read position
pub(super) struct AvioContext {
    pub(super) data: *const u8,
    pub(super) size: usize,
    pub(super) pos: usize,
}

/// AVIO read callback - reads bytes from our memory buffer
pub(super) unsafe extern "C" fn avio_read_callback(
    opaque: *mut c_void,
    buf: *mut u8,
    buf_size: c_int,
) -> c_int {
    let ctx = &mut *(opaque as *mut AvioContext);
    let remaining = ctx.size - ctx.pos;
    let to_read = (buf_size as usize).min(remaining);

    if to_read == 0 {
        return ffmpeg_sys_next::AVERROR_EOF;
    }

    ptr::copy_nonoverlapping(ctx.data.add(ctx.pos), buf, to_read);
    ctx.pos += to_read;
    to_read as c_int
}

/// AVIO seek callback - seeks within our memory buffer
pub(super) unsafe extern "C" fn avio_seek_callback(
    opaque: *mut c_void,
    offset: i64,
    whence: c_int,
) -> i64 {
    let ctx = &mut *(opaque as *mut AvioContext);

    // AVSEEK_SIZE returns the buffer size
    if whence == ffmpeg_sys_next::AVSEEK_SIZE as c_int {
        return ctx.size as i64;
    }

    let new_pos = match whence {
        0 => offset as usize,                     // SEEK_SET
        1 => (ctx.pos as i64 + offset) as usize,  // SEEK_CUR
        2 => (ctx.size as i64 + offset) as usize, // SEEK_END
        _ => return -1,
    };

    if new_pos > ctx.size {
        return -1;
    }

    ctx.pos = new_pos;
    new_pos as i64
}

// --- Streaming AVIO for SparseBuffer ---

/// Context for streaming AVIO - reads from a BufferReader which blocks waiting for data
pub(crate) struct StreamingAvioContext {
    pub(super) reader: std::sync::Mutex<crate::playback::sparse_buffer::BufferReader>,
    pub(super) buffer: SharedSparseBuffer, // kept for total_size queries
    /// Per-decoder cancellation token. Set by the playback service to stop this
    /// specific decoder without affecting other decoders on the same buffer.
    pub(super) cancel_token: Arc<std::sync::atomic::AtomicBool>,
}

/// AVIO read callback for streaming - reads from SparseBuffer, blocking until data available
pub(crate) unsafe extern "C" fn streaming_avio_read_callback(
    opaque: *mut c_void,
    buf: *mut u8,
    buf_size: c_int,
) -> c_int {
    let ctx = &*(opaque as *const StreamingAvioContext);

    if ctx.cancel_token.load(std::sync::atomic::Ordering::Relaxed) {
        return ffmpeg_sys_next::AVERROR_EOF;
    }

    let mut temp_buf = vec![0u8; buf_size as usize];
    match ctx.reader.lock().unwrap().read(&mut temp_buf) {
        Some(0) => ffmpeg_sys_next::AVERROR_EOF,
        Some(n) => {
            ptr::copy_nonoverlapping(temp_buf.as_ptr(), buf, n);
            n as c_int
        }
        None => {
            // Reader cancelled
            ctx.cancel_token
                .store(true, std::sync::atomic::Ordering::Relaxed);
            ffmpeg_sys_next::AVERROR_EOF
        }
    }
}

/// AVIO seek callback for streaming - seeks within SparseBuffer
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

// --- AVIO write context for encoding to memory ---

/// Context for AVIO write callbacks - accumulates encoded data
pub(super) struct WriteAvioContext {
    pub(super) data: Vec<u8>,
    pub(super) pos: usize,
}

/// AVIO write callback - writes bytes to our memory buffer
pub(super) unsafe extern "C" fn avio_write_callback(
    opaque: *mut c_void,
    buf: *const u8,
    buf_size: c_int,
) -> c_int {
    let ctx = &mut *(opaque as *mut WriteAvioContext);
    let size = buf_size as usize;

    // Ensure buffer has enough capacity
    let required_len = ctx.pos + size;
    if required_len > ctx.data.len() {
        ctx.data.resize(required_len, 0);
    }

    ptr::copy_nonoverlapping(buf, ctx.data.as_mut_ptr().add(ctx.pos), size);
    ctx.pos += size;
    buf_size
}

/// AVIO seek callback for writing - allows encoder to seek back for headers
pub(super) unsafe extern "C" fn avio_write_seek_callback(
    opaque: *mut c_void,
    offset: i64,
    whence: c_int,
) -> i64 {
    let ctx = &mut *(opaque as *mut WriteAvioContext);

    // AVSEEK_SIZE returns the buffer size
    if whence == ffmpeg_sys_next::AVSEEK_SIZE as c_int {
        return ctx.data.len() as i64;
    }

    let new_pos = match whence {
        0 => offset as usize,                           // SEEK_SET
        1 => (ctx.pos as i64 + offset) as usize,        // SEEK_CUR
        2 => (ctx.data.len() as i64 + offset) as usize, // SEEK_END
        _ => return -1,
    };

    ctx.pos = new_pos;
    new_pos as i64
}
