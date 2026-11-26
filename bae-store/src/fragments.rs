//! The read fragments this crate offers to other crates' reads.

sqlive_compose::fragment!(
    StoredFileIds,
    r#"
    SELECT f.id AS file_id FROM coven_files AS f
"#
);

impl StoredFileIds {
    pub fn file_id(&self) -> &str {
        &self.file_id
    }
}
