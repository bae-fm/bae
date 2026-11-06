//! The versions of the tables the import list's order reads, which tell a
//! read whether its last read's rows still sit where they did. bae-library,
//! whose uploads queue a release's files, tests the version that moves.

pub(super) const SQL: &str = include_str!("versions.sql");
