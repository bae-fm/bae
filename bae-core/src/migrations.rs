//! bae's synced-schema migration ladder; coven applies versions above `PRAGMA user_version` at open.

/// The ordered migration ladder. Versions are 1-based and contiguous.
pub fn all() -> Vec<coven::Migration> {
    vec![coven::Migration::sql(
        1,
        "initial",
        include_str!("../migrations/001_initial.sql"),
    )]
}
