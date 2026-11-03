pub mod admin_change;
pub(crate) mod client;
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
pub(crate) mod schema;
pub use schema::configure_store;
mod resolved_track_audio;
#[cfg(desktop)]
pub(crate) use resolved_track_audio::TrackAudioMeta;
pub use resolved_track_audio::{ResolvedTrackAudio, ResolvedTrackAudioSegment, TrackAudioError};
pub mod source_audio;
pub mod stored_name;
pub mod synced;
#[cfg(test)]
pub(crate) mod test_devices;
pub(crate) mod watched_path;
pub use watched_path::{RelativeFolder, WatchedPathError};
// Only an import names its own artist credits from outside `db`.
pub use client::list_windows::{LibraryBrowseWindow, LibraryPageWindow, LibraryPageWindows};
#[cfg(test)]
pub(crate) use client::resolved_in_an_empty_library;
#[cfg(not(any(target_os = "ios", target_os = "android")))]
pub(crate) use client::ArtistCredits;
pub(crate) use client::FileRow;
pub(crate) use client::PendingRow;
#[cfg(not(any(target_os = "ios", target_os = "android")))]
pub(crate) use client::RecordsChange;
pub(crate) use client::ReleaseMetadataRows;
pub(crate) use client::StoredTransition;
#[cfg(not(any(target_os = "ios", target_os = "android")))]
pub(crate) use client::{AlbumFields, ReleaseEditWrite};
pub use client::{Database, IdentityWriteError, ReleaseDeletion, StorageSetupSteps};
#[cfg(all(
    any(test, feature = "test-utils", debug_assertions),
    not(any(target_os = "ios", target_os = "android"))
))]
pub(crate) use client::{SeededAlbum, SeededAudio};
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
