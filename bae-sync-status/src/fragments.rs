//! The read fragments this crate offers to other crates' reads.

sqlive_compose::fragment!(
    StartFailureCategory,
    r#"
    SELECT f.category FROM sync_start_failure AS f
"#
);

impl StartFailureCategory {
    pub fn category(&self) -> &str {
        &self.category
    }
}
