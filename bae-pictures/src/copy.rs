//! Making a copy of a picture at a step: libvips reads the original once,
//! top to bottom, shrinking as the rows arrive, turns it upright by its
//! orientation flag and converts its colours to sRGB.

use crate::{IsCancelled, PictureOrigin, PictureStep, PicturesError};

/// How a copy is encoded: JPEG, or PNG for a picture with transparency.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CopyFormat {
    Jpeg,
    Png,
}

/// A picture's copy at one step.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PictureCopy {
    pub format: CopyFormat,
    pub bytes: Vec<u8>,
}

/// The copy of the picture `origin` holds at `step`: it fits a square of the
/// step's edge, and is never larger than the picture. A copy of a picture
/// with transparency is a PNG, any other a JPEG. `origin` is read in chunks,
/// and `cancelled` is asked before each, so a cancelled copy ends with
/// [`PicturesError::Cancelled`] at the next one.
#[cfg(libvips)]
pub fn make_copy(
    origin: impl PictureOrigin,
    cancelled: IsCancelled,
    step: PictureStep,
) -> Result<PictureCopy, PicturesError> {
    let result = libvips::copy_of(origin, cancelled.clone(), step);
    match result {
        Err(_) if cancelled() => Err(PicturesError::Cancelled),
        other => other,
    }
}

/// Without libvips there is no copy to make: see `build.rs`.
#[cfg(not(libvips))]
pub fn make_copy(
    _origin: impl PictureOrigin,
    _cancelled: IsCancelled,
    _step: PictureStep,
) -> Result<PictureCopy, PicturesError> {
    Err(PicturesError::NoLibvips)
}

#[cfg(libvips)]
mod libvips {
    use std::ffi::{c_char, c_int, c_void};
    use std::ptr;
    use std::slice;

    use super::{CopyFormat, PictureCopy};
    use crate::source::OriginSource;
    use crate::vips::{check, Image};
    use crate::{ffi, IsCancelled, PictureOrigin, PictureStep, PicturesError};

    /// The JPEG quality copies are saved at.
    const JPEG_QUALITY: c_int = 85;

    pub(super) fn copy_of(
        origin: impl PictureOrigin,
        cancelled: IsCancelled,
        step: PictureStep,
    ) -> Result<PictureCopy, PicturesError> {
        let edge = c_int::from(step.edge());
        let source = OriginSource::new(origin, cancelled)?;
        let mut image = ptr::null_mut();
        // SAFETY: the options are name and value pairs that end in a null pointer,
        // and `source` outlives the call.
        let status = unsafe {
            ffi::vips_thumbnail_source(
                source.as_ptr(),
                &mut image,
                edge,
                c"height".as_ptr(),
                edge,
                c"size".as_ptr(),
                ffi::VIPS_SIZE_DOWN,
                ptr::null::<c_char>(),
            )
        };
        check(status)?;
        let image = Image::adopt(image)?;
        // SAFETY: `image` is a valid image.
        let has_alpha = unsafe { ffi::vips_image_hasalpha(image.0) } != 0;
        let mut buffer: *mut c_void = ptr::null_mut();
        let mut length = 0usize;
        // SAFETY: the options are name and value pairs that end in a null
        // pointer, and libvips fills `buffer` and `length` on success.
        let status = unsafe {
            if has_alpha {
                ffi::vips_pngsave_buffer(
                    image.0,
                    &mut buffer,
                    &mut length,
                    c"keep".as_ptr(),
                    0 as c_int,
                    ptr::null::<c_char>(),
                )
            } else {
                ffi::vips_jpegsave_buffer(
                    image.0,
                    &mut buffer,
                    &mut length,
                    c"Q".as_ptr(),
                    JPEG_QUALITY,
                    c"keep".as_ptr(),
                    0 as c_int,
                    ptr::null::<c_char>(),
                )
            }
        };
        check(status)?;
        // SAFETY: on success `buffer` holds `length` bytes that this copies and
        // then frees with the allocator libvips used.
        let bytes = unsafe {
            let bytes = slice::from_raw_parts(buffer.cast::<u8>(), length).to_vec();
            ffi::g_free(buffer);
            bytes
        };
        Ok(PictureCopy {
            format: if has_alpha { CopyFormat::Png } else { CopyFormat::Jpeg },
            bytes,
        })
    }
}
