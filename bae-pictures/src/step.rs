//! The sizes a copy of a picture is made at.

/// A copy's longer edge in pixels. Each step is twice the one before; a
/// request rounds up to the next step, and nothing above the last is made
/// here.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum PictureStep {
    S192,
    S384,
    S768,
    S1536,
}

impl PictureStep {
    /// The smallest step that holds `pixels`, or the largest step for a
    /// request above it.
    pub fn covering(pixels: u32) -> Self {
        match pixels {
            0..=192 => Self::S192,
            193..=384 => Self::S384,
            385..=768 => Self::S768,
            _ => Self::S1536,
        }
    }

    pub fn edge(self) -> u16 {
        match self {
            Self::S192 => 192,
            Self::S384 => 384,
            Self::S768 => 768,
            Self::S1536 => 1536,
        }
    }
}
