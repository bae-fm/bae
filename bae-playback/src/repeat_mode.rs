/// Repeat mode for playback.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum RepeatMode {
    /// No repeat — play through and stop.
    Off,
    /// Pin the current track.
    Track,
    /// Loop the whole context (the release being played from).
    Context,
}

impl RepeatMode {
    /// The mode a repeat button steps to next: Off → Context → Track → Off.
    ///
    /// Which mode follows which is a fact about repeat itself, not about any one
    /// button, so it is decided here: the broad case (loop what is playing) comes
    /// before the narrow one (loop this track). Playback only ever *sets* an
    /// absolute mode — the caller works out the target and sends it.
    pub fn next(self) -> Self {
        match self {
            Self::Off => Self::Context,
            Self::Context => Self::Track,
            Self::Track => Self::Off,
        }
    }
}

#[cfg(test)]
mod tests {
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
}
