//! The read fragments this crate offers to other crates' reads.

sqlive_compose::fragment!(
    SavePresetNames,
    r#"
    SELECT p.name, p.position FROM save_preset AS p
"#
);

impl SavePresetNames {
    pub fn name(&self) -> &str {
        &self.name
    }
    pub fn position(&self) -> i64 {
        self.position
    }
}
