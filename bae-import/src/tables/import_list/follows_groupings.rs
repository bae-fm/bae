//! The placement following the releases read from several folders picked
//! together, and the folders each takes in.

pub(super) const SQL: &str = include_str!("follows_groupings.sql");

#[cfg(test)]
#[path = "follows_groupings_tests.rs"]
mod tests;
