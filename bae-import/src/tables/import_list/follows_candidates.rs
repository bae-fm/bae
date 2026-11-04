//! The placement following each release's stored import state: its draft,
//! album credits, provenance, link, verdict and found releases, failure, and
//! what is running for it.

pub(super) const SQL: &str = include_str!("follows_candidates.sql");

#[cfg(test)]
#[path = "follows_candidates_tests.rs"]
mod tests;
