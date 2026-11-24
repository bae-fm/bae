//! Starting libvips and the handles and checks every libvips call uses.

use std::ffi::{c_int, CStr};
use std::sync::OnceLock;

use crate::{ffi, PicturesError};

/// The threads libvips uses for one copy.
const WORKER_THREADS: c_int = 2;

/// Starts libvips once for the process, and gives every caller the result of
/// that start: libvips reports a failed start only to its first caller.
pub(crate) fn init() -> Result<(), PicturesError> {
    static START: OnceLock<Result<(), String>> = OnceLock::new();
    START
        .get_or_init(|| {
            // SAFETY: argv0 is a valid C string, and this runs once.
            let status = unsafe { ffi::vips_init(c"bae-pictures".as_ptr()) };
            if status == 0 {
                // SAFETY: libvips has started. A copy is background work for
                // a screen that waits on many, so each job uses a few
                // threads and not every core.
                unsafe { ffi::vips_concurrency_set(WORKER_THREADS) };
                Ok(())
            } else {
                Err(take_error())
            }
        })
        .clone()
        .map_err(PicturesError::Vips)
}

/// libvips' description of its last failure, which it then forgets.
pub(crate) fn take_error() -> String {
    // SAFETY: the buffer is a C string that stays valid until the next
    // libvips call on this thread, and this copies it at once.
    unsafe {
        let message = CStr::from_ptr(ffi::vips_error_buffer()).to_string_lossy().trim().to_owned();
        ffi::vips_error_clear();
        message
    }
}

/// A libvips image, released on drop.
pub(crate) struct Image(pub(crate) *mut ffi::VipsImage);

impl Image {
    pub(crate) fn adopt(pointer: *mut ffi::VipsImage) -> Result<Self, PicturesError> {
        if pointer.is_null() {
            Err(PicturesError::Vips(take_error()))
        } else {
            Ok(Self(pointer))
        }
    }
}

impl Drop for Image {
    fn drop(&mut self) {
        // SAFETY: `self.0` is a reference this struct owns.
        unsafe { ffi::g_object_unref(self.0.cast()) };
    }
}

pub(crate) fn check(status: c_int) -> Result<(), PicturesError> {
    if status == 0 {
        Ok(())
    } else {
        Err(PicturesError::Vips(take_error()))
    }
}
