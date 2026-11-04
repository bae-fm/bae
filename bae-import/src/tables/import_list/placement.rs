//! The import list's placement: where every row of the list sits, its
//! indexes for each order, and the rules that place a release
//! (`import_list_entry`), which `import_list_replace` places one release by.

pub(super) const SQL: &str = include_str!("placement.sql");

#[cfg(test)]
#[path = "placement_tests.rs"]
pub(crate) mod tests;
