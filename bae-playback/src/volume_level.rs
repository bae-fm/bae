//! The level playback plays at, from silent to full.

/// A playback level, from silent (0) to full (1). [`Volume::new`] holds a
/// level asked for to that range where it is set, so every output plays it,
/// and the row stores and screens show it, as it is.
#[derive(Debug, Clone, Copy, PartialEq, PartialOrd)]
pub struct Volume(f32);

impl Volume {
    pub const FULL: Self = Self(1.0);
    pub const SILENT: Self = Self(0.0);

    /// `level` held to silent and full; `None` for a level that is no
    /// number.
    pub fn new(level: f32) -> Option<Self> {
        (!level.is_nan()).then(|| Self(level.clamp(0.0, 1.0)))
    }

    /// The level, from 0.0 to 1.0.
    pub fn get(self) -> f32 {
        self.0
    }
}
