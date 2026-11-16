//! The stars a person rates an album or a track with.

/// One to five stars.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Stars {
    One = 1,
    Two = 2,
    Three = 3,
    Four = 4,
    Five = 5,
}

impl Stars {
    /// Returns the stars for a stored value from 1 to 5, or `None` for any
    /// other value.
    pub fn from_stored(value: i64) -> Option<Self> {
        match value {
            1 => Some(Self::One),
            2 => Some(Self::Two),
            3 => Some(Self::Three),
            4 => Some(Self::Four),
            5 => Some(Self::Five),
            _ => None,
        }
    }
}
