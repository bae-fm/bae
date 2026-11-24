//! Cutting a picture into the tiles a viewer zooms it from: 512-pixel tiles
//! at each zoom level, from the picture's own size halving down to the level
//! that fits one tile. libvips reads the original once, top to bottom, and
//! writes each band of tiles as its rows arrive, so a few rows of the picture
//! are in memory and never the whole of it.
//!
//! The levels are cut from the picture as it is stored. A picture whose
//! orientation flag turns or mirrors it is turned tile by tile afterwards:
//! turning the whole picture would need all of it at once.

use std::path::{Path, PathBuf};

use crate::{CopyFormat, IsCancelled, PictureOrigin, PicturesError};

/// The edge of a tile, in pixels. A tile at a level's right or bottom edge is
/// smaller.
pub const TILE_EDGE: u32 = 512;

/// A picture's tiles: its size upright, and how its tiles are encoded.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Pyramid {
    pub width: u32,
    pub height: u32,
    pub format: CopyFormat,
}

/// One zoom level of a pyramid: its size, and how many tiles across and down.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TileLevel {
    pub width: u32,
    pub height: u32,
    pub columns: u32,
    pub rows: u32,
}

impl Pyramid {
    /// The pyramid's levels, its own size first, each half the one before it
    /// (rounded up), down to the first that fits one tile.
    pub fn levels(&self) -> Vec<TileLevel> {
        let mut levels = Vec::new();
        let (mut width, mut height) = (self.width, self.height);
        loop {
            levels.push(TileLevel {
                width,
                height,
                columns: width.div_ceil(TILE_EDGE),
                rows: height.div_ceil(TILE_EDGE),
            });
            if width <= TILE_EDGE && height <= TILE_EDGE {
                return levels;
            }
            (width, height) = (width.div_ceil(2), height.div_ceil(2));
        }
    }
}

/// The file in `folder` that holds the tile at column `x` and row `y` of
/// level `level` (0 is the picture's own size).
pub fn tile_path(folder: &Path, format: CopyFormat, level: u32, x: u32, y: u32) -> PathBuf {
    folder.join(level.to_string()).join(format!("{x}_{y}.{}", extension(format)))
}

fn extension(format: CopyFormat) -> &'static str {
    match format {
        CopyFormat::Jpeg => "jpg",
        CopyFormat::Png => "png",
    }
}

/// How a picture's orientation flag (EXIF 1 to 8) turns the picture as stored
/// into the picture upright.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Orientation {
    Upright,
    MirrorHorizontal,
    Rotate180,
    MirrorVertical,
    Transpose,
    Rotate90,
    Transverse,
    Rotate270,
}

/// A rectangle of a level, in pixels.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Area {
    pub(crate) left: u32,
    pub(crate) top: u32,
    pub(crate) width: u32,
    pub(crate) height: u32,
}

impl Orientation {
    /// The orientation the flag `flag` names; a flag outside 1 to 8 names
    /// none, and the picture shows as stored.
    pub(crate) fn of_flag(flag: i32) -> Self {
        match flag {
            2 => Self::MirrorHorizontal,
            3 => Self::Rotate180,
            4 => Self::MirrorVertical,
            5 => Self::Transpose,
            6 => Self::Rotate90,
            7 => Self::Transverse,
            8 => Self::Rotate270,
            _ => Self::Upright,
        }
    }

    /// Whether the upright picture is the stored one with its width and
    /// height swapped.
    pub(crate) fn swaps_sides(self) -> bool {
        matches!(self, Self::Transpose | Self::Rotate90 | Self::Transverse | Self::Rotate270)
    }

    /// The area of a level as stored, `stored_width` by `stored_height`, that
    /// turns into the area `upright` of the same level upright.
    pub(crate) fn stored_area(self, stored_width: u32, stored_height: u32, upright: Area) -> Area {
        let (w, h) = (stored_width, stored_height);
        let Area { left: x, top: y, width: tw, height: th } = upright;
        match self {
            Self::Upright => upright,
            Self::MirrorHorizontal => Area { left: w - x - tw, top: y, width: tw, height: th },
            Self::Rotate180 => Area { left: w - x - tw, top: h - y - th, width: tw, height: th },
            Self::MirrorVertical => Area { left: x, top: h - y - th, width: tw, height: th },
            Self::Transpose => Area { left: y, top: x, width: th, height: tw },
            Self::Rotate90 => Area { left: y, top: h - x - tw, width: th, height: tw },
            Self::Transverse => Area { left: w - y - th, top: h - x - tw, width: th, height: tw },
            Self::Rotate270 => Area { left: w - y - th, top: x, width: th, height: tw },
        }
    }
}

/// Cut the picture `origin` holds into its tiles in `folder`, which must not
/// exist yet and which a failed or cancelled cut leaves partly written for
/// the caller to remove. `origin` is read in chunks with `cancelled` asked
/// before each, so a cancelled cut ends with [`PicturesError::Cancelled`] at
/// the next one. A picture with transparency is cut into PNG tiles, any other
/// into JPEG tiles, in sRGB.
#[cfg(libvips)]
pub fn cut_tiles(
    origin: impl PictureOrigin,
    cancelled: IsCancelled,
    folder: &Path,
) -> Result<Pyramid, PicturesError> {
    let result = libvips::cut(origin, cancelled.clone(), folder);
    match result {
        Err(_) if cancelled() => Err(PicturesError::Cancelled),
        other => other,
    }
}

/// Without libvips there are no tiles to cut: see `build.rs`.
#[cfg(not(libvips))]
pub fn cut_tiles(
    _origin: impl PictureOrigin,
    _cancelled: IsCancelled,
    _folder: &Path,
) -> Result<Pyramid, PicturesError> {
    Err(PicturesError::NoLibvips)
}

#[cfg(libvips)]
mod libvips {
    use std::ffi::{c_char, c_int, CString};
    use std::path::Path;
    use std::ptr;

    use super::{extension, tile_path, Area, Orientation, Pyramid, TILE_EDGE};
    use crate::source::OriginSource;
    use crate::vips::{check, Image};
    use crate::{ffi, CopyFormat, IsCancelled, PictureOrigin, PicturesError};

    /// The JPEG quality tiles are saved at.
    const JPEG_QUALITY: &str = "85";

    #[expect(
        clippy::disallowed_methods,
        reason = "file system: the tile cache's own folders, files bae keeps itself, which coven does not own"
    )]
    pub(super) fn cut(
        origin: impl PictureOrigin,
        cancelled: IsCancelled,
        folder: &Path,
    ) -> Result<Pyramid, PicturesError> {
        let source = OriginSource::new(origin, cancelled)?;
        // SAFETY: the options are name and value pairs that end in a null
        // pointer, and `source` outlives the call.
        let stored = Image::adopt(unsafe {
            ffi::vips_image_new_from_source(
                source.as_ptr(),
                c"".as_ptr(),
                c"access".as_ptr(),
                ffi::VIPS_ACCESS_SEQUENTIAL,
                ptr::null::<c_char>(),
            )
        })
        .map_err(|error| source.blame_read(error))?;
        // SAFETY: `stored` is a valid image.
        let orientation =
            Orientation::of_flag(unsafe { ffi::vips_image_get_orientation(stored.0) });
        let srgb = in_srgb(&stored).map_err(|error| source.blame_read(error))?;
        // SAFETY: `srgb` is a valid image.
        let (width, height, has_alpha) = unsafe {
            (
                dimension(ffi::vips_image_get_width(srgb.0))?,
                dimension(ffi::vips_image_get_height(srgb.0))?,
                ffi::vips_image_hasalpha(srgb.0) != 0,
            )
        };
        let format = if has_alpha { CopyFormat::Png } else { CopyFormat::Jpeg };
        std::fs::create_dir_all(folder).map_err(PicturesError::Write)?;
        let stored_tiles = folder.join("stored");
        let name = path_string(&stored_tiles)?;
        let suffix = c_string(&format!(".{}{}", extension(format), save_options(format)))?;
        // SAFETY: the options are name and value pairs that end in a null
        // pointer, and the strings outlive the call.
        check(unsafe {
            ffi::vips_dzsave(
                srgb.0,
                name.as_ptr(),
                c"layout".as_ptr(),
                ffi::VIPS_FOREIGN_DZ_LAYOUT_DZ,
                c"tile_size".as_ptr(),
                TILE_EDGE as c_int,
                c"overlap".as_ptr(),
                0 as c_int,
                c"depth".as_ptr(),
                ffi::VIPS_FOREIGN_DZ_DEPTH_ONETILE,
                c"container".as_ptr(),
                ffi::VIPS_FOREIGN_DZ_CONTAINER_FS,
                c"suffix".as_ptr(),
                suffix.as_ptr(),
                c"keep".as_ptr(),
                ffi::VIPS_FOREIGN_KEEP_NONE,
                ptr::null::<c_char>(),
            )
        })
        .map_err(|error| source.blame_read(error))?;
        drop(srgb);
        drop(stored);
        // A read that failed ends the picture early, and libvips saves what
        // it has: the failure is the cut's.
        if let Some(failure) = source.take_failure() {
            return Err(failure);
        }
        drop(source);
        let stored_pyramid = Pyramid { width, height, format };
        let upright = if orientation.swaps_sides() {
            Pyramid { width: height, height: width, format }
        } else {
            stored_pyramid
        };
        // dzsave numbers its levels DeepZoom's way: the picture's own size is
        // the highest number, and each level below it is one less.
        let dz_files = folder.join("stored_files");
        let stored_levels = stored_pyramid.levels();
        let (top, count) = dz_levels(&dz_files)?;
        if count != stored_levels.len() {
            return Err(PicturesError::Vips(format!(
                "dzsave cut {count} levels of a {width} by {height} picture, not {}",
                stored_levels.len()
            )));
        }
        let upright_levels = upright.levels();
        for (index, level) in upright_levels.iter().enumerate() {
            let index = u32::try_from(index).map_err(too_large)?;
            let dz_level = dz_files.join((top - index).to_string());
            if orientation == Orientation::Upright {
                std::fs::rename(&dz_level, folder.join(index.to_string()))
                    .map_err(PicturesError::Write)?;
            } else {
                turn_level(&dz_level, folder, index, *level, orientation, format)?;
            }
        }
        std::fs::remove_dir_all(&dz_files).map_err(PicturesError::Write)?;
        std::fs::remove_file(folder.join("stored.dzi")).map_err(PicturesError::Write)?;
        Ok(upright)
    }

    /// The picture with its colours in sRGB: converted through its own colour
    /// profile when it has one, else from the colour space libvips reads it
    /// in.
    fn in_srgb(image: &Image) -> Result<Image, PicturesError> {
        let mut out = ptr::null_mut();
        // SAFETY: `image` is a valid image, and the options end in a null
        // pointer.
        let status = unsafe {
            if ffi::vips_image_get_typeof(image.0, c"icc-profile-data".as_ptr()) != 0 {
                ffi::vips_icc_transform(
                    image.0,
                    &mut out,
                    c"srgb".as_ptr(),
                    c"embedded".as_ptr(),
                    1 as c_int,
                    ptr::null::<c_char>(),
                )
            } else {
                ffi::vips_colourspace(
                    image.0,
                    &mut out,
                    ffi::VIPS_INTERPRETATION_SRGB,
                    ptr::null::<c_char>(),
                )
            }
        };
        check(status)?;
        Image::adopt(out)
    }

    /// The highest level number in dzsave's folder of levels, and how many
    /// levels it holds: one folder for each.
    #[expect(
        clippy::disallowed_methods,
        reason = "file system: the tile cache's own folders, files bae keeps itself, which coven does not own"
    )]
    fn dz_levels(dz_files: &Path) -> Result<(u32, usize), PicturesError> {
        let mut top = None;
        let mut count = 0;
        for entry in std::fs::read_dir(dz_files).map_err(PicturesError::Write)? {
            let entry = entry.map_err(PicturesError::Write)?;
            // dzsave writes a file of the picture's properties beside the
            // levels' folders.
            if !entry.file_type().map_err(PicturesError::Write)?.is_dir() {
                continue;
            }
            let level = entry.file_name().to_str().and_then(|name| name.parse::<u32>().ok());
            let Some(level) = level else {
                return Err(PicturesError::Vips(format!(
                    "dzsave wrote {:?}, which names no level",
                    entry.file_name()
                )));
            };
            top = top.max(Some(level));
            count += 1;
        }
        let top = top.ok_or_else(|| PicturesError::Vips("dzsave wrote no level".to_string()))?;
        Ok((top, count))
    }

    /// Write the tiles of level `index` upright into `folder`, each made from
    /// the stored tiles in `dz_level` that hold its pixels, turned by
    /// `orientation`. `level` is the level as it is upright.
    #[expect(
        clippy::disallowed_methods,
        reason = "file system: the tile cache's own folders, files bae keeps itself, which coven does not own"
    )]
    fn turn_level(
        dz_level: &Path,
        folder: &Path,
        index: u32,
        level: super::TileLevel,
        orientation: Orientation,
        format: CopyFormat,
    ) -> Result<(), PicturesError> {
        let (width, height) = (level.width, level.height);
        let (stored_width, stored_height) =
            if orientation.swaps_sides() { (height, width) } else { (width, height) };
        std::fs::create_dir_all(folder.join(index.to_string())).map_err(PicturesError::Write)?;
        for y in 0..level.rows {
            for x in 0..level.columns {
                let left = x * TILE_EDGE;
                let top = y * TILE_EDGE;
                let upright = Area {
                    left,
                    top,
                    width: TILE_EDGE.min(width - left),
                    height: TILE_EDGE.min(height - top),
                };
                let area = orientation.stored_area(stored_width, stored_height, upright);
                let tile = stored_area(dz_level, area, format)?;
                let turned = turn(tile, orientation)?;
                save(&turned, &tile_path(folder, format, index, x, y), format)?;
            }
        }
        Ok(())
    }

    /// The pixels of `area` of a stored level, joined from the stored tiles
    /// in `dz_level` that hold them.
    fn stored_area(
        dz_level: &Path,
        area: Area,
        format: CopyFormat,
    ) -> Result<Image, PicturesError> {
        let (first_column, first_row) = (area.left / TILE_EDGE, area.top / TILE_EDGE);
        let last_column = (area.left + area.width - 1) / TILE_EDGE;
        let last_row = (area.top + area.height - 1) / TILE_EDGE;
        let mut tiles = Vec::new();
        for row in first_row..=last_row {
            for column in first_column..=last_column {
                let path = dz_level.join(format!("{column}_{row}.{}", extension(format)));
                let name = path_string(&path)?;
                // SAFETY: the options end in a null pointer.
                tiles.push(Image::adopt(unsafe {
                    ffi::vips_image_new_from_file(name.as_ptr(), ptr::null::<c_char>())
                })?);
            }
        }
        let across = c_int::try_from(last_column - first_column + 1).map_err(too_large)?;
        let mut pointers: Vec<_> = tiles.iter().map(|tile| tile.0).collect();
        let mut joined = ptr::null_mut();
        // SAFETY: `pointers` holds valid images that `tiles` keeps alive, and
        // the options end in a null pointer.
        check(unsafe {
            ffi::vips_arrayjoin(
                pointers.as_mut_ptr(),
                &mut joined,
                c_int::try_from(pointers.len()).map_err(too_large)?,
                c"across".as_ptr(),
                across,
                ptr::null::<c_char>(),
            )
        })?;
        let joined = Image::adopt(joined)?;
        let mut out = ptr::null_mut();
        // SAFETY: `joined` is a valid image, and the options end in a null
        // pointer.
        check(unsafe {
            ffi::vips_extract_area(
                joined.0,
                &mut out,
                pixels(area.left - first_column * TILE_EDGE)?,
                pixels(area.top - first_row * TILE_EDGE)?,
                pixels(area.width)?,
                pixels(area.height)?,
                ptr::null::<c_char>(),
            )
        })?;
        Image::adopt(out)
    }

    /// `tile` turned from its stored orientation to upright.
    fn turn(tile: Image, orientation: Orientation) -> Result<Image, PicturesError> {
        let (angle, mirror) = match orientation {
            Orientation::Upright => (None, None),
            Orientation::MirrorHorizontal => (None, Some(ffi::VIPS_DIRECTION_HORIZONTAL)),
            Orientation::Rotate180 => (Some(ffi::VIPS_ANGLE_D180), None),
            Orientation::MirrorVertical => (None, Some(ffi::VIPS_DIRECTION_VERTICAL)),
            Orientation::Transpose => {
                (Some(ffi::VIPS_ANGLE_D90), Some(ffi::VIPS_DIRECTION_HORIZONTAL))
            }
            Orientation::Rotate90 => (Some(ffi::VIPS_ANGLE_D90), None),
            Orientation::Transverse => {
                (Some(ffi::VIPS_ANGLE_D270), Some(ffi::VIPS_DIRECTION_HORIZONTAL))
            }
            Orientation::Rotate270 => (Some(ffi::VIPS_ANGLE_D270), None),
        };
        let mut image = tile;
        if let Some(angle) = angle {
            let mut out = ptr::null_mut();
            // SAFETY: `image` is a valid image, and the options end in a null
            // pointer.
            check(unsafe { ffi::vips_rot(image.0, &mut out, angle, ptr::null::<c_char>()) })?;
            image = Image::adopt(out)?;
        }
        if let Some(direction) = mirror {
            let mut out = ptr::null_mut();
            // SAFETY: as above.
            check(unsafe { ffi::vips_flip(image.0, &mut out, direction, ptr::null::<c_char>()) })?;
            image = Image::adopt(out)?;
        }
        Ok(image)
    }

    fn save(image: &Image, path: &Path, format: CopyFormat) -> Result<(), PicturesError> {
        let name = c_string(&format!("{}{}", path_text(path)?, save_options(format)))?;
        // SAFETY: `image` is a valid image, and the options end in a null
        // pointer.
        check(unsafe {
            ffi::vips_image_write_to_file(image.0, name.as_ptr(), ptr::null::<c_char>())
        })
    }

    /// The options a tile is saved with, as libvips reads them after a file
    /// name: no metadata, and JPEG's quality.
    fn save_options(format: CopyFormat) -> String {
        match format {
            CopyFormat::Jpeg => format!("[Q={JPEG_QUALITY},keep=none]"),
            CopyFormat::Png => "[keep=none]".to_string(),
        }
    }

    fn dimension(value: c_int) -> Result<u32, PicturesError> {
        u32::try_from(value)
            .map_err(|error| PicturesError::Vips(format!("an image {value} wide: {error}")))
    }

    fn pixels(value: u32) -> Result<c_int, PicturesError> {
        c_int::try_from(value).map_err(too_large)
    }

    fn too_large(error: std::num::TryFromIntError) -> PicturesError {
        PicturesError::Vips(format!("the picture is too large to cut into tiles: {error}"))
    }

    fn path_text(path: &Path) -> Result<&str, PicturesError> {
        path.to_str().ok_or_else(|| {
            PicturesError::Vips(format!("{} is no UTF-8 path libvips can write", path.display()))
        })
    }

    fn path_string(path: &Path) -> Result<CString, PicturesError> {
        c_string(path_text(path)?)
    }

    fn c_string(text: &str) -> Result<CString, PicturesError> {
        CString::new(text).map_err(|error| PicturesError::Vips(format!("{text:?}: {error}")))
    }
}

#[cfg(test)]
#[path = "tiles_tests.rs"]
mod tests;
