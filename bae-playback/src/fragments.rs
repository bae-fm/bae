//! The read fragments this crate offers to other crates' reads.

sqlive_compose::fragment!(
    PlaybackNowState,
    r#"
    SELECT n.state FROM playback_now AS n
"#
);

impl PlaybackNowState {
    pub fn state(&self) -> &str {
        &self.state
    }
}
