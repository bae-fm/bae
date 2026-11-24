//! A libvips source that reads a file with plain `read` and `seek` calls.
//! libvips' own file sources map some formats into memory, and a mapped read
//! of a file on a network share stalls on a page fault that nothing can
//! cancel. A source that answers `read` and `seek` is never mapped.

use std::ffi::{c_int, c_void};
use std::fs::File;
use std::io::{ErrorKind, Read, Seek, SeekFrom};
use std::path::Path;
use std::slice;
use std::sync::{Mutex, PoisonError};

use crate::ffi::{self, VipsSource};
use crate::PicturesError;

/// The file behind one source. libvips reads from its own worker threads, one
/// call at a time.
struct Reader {
    file: Mutex<File>,
}

/// A source over one file. Dropping it releases this handle, and libvips frees
/// the file once the images made from the source are gone.
pub(crate) struct FileSource {
    source: *mut VipsSource,
}

impl FileSource {
    #[expect(
        clippy::disallowed_methods,
        reason = "file system: the picture is the person's own file or a catalog download, which no library holds"
    )]
    pub(crate) fn open(path: &Path) -> Result<Self, PicturesError> {
        let file = File::open(path)
            .map_err(|source| PicturesError::Open { path: path.to_path_buf(), source })?;
        crate::init()?;
        let reader = Box::into_raw(Box::new(Reader { file: Mutex::new(file) }));
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
        Ok(Self { source })
    }

    pub(crate) fn as_ptr(&self) -> *mut VipsSource {
        self.source
    }
}

impl Drop for FileSource {
    fn drop(&mut self) {
        // SAFETY: `source` is the reference `open` took.
        unsafe { ffi::g_object_unref(self.source.cast()) };
    }
}

/// Fills `buffer` from the file's current position, and returns the number of
/// bytes read, 0 at the end of the file and -1 on a read error.
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
    // SAFETY: `data` is the Reader that `open` leaked, and libvips passes a
    // buffer of `length` writable bytes.
    let (reader, buffer) = unsafe {
        (&*data.cast::<Reader>(), slice::from_raw_parts_mut(buffer.cast::<u8>(), length))
    };
    let mut file = reader.file.lock().unwrap_or_else(PoisonError::into_inner);
    loop {
        match file.read(buffer) {
            Ok(count) => return i64::try_from(count).unwrap_or(-1),
            Err(error) if error.kind() == ErrorKind::Interrupted => {}
            Err(error) => {
                tracing::warn!(%error, "reading a picture's file failed");
                return -1;
            }
        }
    }
}

/// Moves the file's position as `lseek` does, and returns the new position or
/// -1. `whence` is 0 from the start, 1 from the current position and 2 from
/// the end.
unsafe extern "C" fn seek(
    _source: *mut c_void,
    offset: i64,
    whence: c_int,
    data: *mut c_void,
) -> i64 {
    // SAFETY: `data` is the Reader that `open` leaked.
    let reader = unsafe { &*data.cast::<Reader>() };
    let target = match whence {
        0 => u64::try_from(offset).map(SeekFrom::Start).ok(),
        1 => Some(SeekFrom::Current(offset)),
        2 => Some(SeekFrom::End(offset)),
        _ => None,
    };
    let Some(target) = target else { return -1 };
    let mut file = reader.file.lock().unwrap_or_else(PoisonError::into_inner);
    match file.seek(target) {
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
    // SAFETY: `data` is the Reader that `open` leaked, and this runs once.
    drop(unsafe { Box::from_raw(data.cast::<Reader>()) });
}
