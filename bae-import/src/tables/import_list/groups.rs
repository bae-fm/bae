//! Found's groups: each group's header, kept in line with its members and
//! dated by the newest, whether its folder is offered to be read as one
//! release, and whether a row at a watched folder's top level holds a nested
//! release.

pub(super) const SQL: &str = include_str!("groups.sql");

#[cfg(test)]
#[path = "groups_tests.rs"]
mod tests;
