use crate::audio_output::AudioOutput;

/// The main player's output volume and mute, as one owner. The level itself
/// lives in the `AudioOutput` — that is the gain the audio callback multiplies
/// by — so muting parks the output at zero and keeps the level to put back
/// here. Every transition goes through these methods, which is what makes
/// "unmute restores the pre-mute level" a single decision rather than one made
/// again at each command.
///
/// The output is passed in rather than held, because the service swaps it when
/// AirPlay takes over the sink and puts the local one back when AirPlay ends.
pub(super) struct OutputVolume {
    muted: bool,
    /// The level to put back on unmute. Read only while `muted`; every mute
    /// captures it afresh from the output.
    pre_mute_level: f32,
}

impl OutputVolume {
    pub(super) fn new() -> Self {
        Self { muted: false, pre_mute_level: 1.0 }
    }

    #[cfg(test)]
    fn is_muted(&self) -> bool {
        self.muted
    }

    /// The level the user set — what the UI shows. While muted the output
    /// sits at zero, so the remembered pre-mute level is the answer.
    #[cfg(test)]
    fn level(&self, output: &dyn AudioOutput) -> f32 {
        if self.muted {
            self.pre_mute_level
        } else {
            output.get_volume()
        }
    }

    /// What a listener hears right now — zero while muted. A remote device is
    /// seeded with this so it matches what the user hears.
    pub(super) fn audible_level(&self, output: &dyn AudioOutput) -> f32 {
        if self.muted {
            0.0
        } else {
            output.get_volume()
        }
    }

    /// Apply `level` to the output. Returns whether this lifted mute: raising
    /// the level above zero is how a muted output is unmuted, while setting
    /// zero leaves mute as it stands.
    pub(super) fn set(&mut self, level: f32, output: &dyn AudioOutput) -> bool {
        output.set_volume(level);
        let lifted_mute = self.muted && level > 0.0;
        if lifted_mute {
            self.muted = false;
        }
        lifted_mute
    }

    /// Mute or unmute. Returns the new audible level — zero on mute, the
    /// pre-mute level on unmute — or `None` when the output is already in that
    /// state and nothing changes.
    pub(super) fn set_muted(&mut self, muted: bool, output: &dyn AudioOutput) -> Option<f32> {
        if muted == self.muted {
            return None;
        }
        let level = if muted {
            self.pre_mute_level = output.get_volume();
            0.0
        } else {
            self.pre_mute_level
        };
        self.muted = muted;
        output.set_volume(level);
        Some(level)
    }

    /// Adopt a persisted level and mute state: the output plays at `level`, or
    /// at zero with `level` held for the unmute when the session was muted.
    pub(super) fn restore(&mut self, level: f32, muted: bool, output: &dyn AudioOutput) {
        self.muted = muted;
        self.pre_mute_level = level;
        output.set_volume(if muted { 0.0 } else { level });
    }
}

#[cfg(test)]
#[path = "volume_tests.rs"]
mod tests;
