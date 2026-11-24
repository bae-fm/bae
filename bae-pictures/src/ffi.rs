//! The libvips and GLib functions the crate calls, declared by hand. The
//! variadic ones take their options as name and value pairs and end with a
//! null pointer.

use std::ffi::{c_char, c_int, c_ulong, c_void};

/// A libvips image, which only libvips reads.
#[repr(C)]
pub(crate) struct VipsImage {
    _opaque: [u8; 0],
}

/// `VipsSource`, of which `VipsSourceCustom` is the first member.
#[repr(C)]
pub(crate) struct VipsSource {
    _opaque: [u8; 0],
}

/// `VipsSize`: shrink a larger picture and never enlarge a smaller one.
pub(crate) const VIPS_SIZE_DOWN: c_int = 2;

/// `GClosureNotify`'s first two arguments: the data and the closure.
pub(crate) type DestroyNotify = unsafe extern "C" fn(*mut c_void, *mut c_void);

extern "C" {
    pub(crate) fn vips_init(argv0: *const c_char) -> c_int;
    pub(crate) fn vips_concurrency_set(concurrency: c_int);
    pub(crate) fn vips_error_buffer() -> *const c_char;
    pub(crate) fn vips_error_clear();
    pub(crate) fn vips_source_custom_new() -> *mut VipsSource;
    pub(crate) fn vips_thumbnail_source(
        source: *mut VipsSource,
        out: *mut *mut VipsImage,
        width: c_int,
        ...
    ) -> c_int;
    pub(crate) fn vips_image_hasalpha(image: *mut VipsImage) -> c_int;
    pub(crate) fn vips_jpegsave_buffer(
        image: *mut VipsImage,
        buffer: *mut *mut c_void,
        length: *mut usize,
        ...
    ) -> c_int;
    pub(crate) fn vips_pngsave_buffer(
        image: *mut VipsImage,
        buffer: *mut *mut c_void,
        length: *mut usize,
        ...
    ) -> c_int;
    pub(crate) fn g_free(memory: *mut c_void);
    pub(crate) fn g_object_unref(object: *mut c_void);
    pub(crate) fn g_signal_connect_data(
        instance: *mut c_void,
        signal: *const c_char,
        handler: unsafe extern "C" fn(),
        data: *mut c_void,
        destroy: Option<DestroyNotify>,
        flags: c_int,
    ) -> c_ulong;
}
