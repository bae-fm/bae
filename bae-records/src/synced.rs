//! The record rows the store shares, the columns that reference coven's
//! files, and records' kinds of file with their cache budgets.

use coven::{FileColumn, Namespace, Provenance, RowIdentity, SyncedTable};

/// The person's own files of a release: audio, cue sheets, scans, logs.
pub const RELEASE_FILES_NAMESPACE: &str = "release_files";
/// A release's cover at 768 pixels, for grids and rows.
pub const COVER_BROWSE_NAMESPACE: &str = "cover_browse";
/// A release's cover at 1536 pixels, for the album panel, the full player and
/// Now Playing.
pub const COVER_LARGE_NAMESPACE: &str = "cover_large";
/// The picture a cover is made from when that picture is no file of the
/// release: the catalog image the person picked, or art embedded in an audio
/// file.
pub const COVER_ORIGINALS_NAMESPACE: &str = "cover_originals";
/// The picture bae keeps for an artist.
pub const ARTIST_IMAGES_NAMESPACE: &str = "artist_images";

/// The largest browse copy a cover row names, in bytes.
const COVER_BROWSE_LIMIT: u64 = 4 * 1024 * 1024;
/// The largest large copy a cover row names, in bytes.
const COVER_LARGE_LIMIT: u64 = 16 * 1024 * 1024;

/// The kinds of file records keep, each with this device's cache budget in
/// bytes. Each namespace evicts against its own budget, so audio pressure
/// never wipes the cover cache: `release_files` takes the bulk since audio
/// dominates, and each kind of picture keeps a reserved slice. A picture
/// downloads when it is first read and stays cached until its namespace's
/// budget evicts it; pictures are not pinned.
///
/// The cover budgets: a browse copy is about 100 KB, so the 1 GiB of
/// `cover_browse` holds the copies of 10,000 albums, twice a 5,000-album
/// library's, and a device downloads a large copy (about 400 KB) only for a
/// cover shown large, so 1 GiB of `cover_large` holds about 2,500 of them. A
/// cover's original is read only by the viewer, so `cover_originals` keeps
/// 512 MiB, about 250 originals of 2 MB.
pub fn namespaces() -> Vec<Namespace> {
    vec![
        Namespace::new(
            RELEASE_FILES_NAMESPACE,
            Provenance::UserProvided,
            crate::CacheLimit::DEFAULT.bytes(),
        ),
        Namespace::new(COVER_BROWSE_NAMESPACE, Provenance::AppProvided, 1024 * 1024 * 1024),
        Namespace::new(COVER_LARGE_NAMESPACE, Provenance::AppProvided, 1024 * 1024 * 1024),
        Namespace::new(COVER_ORIGINALS_NAMESPACE, Provenance::AppProvided, 512 * 1024 * 1024),
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
        SyncedTable::new("album_artists", RowIdentity::IndependentUuid),
        SyncedTable::new("releases", RowIdentity::IndependentUuid),
        SyncedTable::new("release_records", RowIdentity::SharedKey),
        SyncedTable::new("release_mediums", RowIdentity::IndependentUuid)
            .key_columns(["id", "release_id"]),
        SyncedTable::new("release_media", RowIdentity::IndependentUuid),
        SyncedTable::new("release_barcodes", RowIdentity::IndependentUuid),
        SyncedTable::new("labels", RowIdentity::SharedKey),
        SyncedTable::new("label_merges", RowIdentity::SharedKey),
        SyncedTable::new("release_labels", RowIdentity::IndependentUuid),
        SyncedTable::new("tracks", RowIdentity::IndependentUuid),
        SyncedTable::new("track_artists", RowIdentity::IndependentUuid),
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
        // A carved track is one row, keyed by the track's id.
        SyncedTable::new("carved_tracks", RowIdentity::IndependentUuid).key_columns(["track_id"]),
        // The write naming a copy waits until its file is stored. An original
        // can pass a bound file's limit, so its reference trails the row.
        // A copy in a picture with transparency is a PNG, which is larger
        // than a JPEG, so the limits leave room for one.
        SyncedTable::new("covers", RowIdentity::SharedKey)
            .file_column(FileColumn::new("browse_file").bound_with_limit(COVER_BROWSE_LIMIT))
            .file_column(FileColumn::new("large_file").bound_with_limit(COVER_LARGE_LIMIT))
            .file_column(FileColumn::new("original_file")),
        SyncedTable::new("artist_images", RowIdentity::SharedKey)
            .file_column(FileColumn::new("file").bound()),
    ]
}

#[cfg(test)]
#[path = "synced_tests.rs"]
mod tests;
