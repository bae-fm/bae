//! The read fragments this crate offers to other crates' reads.

sqlive_compose::fragment!(
    WatchedFolderPositions,
    r#"
    SELECT w.position FROM watched_import_folders AS w
"#
);

impl WatchedFolderPositions {
    pub fn position(&self) -> i64 {
        self.position
    }
}
