//! The placement following what this device imported and the library
//! records its rows show: an import's origin, the artist and credit names a
//! draft prints, and an imported release's album.

pub(super) const SQL: &str = include_str!("follows_library.sql");

#[cfg(test)]
#[path = "follows_library_tests.rs"]
mod tests;
