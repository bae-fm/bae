use crate::audio_output::PlayerOutput;

/// The main player's output volume and mute, as one owner: the level the
/// person set, and whether it is muted. The output's gain — what the audio
/// callback multiplies by — is this level, or zero while muted, written to
/// the output by every transition here. That is what makes "unmute restores
/// the pre-mute level" a single decision rather than one made again at each
/// command.
///
/// The output is passed in rather than held, because the service swaps it
/// when AirPlay takes over the sink and puts the local one back when AirPlay
/// ends; whichever output is in place plays at the level held here.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(super) enum OutputVolume {
    /// Playing at this level.
    Audible(f32),
    /// Muted: the output sits at zero, and `level` is what unmuting puts
    /// back.
    Muted { level: f32 },
}

impl OutputVolume {
    pub(super) fn new() -> Self {
        Self::Audible(1.0)
    }

    #[cfg(test)]
    fn is_muted(&self) -> bool {
        matches!(self, Self::Muted { .. })
    }

    /// The level the person set — what the UI shows, muted or not.
    #[cfg(test)]
    fn level(&self) -> f32 {
        match *self {
            Self::Audible(level) | Self::Muted { level } => level,
        }
    }

    /// What a listener hears right now — zero while muted. A remote device is
    /// seeded with this so it matches what the user hears.
    pub(super) fn audible_level(&self) -> f32 {
        match *self {
            Self::Audible(level) => level,
            Self::Muted { .. } => 0.0,
        }
    }

    /// Play `output` at the audible level: what an output put in place of
    /// another takes on.
    pub(super) fn apply_to(&self, output: &PlayerOutput) {
        output.set_volume(self.audible_level());
    }

    /// Set the level to `level`. Returns whether this lifted mute: raising
    /// the level above zero is how a muted output is unmuted, while setting
    /// zero leaves mute as it stands.
    pub(super) fn set(&mut self, level: f32, output: &PlayerOutput) -> bool {
        let lifted_mute = match *self {
            Self::Audible(_) => {
                *self = Self::Audible(level);
                false
            }
            Self::Muted { .. } if level > 0.0 => {
                *self = Self::Audible(level);
                true
            }
            Self::Muted { .. } => false,
        };
        self.apply_to(output);
        lifted_mute
    }

    /// Mute or unmute. Returns the new audible level — zero on mute, the
    /// pre-mute level on unmute — or `None` when the output is already in that
    /// state and nothing changes.
    pub(super) fn set_muted(&mut self, muted: bool, output: &PlayerOutput) -> Option<f32> {
        *self = match (*self, muted) {
            (Self::Audible(level), true) => Self::Muted { level },
            (Self::Muted { level }, false) => Self::Audible(level),
            (Self::Audible(_), false) | (Self::Muted { .. }, true) => return None,
        };
        self.apply_to(output);
        Some(self.audible_level())
    }

    /// Adopt a persisted level and mute state: the output plays at `level`, or
    /// at zero with `level` held for the unmute when the session was muted.
    pub(super) fn restore(&mut self, level: f32, muted: bool, output: &PlayerOutput) {
        *self = if muted { Self::Muted { level } } else { Self::Audible(level) };
        self.apply_to(output);
    }
}

#[cfg(test)]
#[path = "volume_tests.rs"]
mod tests;
