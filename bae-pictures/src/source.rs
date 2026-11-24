//! A libvips source that reads its picture with plain `read` and `seek` calls.
//! libvips' own file sources map some formats into memory, and a mapped read
//! of a file on a network share stalls on a page fault that nothing can
//! cancel. A source that answers `read` and `seek` is never mapped, and the
//! `read` is where a job notices that nobody wants its picture any more.

use std::ffi::{c_int, c_void};
use std::io::{ErrorKind, SeekFrom};
use std::slice;
use std::sync::{Mutex, PoisonError};

use crate::ffi::{self, VipsSource};
use crate::{IsCancelled, PictureOrigin, PicturesError};

/// The origin behind one source. libvips reads from its own worker threads,
/// one call at a time.
struct Reader {
    origin: Mutex<Box<dyn PictureOrigin>>,
    cancelled: IsCancelled,
    /// Why the first read that failed did, which libvips alone cannot say: a
    /// failed read ends a JPEG as a truncated picture and a PNG as a warning,
    /// so a save from the source can succeed over a picture that stops early.
    failure: Mutex<Option<PicturesError>>,
}

impl Reader {
    /// Keep why a read failed, unless an earlier read's failure is kept.
    fn fail(&self, error: PicturesError) {
        self.failure.lock().unwrap_or_else(PoisonError::into_inner).get_or_insert(error);
    }
}

/// A source over one origin. Dropping it releases this handle, and libvips
/// frees the origin once the images made from the source are gone.
pub(crate) struct OriginSource {
    source: *mut VipsSource,
    /// The source's reader, which lives while this handle does.
    reader: *const Reader,
}

impl OriginSource {
    pub(crate) fn new(
        origin: impl PictureOrigin,
        cancelled: IsCancelled,
    ) -> Result<Self, PicturesError> {
        crate::vips::init()?;
        let reader = Box::into_raw(Box::new(Reader {
            origin: Mutex::new(Box::new(origin)),
            cancelled,
            failure: Mutex::new(None),
        }));
        // SAFETY: the signal handlers match the argument lists of
        // VipsSourceCustom's `read` and `seek` signals, and `reader` lives
        // until libvips calls `free_reader` as the source is finalized.
        let source = unsafe {
            let source = ffi::vips_source_custom_new();
            ffi::g_signal_connect_data(
                source.cast(),
                c"read".as_ptr(),
                std::mem::transmute::<
                    unsafe extern "C" fn(*mut c_void, *mut c_void, i64, *mut c_void) -> i64,
                    unsafe extern "C" fn(),
                >(read),
                reader.cast(),
                None,
                0,
            );
            ffi::g_signal_connect_data(
                source.cast(),
                c"seek".as_ptr(),
                std::mem::transmute::<
                    unsafe extern "C" fn(*mut c_void, i64, c_int, *mut c_void) -> i64,
                    unsafe extern "C" fn(),
                >(seek),
                reader.cast(),
                Some(free_reader),
                0,
            );
            source
        };
        Ok(Self { source, reader })
    }

    /// Why a read of the picture failed, if one did: a save from this source
    /// that returned Ok over a failed read holds a picture that stops early.
    pub(crate) fn take_failure(&self) -> Option<PicturesError> {
        // SAFETY: `reader` lives while libvips holds the source, which this
        // handle does until it is dropped.
        let reader = unsafe { &*self.reader };
        reader.failure.lock().unwrap_or_else(PoisonError::into_inner).take()
    }

    /// What libvips' `error` came from: the failed read of the picture when
    /// one failed, since libvips reports only that it could not read.
    pub(crate) fn blame_read(&self, error: PicturesError) -> PicturesError {
        self.take_failure().unwrap_or(error)
    }

    pub(crate) fn as_ptr(&self) -> *mut VipsSource {
        self.source
    }
}

impl Drop for OriginSource {
    fn drop(&mut self) {
        // SAFETY: `source` is the reference `new` took.
        unsafe { ffi::g_object_unref(self.source.cast()) };
    }
}

/// Fills `buffer` from the origin's current position, and returns the number
/// of bytes read, 0 at the end and -1 on a read error or a cancelled job.
unsafe extern "C" fn read(
    _source: *mut c_void,
    buffer: *mut c_void,
    length: i64,
    data: *mut c_void,
) -> i64 {
    let Ok(length) = usize::try_from(length) else {
        tracing::error!(length, "libvips asked for a read of negative length");
        return -1;
    };
    // SAFETY: `data` is the Reader that `new` leaked, and libvips passes a
    // buffer of `length` writable bytes.
    let (reader, buffer) = unsafe {
        (&*data.cast::<Reader>(), slice::from_raw_parts_mut(buffer.cast::<u8>(), length))
    };
    if (reader.cancelled)() {
        reader.fail(PicturesError::Cancelled);
        return -1;
    }
    let mut origin = reader.origin.lock().unwrap_or_else(PoisonError::into_inner);
    loop {
        match origin.read(buffer) {
            Ok(count) => return i64::try_from(count).unwrap_or(-1),
            Err(error) if error.kind() == ErrorKind::Interrupted => {}
            Err(error) => {
                tracing::warn!(%error, "reading a picture's file failed");
                reader.fail(PicturesError::Read(error));
                return -1;
            }
        }
    }
}

/// Moves the origin's position as `lseek` does, and returns the new position
/// or -1. `whence` is 0 from the start, 1 from the current position and 2
/// from the end.
unsafe extern "C" fn seek(
    _source: *mut c_void,
    offset: i64,
    whence: c_int,
    data: *mut c_void,
) -> i64 {
    // SAFETY: `data` is the Reader that `new` leaked.
    let reader = unsafe { &*data.cast::<Reader>() };
    let target = match whence {
        0 => u64::try_from(offset).map(SeekFrom::Start).ok(),
        1 => Some(SeekFrom::Current(offset)),
        2 => Some(SeekFrom::End(offset)),
        _ => None,
    };
    let Some(target) = target else { return -1 };
    let mut origin = reader.origin.lock().unwrap_or_else(PoisonError::into_inner);
    match origin.seek(target) {
        Ok(position) => i64::try_from(position).unwrap_or(-1),
        Err(error) => {
            tracing::warn!(%error, "seeking in a picture's file failed");
            -1
        }
    }
}

/// Frees the Reader once libvips finalizes the source and its two signal
/// handlers are gone.
unsafe extern "C" fn free_reader(data: *mut c_void, _closure: *mut c_void) {
    // SAFETY: `data` is the Reader that `new` leaked, and this runs once.
    drop(unsafe { Box::from_raw(data.cast::<Reader>()) });
}
