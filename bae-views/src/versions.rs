//! The versions of the tables the library's paged lists read, which tell a
//! read whether its last read's rows still sit where they did.
//!
//! Its table and the triggers writing it are `versions.sql`.

/// The SQL creating this part's table and the triggers that advance it.
pub(super) const SQL: &str = include_str!("versions.sql");

pub(super) const TABLE: &str = "library_list_versions";
