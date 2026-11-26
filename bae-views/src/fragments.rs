//! The read fragments this crate offers to other crates' reads.

sqlive_compose::fragment!(
    UploadBatchReleases,
    r#"
    SELECT b.release_id FROM upload_batch AS b
"#
);

impl UploadBatchReleases {
    pub fn release_id(&self) -> &str {
        &self.release_id
    }
}
