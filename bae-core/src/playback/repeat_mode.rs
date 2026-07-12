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
