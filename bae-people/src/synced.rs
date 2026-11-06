//! The people rows the store shares, the column that references coven's
//! files, and the kind of file people keep with its cache budget.

use coven::{FileColumn, Namespace, Provenance, RowIdentity, SyncedTable};

/// Profile pictures.
pub const AVATARS_NAMESPACE: &str = "avatars";

/// The kind of file people keep: profile pictures, with this device's cache
/// budget in bytes. A picture downloads when it is first read and stays cached
/// until the namespace's budget evicts it; pictures are not pinned.
pub fn namespaces() -> Vec<Namespace> {
    vec![Namespace::new(
        AVATARS_NAMESPACE,
        Provenance::AppProvided,
        64 * 1024 * 1024, // 64 MiB
    )]
}

/// The people rows shared with the store.
pub fn synced_tables() -> Vec<SyncedTable> {
    vec![
        SyncedTable::new("history", RowIdentity::IndependentUuid)
            .owner_only()
            .audience_column("audience"),
        SyncedTable::new("ratings", RowIdentity::SharedKey).owner_only(),
        SyncedTable::new("playlists", RowIdentity::IndependentUuid)
            .owner_only()
            .audience_column("audience"),
        SyncedTable::new("playlist_entries", RowIdentity::SharedKey)
            .key_columns(["playlist_id", "position_id"])
            .owner_only()
            .audience_from("playlist_id"),
        SyncedTable::new("people", RowIdentity::SharedKey)
            .owner_only()
            .file_column(FileColumn::new("avatar_id").bound()),
    ]
}

#[cfg(test)]
#[path = "synced_tests.rs"]
mod tests;
