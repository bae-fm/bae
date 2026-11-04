//! The placement following what the scan stores: its releases, their
//! folders' dates, the watched folders' order, the folders the person
//! skipped and a failed folder's tags.

pub(super) const SQL: &str = include_str!("follows_scans.sql");

#[cfg(test)]
#[path = "follows_scans_tests.rs"]
mod tests;
