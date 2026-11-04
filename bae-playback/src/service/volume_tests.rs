use super::*;
use crate::audio_output::{AudioOutputControls, FailingAudioOutput, PlayerOutput};

fn at(level: f32) -> Volume {
    Volume::new(level).unwrap()
}

/// A player output at full volume over an output that builds no stream: the
/// level is all `OutputVolume` touches.
fn volume_only_output() -> PlayerOutput {
    PlayerOutput::new(Box::new(FailingAudioOutput), AudioOutputControls::new(crate::Volume::FULL))
}

#[test]
fn unmute_restores_the_level_the_user_set() {
    let output = volume_only_output();
    let mut volume = OutputVolume::new();

    assert!(!volume.set(at(0.4), &output));
    assert_eq!(volume.set_muted(true, &output), Some(at(0.0)));
    assert_eq!(output.get_volume(), 0.0);
    assert_eq!(volume.level(), at(0.4), "the UI keeps showing 0.4");
    assert_eq!(volume.audible_level(), at(0.0));

    assert_eq!(volume.set_muted(false, &output), Some(at(0.4)));
    assert_eq!(output.get_volume(), 0.4);
}

#[test]
fn muting_twice_changes_nothing() {
    let output = volume_only_output();
    let mut volume = OutputVolume::new();
    volume.set(at(0.6), &output);

    assert_eq!(volume.set_muted(true, &output), Some(at(0.0)));
    assert_eq!(volume.set_muted(true, &output), None);
    assert_eq!(
        volume.set_muted(false, &output),
        Some(at(0.6)),
        "the repeated mute did not overwrite the pre-mute level with zero"
    );
}

#[test]
fn raising_the_level_unmutes_and_setting_zero_does_not() {
    let output = volume_only_output();
    let mut volume = OutputVolume::new();
    volume.set(at(0.5), &output);
    volume.set_muted(true, &output);

    assert!(!volume.set(at(0.0), &output), "zero leaves mute standing");
    assert!(volume.is_muted());

    assert!(volume.set(at(0.3), &output), "a level above zero lifts mute");
    assert!(!volume.is_muted());
    assert_eq!(volume.level(), at(0.3));
}

#[test]
fn a_restored_muted_session_comes_back_silent_at_its_saved_level() {
    let output = volume_only_output();
    let mut volume = OutputVolume::new();

    volume.restore(at(0.7), true, &output);
    assert_eq!(output.get_volume(), 0.0);
    assert_eq!(volume.level(), at(0.7));
    assert_eq!(volume.set_muted(false, &output), Some(at(0.7)));

    volume.restore(at(0.2), false, &output);
    assert!(!volume.is_muted());
    assert_eq!(output.get_volume(), 0.2);
}
