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
#[path = "repeat_mode_tests.rs"]
mod tests;
