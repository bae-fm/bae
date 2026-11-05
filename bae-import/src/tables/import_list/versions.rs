//! The versions of the tables the import list's order reads, which tell a
//! read whether its last read's rows still sit where they did.

pub(super) const SQL: &str = include_str!("versions.sql");

#[cfg(test)]
#[path = "versions_tests.rs"]
mod tests;
