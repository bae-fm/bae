//! Where an upright tile's pixels are in the picture as stored, which a
//! picture's orientation flag turns or mirrors.

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

#[cfg(test)]
#[path = "orientation_tests.rs"]
mod tests;
