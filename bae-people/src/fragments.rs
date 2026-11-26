//! The read fragments this crate offers to other crates' reads.

sqlive_compose::fragment!(
    PersonNames,
    r#"
    SELECT p.id AS person_id, p.name FROM people AS p
"#
);

impl PersonNames {
    pub fn person_id(&self) -> &str {
        &self.person_id
    }
    pub fn name(&self) -> &str {
        &self.name
    }
}
