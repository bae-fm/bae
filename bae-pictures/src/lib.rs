//! Pictures through libvips: a small JPEG of a large picture and a tile
//! pyramid of it. Each reads the original
//! once, top to bottom, through plain reads, and holds a few rows of it in
//! memory rather than the whole picture.

mod ffi;
mod source;

use std::ffi::{c_char, c_int, CStr, CString};
use std::os::unix::ffi::OsStrExt;
use std::path::{Path, PathBuf};
use std::ptr;

use source::FileSource;

/// Why a picture job failed.
#[derive(Debug, thiserror::Error)]
pub enum PicturesError {
    #[error("cannot open {}: {source}", .path.display())]
    Open { path: PathBuf, source: std::io::Error },
    #[error("libvips: {0}")]
    Vips(String),
    #[error("path {} cannot be passed to libvips: {source}", .path.display())]
    Path { path: PathBuf, source: std::ffi::NulError },
}

/// Starts libvips. libvips counts the calls, so starting it again is safe and
/// only the first call does the work.
fn init() -> Result<(), PicturesError> {
    // SAFETY: argv0 is a valid C string.
    check(unsafe { ffi::vips_init(c"bae-pictures".as_ptr()) })
}

/// libvips' description of its last failure, which it then forgets.
fn take_error() -> String {
    // SAFETY: the buffer is a C string that stays valid until the next
    // libvips call on this thread, and this copies it at once.
    unsafe {
        let message = CStr::from_ptr(ffi::vips_error_buffer()).to_string_lossy().trim().to_owned();
        ffi::vips_error_clear();
        message
    }
}

fn c_path(path: &Path) -> Result<CString, PicturesError> {
    CString::new(path.as_os_str().as_bytes())
        .map_err(|source| PicturesError::Path { path: path.to_path_buf(), source })
}

/// A libvips image, released on drop.
struct Image(*mut ffi::VipsImage);

impl Image {
    fn adopt(pointer: *mut ffi::VipsImage) -> Result<Self, PicturesError> {
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

fn check(status: c_int) -> Result<(), PicturesError> {
    if status == 0 {
        Ok(())
    } else {
        Err(PicturesError::Vips(take_error()))
    }
}

/// Writes `thumbnail` as a JPEG that fits in a square of `edge` pixels, from
/// the picture at `original`. libvips shrinks the picture as the rows arrive,
/// turns it upright by its orientation flag, and converts its colours to sRGB.
pub fn thumbnail_jpeg(original: &Path, thumbnail: &Path, edge: u16) -> Result<(), PicturesError> {
    let edge = c_int::from(edge);
    let output = c_path(thumbnail)?;
    let source = FileSource::open(original)?;
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
    // SAFETY: as above.
    let status = unsafe {
        ffi::vips_jpegsave(
            image.0,
            output.as_ptr(),
            c"Q".as_ptr(),
            80 as c_int,
            ptr::null::<c_char>(),
        )
    };
    check(status)
}

/// Writes a Deep Zoom pyramid of the picture at `original` under `tiles`: the
/// folder `<tiles>_files` holds one folder per zoom level of JPEG tiles of
/// `tile_edge` pixels, and `<tiles>.dzi` names their sizes. The picture is
/// cut as its rows arrive, one band of tiles at a time.
pub fn cut_tiles(original: &Path, tiles: &Path, tile_edge: u16) -> Result<(), PicturesError> {
    let tile_edge = c_int::from(tile_edge);
    let output = c_path(tiles)?;
    let source = FileSource::open(original)?;
    // SAFETY: the options are name and value pairs that end in a null pointer.
    let image = Image::adopt(unsafe {
        ffi::vips_image_new_from_source(
            source.as_ptr(),
            c"".as_ptr(),
            c"access".as_ptr(),
            ffi::VIPS_ACCESS_SEQUENTIAL,
            ptr::null::<c_char>(),
        )
    })?;
    // SAFETY: as above.
    let status = unsafe {
        ffi::vips_dzsave(
            image.0,
            output.as_ptr(),
            c"tile_size".as_ptr(),
            tile_edge,
            c"overlap".as_ptr(),
            0 as c_int,
            c"suffix".as_ptr(),
            c".jpg[Q=80]".as_ptr(),
            ptr::null::<c_char>(),
        )
    };
    check(status)
}

#[cfg(test)]
mod tests;
