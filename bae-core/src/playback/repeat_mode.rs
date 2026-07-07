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
    pub(crate) fn next(self) -> Self {
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

    /// Cycling advances Off → Context → Track and wraps back to Off.
    #[test]
    fn next_cycles_off_context_track_off() {
        assert_eq!(RepeatMode::Off.next(), RepeatMode::Context);
        assert_eq!(RepeatMode::Context.next(), RepeatMode::Track);
        assert_eq!(RepeatMode::Track.next(), RepeatMode::Off);
    }
}
