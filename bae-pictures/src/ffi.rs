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

/// `VipsAccess`: read the picture once, top to bottom.
pub(crate) const VIPS_ACCESS_SEQUENTIAL: c_int = 1;
/// `VipsInterpretation`: 8-bit sRGB.
pub(crate) const VIPS_INTERPRETATION_SRGB: c_int = 22;
/// `VipsForeignDzLayout`: DeepZoom's folders, one per level.
pub(crate) const VIPS_FOREIGN_DZ_LAYOUT_DZ: c_int = 0;
/// `VipsForeignDzDepth`: levels down to the one that fits one tile.
pub(crate) const VIPS_FOREIGN_DZ_DEPTH_ONETILE: c_int = 1;
/// `VipsForeignDzContainer`: tiles as files in folders.
pub(crate) const VIPS_FOREIGN_DZ_CONTAINER_FS: c_int = 0;
/// `VipsForeignKeep`: no metadata in what is saved.
pub(crate) const VIPS_FOREIGN_KEEP_NONE: c_int = 0;
/// `VipsDirection`.
pub(crate) const VIPS_DIRECTION_HORIZONTAL: c_int = 0;
pub(crate) const VIPS_DIRECTION_VERTICAL: c_int = 1;
/// `VipsAngle`, clockwise.
pub(crate) const VIPS_ANGLE_D90: c_int = 1;
pub(crate) const VIPS_ANGLE_D180: c_int = 2;
pub(crate) const VIPS_ANGLE_D270: c_int = 3;

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
    pub(crate) fn vips_image_new_from_source(
        source: *mut VipsSource,
        option_string: *const c_char,
        ...
    ) -> *mut VipsImage;
    pub(crate) fn vips_image_new_from_file(name: *const c_char, ...) -> *mut VipsImage;
    pub(crate) fn vips_image_write_to_file(
        image: *mut VipsImage,
        name: *const c_char,
        ...
    ) -> c_int;
    pub(crate) fn vips_image_get_width(image: *const VipsImage) -> c_int;
    pub(crate) fn vips_image_get_height(image: *const VipsImage) -> c_int;
    pub(crate) fn vips_image_get_orientation(image: *mut VipsImage) -> c_int;
    pub(crate) fn vips_image_get_typeof(image: *const VipsImage, name: *const c_char) -> c_ulong;
    pub(crate) fn vips_icc_transform(
        image: *mut VipsImage,
        out: *mut *mut VipsImage,
        output_profile: *const c_char,
        ...
    ) -> c_int;
    pub(crate) fn vips_colourspace(
        image: *mut VipsImage,
        out: *mut *mut VipsImage,
        space: c_int,
        ...
    ) -> c_int;
    pub(crate) fn vips_dzsave(image: *mut VipsImage, name: *const c_char, ...) -> c_int;
    pub(crate) fn vips_arrayjoin(
        images: *mut *mut VipsImage,
        out: *mut *mut VipsImage,
        count: c_int,
        ...
    ) -> c_int;
    pub(crate) fn vips_extract_area(
        image: *mut VipsImage,
        out: *mut *mut VipsImage,
        left: c_int,
        top: c_int,
        width: c_int,
        height: c_int,
        ...
    ) -> c_int;
    pub(crate) fn vips_rot(
        image: *mut VipsImage,
        out: *mut *mut VipsImage,
        angle: c_int,
        ...
    ) -> c_int;
    pub(crate) fn vips_flip(
        image: *mut VipsImage,
        out: *mut *mut VipsImage,
        direction: c_int,
        ...
    ) -> c_int;
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
