//! bae's library store: the `Database` over coven's handle and the entry
//! points every read and write runs through, the schema (its one migration,
//! the synced tables and the SQL functions it declares), the library's
//! records and the row writers that keep them, list windows, and the people,
//! collections and history a library holds.

pub mod admin_change;
pub mod client;
pub mod collections;
pub mod credits;
mod error;
pub use error::DbError;
pub mod history;
pub mod identity;
pub mod membership;
pub mod migrations;
mod models;
pub mod people;
pub mod schema;
pub use schema::configure_store;
mod resolved_track_audio;
pub use resolved_track_audio::{
    ResolvedTrackAudio, ResolvedTrackAudioSegment, TrackAudioError, TrackAudioMeta,
};
pub mod source_audio;
pub mod stored_name;
pub mod synced;
#[cfg(any(test, feature = "test-utils"))]
pub mod test_devices;
#[cfg(any(test, feature = "test-utils"))]
pub mod test_fixtures;
pub mod watched_path;
pub use client::list_windows::{LibraryBrowseWindow, LibraryPageWindow, LibraryPageWindows};
#[cfg(any(test, feature = "test-utils"))]
pub use client::resolved_in_an_empty_library;
pub use watched_path::{RelativeFolder, WatchedPathError};
// Only an import names its own artist credits from outside bae-db.
pub use client::AlbumFields;
#[cfg(not(any(target_os = "ios", target_os = "android")))]
pub use client::ArtistCredits;
pub use client::FileRow;
pub use client::PendingRow;
#[cfg(not(any(target_os = "ios", target_os = "android")))]
pub use client::RecordsChange;
#[cfg(not(any(target_os = "ios", target_os = "android")))]
pub use client::ReleaseEditWrite;
pub use client::ReleaseMetadataRows;
pub use client::StoredTransition;
pub use client::{Database, IdentityWriteError, ReleaseDeletion, StorageSetupSteps};
#[cfg(all(
    any(test, feature = "test-utils", debug_assertions),
    not(any(target_os = "ios", target_os = "android"))
))]
pub use client::{SeededAlbum, SeededAudio};
pub use models::*;

#[cfg(test)]
mod tests {
    /// `.cargo/config.toml` builds SQLite without its process-wide memory
    /// statistics, which would serialize every allocation through one mutex.
    #[test]
    fn sqlite_is_built_without_memory_statistics() {
        let connection = rusqlite::Connection::open_in_memory().unwrap();
        let built_without: bool = connection
            .query_row("SELECT sqlite_compileoption_used('DEFAULT_MEMSTATUS=0')", [], |row| {
                row.get(0)
            })
            .unwrap();
        assert!(built_without, "SQLite was built with memory statistics on");
    }

    /// `.cargo/config.toml` keeps a statement's journal in memory up to
    /// 124 KiB, so the FTS5 segment merges bae's triggers run inside a write
    /// don't send every later statement's journal to a file.
    #[test]
    fn sqlite_keeps_a_statement_journal_in_memory_up_to_124_kib() {
        let connection = rusqlite::Connection::open_in_memory().unwrap();
        let spill: bool = connection
            .query_row("SELECT sqlite_compileoption_used('STMTJRNL_SPILL=126976')", [], |row| {
                row.get(0)
            })
            .unwrap();
        assert!(spill, "SQLite was built with another statement journal spill");
    }
}
