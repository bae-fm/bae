//! bae's synced-schema migration ladder; coven applies versions above `PRAGMA user_version` at open.

/// The collation bae's schema names for folder paths in the order a person
/// reads them.
pub(crate) const NATURAL_ORDER: &str = "natural_order";

/// The version of [`natural_order`] coven records. A change to what it
/// compares bumps it, and coven rebuilds the indexes that use it on the next
/// writable open.
pub(crate) const NATURAL_ORDER_VERSION: u32 = 1;

/// Natural order: "Disc 2" before "Disc 10" and letters without regard to
/// case, then by bytes where that finds two strings alike, so two different
/// strings never tie.
pub(crate) fn natural_order(left: &str, right: &str) -> std::cmp::Ordering {
    natord::compare_ignore_case(left, right).then_with(|| left.cmp(right))
}

/// The ordered migration ladder. Versions are 1-based and contiguous.
pub fn all() -> Vec<coven::Migration> {
    vec![coven::Migration::addition(
        1,
        "initial",
        include_str!("../migrations/001_initial.sql"),
    )]
}
