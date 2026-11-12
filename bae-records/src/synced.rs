//! The record rows the store shares, the columns that reference coven's
//! files, and records' kinds of file with their cache budgets.

use coven::{FileColumn, Namespace, Provenance, RowIdentity, SyncedTable};

/// The person's own files of a release: audio, cue sheets, scans, logs.
pub const RELEASE_FILES_NAMESPACE: &str = "release_files";
/// The album cover bae makes for a release.
pub const COVERS_NAMESPACE: &str = "covers";
/// The picture bae keeps for an artist.
pub const ARTIST_IMAGES_NAMESPACE: &str = "artist_images";

/// The kinds of file records keep, each with this device's cache budget in
/// bytes. Each namespace evicts against its own budget, so audio pressure
/// never wipes the cover cache: `release_files` takes the bulk since audio
/// dominates, and `covers` (grid art) and `artist_images` each keep a reserved
/// slice. A cover downloads when it is first read and stays cached until its
/// namespace's budget evicts it; covers are not pinned.
pub fn namespaces() -> Vec<Namespace> {
    vec![
        Namespace::new(
            RELEASE_FILES_NAMESPACE,
            Provenance::UserProvided,
            crate::CacheLimit::DEFAULT.bytes(),
        ),
        Namespace::new(
            COVERS_NAMESPACE,
            Provenance::AppProvided,
            512 * 1024 * 1024, // 512 MiB
        ),
        Namespace::new(ARTIST_IMAGES_NAMESPACE, Provenance::AppProvided, 256 * 1024 * 1024),
    ]
}

/// The library rows shared with the store. Local audio paths are deliberately
/// absent: a row references a file by its coven file id, and coven alone
/// knows where its bytes are.
pub fn synced_tables() -> Vec<SyncedTable> {
    vec![
        SyncedTable::new("artists", RowIdentity::SharedKey),
        SyncedTable::new("artist_merges", RowIdentity::SharedKey),
        SyncedTable::new("artist_credits", RowIdentity::SharedKey)
            .key_columns(["artist_id", "name_key"]),
        SyncedTable::new("albums", RowIdentity::SharedKey),
        SyncedTable::new("album_artists", RowIdentity::SharedKey),
        SyncedTable::new("releases", RowIdentity::IndependentUuid),
        SyncedTable::new("release_records", RowIdentity::SharedKey),
        SyncedTable::new("release_mediums", RowIdentity::SharedKey),
        SyncedTable::new("release_media", RowIdentity::SharedKey),
        SyncedTable::new("release_barcodes", RowIdentity::SharedKey),
        SyncedTable::new("labels", RowIdentity::SharedKey),
        SyncedTable::new("label_merges", RowIdentity::SharedKey),
        SyncedTable::new("release_labels", RowIdentity::SharedKey),
        SyncedTable::new("tracks", RowIdentity::IndependentUuid),
        SyncedTable::new("track_artists", RowIdentity::SharedKey),
        SyncedTable::new("works", RowIdentity::SharedKey),
        SyncedTable::new("work_artists", RowIdentity::SharedKey),
        SyncedTable::new("work_parts", RowIdentity::SharedKey),
        SyncedTable::new("track_works", RowIdentity::IndependentUuid),
        SyncedTable::new("release_artist_roles", RowIdentity::IndependentUuid),
        SyncedTable::new("track_artist_roles", RowIdentity::IndependentUuid),
        SyncedTable::new("release_files", RowIdentity::IndependentUuid)
            .file_column(FileColumn::new("file")),
        SyncedTable::new("audio_formats", RowIdentity::IndependentUuid),
        SyncedTable::new("audio_format_segments", RowIdentity::IndependentUuid),
        // A segment's points are one row, keyed by the segment's id.
        SyncedTable::new("audio_segment_scrub_points", RowIdentity::IndependentUuid)
            .key_columns(["audio_format_segment_id"]),
        // An image reaches other devices with its row: the write naming it
        // waits until its file is stored.
        SyncedTable::new("covers", RowIdentity::SharedKey)
            .file_column(FileColumn::new("file").bound()),
        SyncedTable::new("artist_images", RowIdentity::SharedKey)
            .file_column(FileColumn::new("file").bound()),
    ]
}

#[cfg(test)]
#[path = "synced_tests.rs"]
mod tests;
