//! bae's synced-schema migration ladder, registered on the coven builder.
//! coven runs each migration whose version exceeds the db's `PRAGMA user_version`
//! at open; the applied version becomes the wire `schema_version`.

/// The ordered migration ladder. Versions are 1-based and contiguous.
pub fn all() -> Vec<coven::Migration> {
    vec![
        coven::Migration::sql(1, "initial", include_str!("../migrations/001_initial.sql")),
        coven::Migration::sql(
            2,
            "blob_content_hash",
            include_str!("../migrations/002_blob_content_hash.sql"),
        ),
    ]
}
