//! bae's sync boundary around coven.
//!
//! coven owns sync, keys, storage and database connections. bae declares its
//! shared rows, their files and cache budgets here.

// The sync substrate lives in coven; these resolve `crate::sync::<item>`
// unchanged. Cloud blob operations use the exact stored blob reference.
pub use coven::{decode_code_info, restore_from_code};

pub mod membership;

#[cfg(test)]
mod identity_tests;
#[cfg(test)]
pub(crate) mod test_devices;

use coven::{CacheFill, Provenance};
use coven::{FileDecl, RowIdentity, SyncedTable};

/// The localized message key for a visible post-open artwork-loading state.
/// Terminal success and the idle state have no banner.
pub fn eager_cache_fill_title_key(status: &coven::EagerCacheFillStatus) -> Option<&'static str> {
    match status {
        coven::EagerCacheFillStatus::Idle => None,
        coven::EagerCacheFillStatus::Downloading(_) => Some("core.artwork_cache.downloading"),
        coven::EagerCacheFillStatus::Cancelled(_) => Some("core.artwork_cache.cancelled"),
    }
}

/// S3 configuration data for save_s3_config.
pub struct S3ConfigData {
    pub bucket: String,
    pub region: String,
    pub endpoint: Option<String>,
    pub key_prefix: Option<String>,
    pub access_key: String,
    pub secret_key: String,
    pub profile: Option<coven::S3Profile>,
}

/// Cloud namespace for release-file (audio/image/text/…) blobs — the user's own
/// imported files. coven keys them under `release_files/…` and segments their
/// cache + budget by this name.
pub const RELEASE_FILES_NAMESPACE: &str = "release_files";
/// Cloud namespace for the bae-produced album cover blob (1:1 with a release).
pub const COVERS_NAMESPACE: &str = "covers";
/// Cloud namespace for the bae-produced artist image blob (1:1 with an artist).
pub const ARTIST_IMAGES_NAMESPACE: &str = "artist_images";
/// Profile avatars, fetched with their people rows.
pub const PEOPLE_NAMESPACE: &str = "people";

/// This device's cache budget (bytes) per namespace. Each namespace evicts
/// against its own budget, so audio pressure never wipes the cover cache:
/// `release_files` takes the bulk since audio dominates, and `covers` (grid art)
/// and `artist_images` each keep a reserved slice. A `CacheEager` cover evicted
/// under pressure shows a placeholder and re-fetches on the next pull — covers
/// are not pinned.
pub const CACHE_BUDGETS: [(&str, u64); 4] = [
    (RELEASE_FILES_NAMESPACE, 20 * 1024 * 1024 * 1024), // 20 GiB
    (COVERS_NAMESPACE, 512 * 1024 * 1024),              // 512 MiB
    (ARTIST_IMAGES_NAMESPACE, 256 * 1024 * 1024),
    (PEOPLE_NAMESPACE, 64 * 1024 * 1024), // 64 MiB
];

/// The library rows shared with the store. Local audio paths are deliberately
/// absent: only attaching a file makes its bytes available to other devices.
pub fn synced_tables() -> Vec<SyncedTable> {
    vec![
        SyncedTable::new("artists", RowIdentity::SharedKey),
        SyncedTable::new("artist_merges", RowIdentity::SharedKey),
        SyncedTable::new("albums", RowIdentity::SharedKey),
        SyncedTable::new("album_artists", RowIdentity::SharedKey),
        SyncedTable::new("releases", RowIdentity::IndependentUuid),
        SyncedTable::new("release_records", RowIdentity::SharedKey),
        SyncedTable::new("tracks", RowIdentity::IndependentUuid),
        SyncedTable::new("track_artists", RowIdentity::IndependentUuid),
        SyncedTable::new("works", RowIdentity::SharedKey),
        SyncedTable::new("work_artists", RowIdentity::SharedKey),
        SyncedTable::new("work_parts", RowIdentity::SharedKey),
        SyncedTable::new("track_works", RowIdentity::IndependentUuid),
        SyncedTable::new("release_artist_roles", RowIdentity::IndependentUuid),
        SyncedTable::new("track_artist_roles", RowIdentity::IndependentUuid),
        SyncedTable::new("release_files", RowIdentity::IndependentUuid).carries_files(
            FileDecl::new(
                RELEASE_FILES_NAMESPACE,
                CACHE_BUDGETS[0].1,
                Provenance::UserProvided,
                CacheFill::CacheLazy,
            )
            .with_size_column("file_size")
            .write_once(),
        ),
        SyncedTable::new("audio_formats", RowIdentity::IndependentUuid),
        SyncedTable::new("audio_format_segments", RowIdentity::IndependentUuid),
        SyncedTable::new("covers", RowIdentity::SharedKey).carries_files(
            FileDecl::new(
                COVERS_NAMESPACE,
                CACHE_BUDGETS[1].1,
                Provenance::AppProvided,
                CacheFill::CacheEager,
            )
            .with_id_column("blob_id")
            .with_size_column("file_size"),
        ),
        SyncedTable::new("artist_images", RowIdentity::SharedKey).carries_files(
            FileDecl::new(
                ARTIST_IMAGES_NAMESPACE,
                CACHE_BUDGETS[2].1,
                Provenance::AppProvided,
                CacheFill::CacheEager,
            )
            .with_id_column("blob_id")
            .with_size_column("file_size"),
        ),
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
            .carries_files(
                FileDecl::new(
                    PEOPLE_NAMESPACE,
                    CACHE_BUDGETS[3].1,
                    Provenance::AppProvided,
                    CacheFill::CacheEager,
                )
                .with_id_column("avatar_id")
                .with_size_column("file_size"),
            ),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeSet;

    #[test]
    fn library_declarations_match_the_store_audience_and_file_contract() {
        let tables = synced_tables();
        let names: BTreeSet<_> = tables.iter().map(SyncedTable::name).collect();
        assert_eq!(
            names,
            BTreeSet::from([
                "artists",
                "artist_merges",
                "albums",
                "album_artists",
                "releases",
                "release_records",
                "tracks",
                "track_artists",
                "works",
                "work_artists",
                "work_parts",
                "track_works",
                "release_artist_roles",
                "track_artist_roles",
                "release_files",
                "audio_formats",
                "audio_format_segments",
                "covers",
                "artist_images",
                "people",
                "ratings",
                "playlists",
                "playlist_entries",
            ])
        );
        for table in &tables {
            let keys: &[&str] = if table.name() == "playlist_entries" {
                &["playlist_id", "position_id"]
            } else {
                &["id"]
            };
            assert_eq!(table.get_key_columns(), keys);
            assert_eq!(
                table.get_audience_column(),
                (table.name() == "playlists").then_some("audience")
            );
            assert_eq!(
                table.get_audience_from(),
                (table.name() == "playlist_entries").then_some("playlist_id")
            );
            assert_eq!(
                table.is_owner_only(),
                matches!(
                    table.name(),
                    "people" | "ratings" | "playlists" | "playlist_entries"
                )
            );
            let expected = match table.name() {
                "release_files" => Some((
                    RELEASE_FILES_NAMESPACE,
                    20 * 1024 * 1024 * 1024,
                    Provenance::UserProvided,
                    CacheFill::CacheLazy,
                    "id",
                    true,
                )),
                "covers" => Some((
                    COVERS_NAMESPACE,
                    512 * 1024 * 1024,
                    Provenance::AppProvided,
                    CacheFill::CacheEager,
                    "blob_id",
                    false,
                )),
                "artist_images" => Some((
                    ARTIST_IMAGES_NAMESPACE,
                    256 * 1024 * 1024,
                    Provenance::AppProvided,
                    CacheFill::CacheEager,
                    "blob_id",
                    false,
                )),
                "people" => Some((
                    PEOPLE_NAMESPACE,
                    64 * 1024 * 1024,
                    Provenance::AppProvided,
                    CacheFill::CacheEager,
                    "avatar_id",
                    false,
                )),
                _ => None,
            };
            assert_eq!(
                table.files().is_some(),
                expected.is_some(),
                "{}",
                table.name()
            );
            if let Some((namespace, budget, provenance, fill, id, once)) = expected {
                let file = table.files().unwrap();
                assert_eq!(file.namespace(), namespace);
                assert_eq!(file.cache_budget(), budget);
                assert_eq!(file.provenance(), &provenance);
                assert_eq!(file.fill(), &fill);
                assert_eq!(file.id_column(), id);
                assert_eq!(file.size_column(), "file_size");
                assert_eq!(file.hash_column(), "hash");
                assert_eq!(file.location_column(), "location");
                assert_eq!(file.is_write_once(), once);
            }
        }
        assert_eq!(
            tables
                .iter()
                .find(|t| t.name() == "covers")
                .unwrap()
                .identity(),
            &RowIdentity::SharedKey
        );
    }
}
