//! bae's synced-schema migration ladder, registered on the coven builder.
//! coven runs each migration whose version exceeds the db's `PRAGMA user_version`
//! at open; the applied version becomes the wire `schema_version`.

/// The ordered migration ladder. Versions are 1-based and contiguous.
/// Pre-1.0 the ladder stays a single migration: schema changes fold into the
/// initial one (`rm -rf ~/.bae` is the migration strategy) rather than
/// accreting steps nobody's deployed database needs.
pub fn all() -> Vec<coven::Migration> {
    vec![coven::Migration::sql(
        1,
        "initial",
        include_str!("../migrations/001_initial.sql"),
    )]
}
