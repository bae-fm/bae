//! The size a request wants a picture at.

use crate::PictureStep;

/// What a request wants: a copy at a step, or the original's own bytes for a
/// viewer that decodes the picture at full size.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PictureSize {
    Copy(PictureStep),
    Original,
}

impl PictureSize {
    /// The size a request for `pixels` wide on its longer side is made at:
    /// the step that covers it, or the original when it names none.
    pub fn covering(pixels: Option<u32>) -> Self {
        pixels.map_or(Self::Original, |pixels| Self::Copy(PictureStep::covering(pixels)))
    }
}
