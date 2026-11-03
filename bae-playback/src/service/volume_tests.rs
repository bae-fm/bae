use super::*;
use crate::audio_output::{
    AudioError, AudioEventSender, AudioOutputControls, AudioState, AudioStream,
};
use crate::source::PlaybackSource;
use std::sync::{Arc, Mutex};

/// An output that only remembers its level — the whole of what `OutputVolume`
/// touches. The play state is unreachable from here, and says so rather than
/// answering with a plausible default.
struct VolumeOnlyOutput(AudioOutputControls);

impl VolumeOnlyOutput {
    fn new() -> Self {
        Self(AudioOutputControls::new(1.0))
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

    fn controls(&self) -> &AudioOutputControls {
        &self.0
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
