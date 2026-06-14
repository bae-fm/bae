/// Repeat mode for playback
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum RepeatMode {
    None,
    Track,
    Album,
}

#[allow(clippy::derivable_impls)]
impl Default for RepeatMode {
    fn default() -> Self {
        RepeatMode::None
    }
}
