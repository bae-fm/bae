use crate::playback::audio_output::AudioOutput;

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
        Self {
            muted: false,
            pre_mute_level: 1.0,
        }
    }

    pub(super) fn is_muted(&self) -> bool {
        self.muted
    }

    /// The level the user set — what the UI shows and what is persisted. While
    /// muted the output sits at zero, so the remembered pre-mute level is the
    /// answer.
    pub(super) fn level(&self, output: &dyn AudioOutput) -> f32 {
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
mod tests {
    use super::*;
    use crate::playback::audio_output::{AudioError, AudioEventSender, AudioState, AudioStream};
    use crate::playback::source::PlaybackSource;
    use std::sync::{Arc, Mutex};

    /// An output that only remembers its level — the whole of what `OutputVolume`
    /// touches. Everything else on the trait is unreachable from here, and says
    /// so rather than answering with a plausible default.
    struct VolumeOnlyOutput(Mutex<f32>);

    impl VolumeOnlyOutput {
        fn new() -> Self {
            Self(Mutex::new(1.0))
        }
    }

    impl AudioOutput for VolumeOnlyOutput {
        fn create_stream(
            &mut self,
            _source: Arc<Mutex<PlaybackSource>>,
            _source_sample_rate: u32,
            _source_channels: u32,
            _audio_events: AudioEventSender,
            _position_update_interval_ms: u32,
        ) -> Result<Box<dyn AudioStream>, AudioError> {
            unreachable!("volume never builds a stream")
        }

        fn set_state(&self, _state: AudioState) {
            unreachable!("volume never sets the play state")
        }

        fn get_state(&self) -> AudioState {
            unreachable!("volume never reads the play state")
        }

        fn set_volume(&self, volume: f32) {
            *self.0.lock().unwrap() = volume;
        }

        fn get_volume(&self) -> f32 {
            *self.0.lock().unwrap()
        }
    }

    #[test]
    fn unmute_restores_the_level_the_user_set() {
        let output = VolumeOnlyOutput::new();
        let mut volume = OutputVolume::new();

        assert!(!volume.set(0.4, &output));
        assert_eq!(volume.set_muted(true, &output), Some(0.0));
        assert_eq!(output.get_volume(), 0.0);
        assert_eq!(volume.level(&output), 0.4, "the UI keeps showing 0.4");
        assert_eq!(volume.audible_level(&output), 0.0);

        assert_eq!(volume.set_muted(false, &output), Some(0.4));
        assert_eq!(output.get_volume(), 0.4);
    }

    #[test]
    fn muting_twice_changes_nothing() {
        let output = VolumeOnlyOutput::new();
        let mut volume = OutputVolume::new();
        volume.set(0.6, &output);

        assert_eq!(volume.set_muted(true, &output), Some(0.0));
        assert_eq!(volume.set_muted(true, &output), None);
        assert_eq!(
            volume.set_muted(false, &output),
            Some(0.6),
            "the repeated mute did not overwrite the pre-mute level with zero"
        );
    }

    #[test]
    fn raising_the_level_unmutes_and_setting_zero_does_not() {
        let output = VolumeOnlyOutput::new();
        let mut volume = OutputVolume::new();
        volume.set(0.5, &output);
        volume.set_muted(true, &output);

        assert!(!volume.set(0.0, &output), "zero leaves mute standing");
        assert!(volume.is_muted());

        assert!(volume.set(0.3, &output), "a level above zero lifts mute");
        assert!(!volume.is_muted());
        assert_eq!(volume.level(&output), 0.3);
    }

    #[test]
    fn a_restored_muted_session_comes_back_silent_at_its_saved_level() {
        let output = VolumeOnlyOutput::new();
        let mut volume = OutputVolume::new();

        volume.restore(0.7, true, &output);
        assert_eq!(output.get_volume(), 0.0);
        assert_eq!(volume.level(&output), 0.7);
        assert_eq!(volume.set_muted(false, &output), Some(0.7));

        volume.restore(0.2, false, &output);
        assert!(!volume.is_muted());
        assert_eq!(output.get_volume(), 0.2);
    }
}
