//! The read fragments this crate offers to other crates' reads.

sqlive_compose::fragment!(
    ArtistIds,
    r#"
    SELECT a.id AS artist_id FROM artists AS a
"#
);

impl ArtistIds {
    pub fn artist_id(&self) -> &str {
        &self.artist_id
    }
}
