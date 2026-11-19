//! The list's counts: how many placement rows each tab holds of each item,
//! Found's releases in each entry of its filter, and each Found group's
//! members in each entry, kept in the write that moves a placement row.

pub(super) const SQL: &str = include_str!("counts.sql");

#[cfg(test)]
#[path = "counts_tests.rs"]
mod tests;
