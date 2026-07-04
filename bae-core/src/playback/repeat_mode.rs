/// Repeat mode for playback.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
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
