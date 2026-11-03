use super::*;

#[test]
fn the_cycle_advances_off_to_context_to_track() {
    assert_eq!(RepeatMode::Off.next(), RepeatMode::Context);
    assert_eq!(RepeatMode::Context.next(), RepeatMode::Track);
    assert_eq!(RepeatMode::Track.next(), RepeatMode::Off);
}

/// The cycle is closed: pressing the button once per mode returns to where it
/// started, so no mode is unreachable and none is a dead end.
#[test]
fn three_steps_return_to_the_starting_mode() {
    for mode in [RepeatMode::Off, RepeatMode::Track, RepeatMode::Context] {
        assert_eq!(mode.next().next().next(), mode);
    }
}
