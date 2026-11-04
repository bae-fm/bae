//! The list's search index: each release's shown text, folded, in a
//! trigram index under its placement row's number.

pub(super) const SQL: &str = include_str!("search.sql");
