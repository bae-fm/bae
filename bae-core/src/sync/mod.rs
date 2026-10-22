//! bae's sync boundary around coven.
//!
//! coven owns sync, keys, storage and database connections. bae declares its
//! shared rows, their files and cache budgets here.

// The sync substrate lives in coven; these resolve `crate::sync::<item>`
// unchanged. Cloud file operations use the exact stored file reference.
pub use coven::{decode_code_info, restore_from_code};

pub mod membership;
pub mod restore_code;

#[cfg(test)]
mod identity_tests;
#[cfg(test)]
pub(crate) mod test_devices;

use coven::{FileDecl, Provenance, RowIdentity, SyncedTable};

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

impl S3ConfigData {
    /// The coven location these settings name, and the key that reaches it.
    /// No key prefix is the bucket's root.
    pub fn into_location(
        self,
    ) -> Result<(coven::StorageConfig, coven::S3Credentials), url::ParseError> {
        let endpoint = self.endpoint.map(|value| value.parse()).transpose()?;
        Ok((
            coven::StorageConfig::S3 {
                bucket: self.bucket,
                region: self.region,
                endpoint,
                prefix: self.key_prefix.unwrap_or_default(),
                profile: self.profile,
            },
            coven::S3Credentials {
                access_key_id: self.access_key,
                secret_access_key: coven::SecretText::new(self.secret_key),
            },
        ))
    }
}

/// Cloud namespace for release-file (audio/image/text/…) files — the user's own
/// imported files. coven keys them under `release_files/…` and segments their
/// cache + budget by this name.
pub const RELEASE_FILES_NAMESPACE: &str = "release_files";
/// Cloud namespace for the bae-produced album cover file (1:1 with a release).
pub const COVERS_NAMESPACE: &str = "covers";
/// Cloud namespace for the bae-produced artist image file (1:1 with an artist).
pub const ARTIST_IMAGES_NAMESPACE: &str = "artist_images";
/// Profile avatars, fetched with their people rows.
pub const PEOPLE_NAMESPACE: &str = "people";

/// This device's cache budget (bytes) per namespace. Each namespace evicts
/// against its own budget, so audio pressure never wipes the cover cache:
/// `release_files` takes the bulk since audio dominates, and `covers` (grid art)
/// and `artist_images` each keep a reserved slice. A cover downloads when it is
/// first read and stays cached until its namespace's budget evicts it; covers
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
        SyncedTable::new("track_artists", RowIdentity::SharedKey),
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
            )
            .with_size_column("file_size"),
        ),
        SyncedTable::new("audio_formats", RowIdentity::IndependentUuid),
        SyncedTable::new("audio_format_segments", RowIdentity::IndependentUuid),
        SyncedTable::new("covers", RowIdentity::SharedKey).carries_files(
            FileDecl::new(
                COVERS_NAMESPACE,
                CACHE_BUDGETS[1].1,
                Provenance::AppProvided,
            )
            .with_size_column("file_size"),
        ),
        SyncedTable::new("artist_images", RowIdentity::SharedKey).carries_files(
            FileDecl::new(
                ARTIST_IMAGES_NAMESPACE,
                CACHE_BUDGETS[2].1,
                Provenance::AppProvided,
            )
            .with_size_column("file_size"),
        ),
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
            .carries_files(
                FileDecl::new(
                    PEOPLE_NAMESPACE,
                    CACHE_BUDGETS[3].1,
                    Provenance::AppProvided,
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
                "history",
                "ratings",
                "playlists",
                "playlist_entries",
            ])
        );
        for table in &tables {
            let shared = matches!(
                table.name(),
                "artists"
                    | "artist_merges"
                    | "albums"
                    | "album_artists"
                    | "release_records"
                    | "track_artists"
                    | "works"
                    | "work_artists"
                    | "work_parts"
                    | "covers"
                    | "artist_images"
                    | "people"
                    | "ratings"
                    | "playlist_entries"
            );
            assert_eq!(
                table.identity(),
                if shared {
                    &RowIdentity::SharedKey
                } else {
                    &RowIdentity::IndependentUuid
                }
            );
            let keys: &[&str] = if table.name() == "playlist_entries" {
                &["playlist_id", "position_id"]
            } else {
                &["id"]
            };
            assert_eq!(table.get_key_columns(), keys);
            assert_eq!(
                table.get_audience_column(),
                matches!(table.name(), "playlists" | "history").then_some("audience")
            );
            assert_eq!(
                table.get_audience_from(),
                (table.name() == "playlist_entries").then_some("playlist_id")
            );
            assert_eq!(
                table.is_owner_only(),
                matches!(
                    table.name(),
                    "people" | "ratings" | "history" | "playlists" | "playlist_entries"
                )
            );
            let expected = match table.name() {
                "release_files" => Some((
                    RELEASE_FILES_NAMESPACE,
                    20 * 1024 * 1024 * 1024,
                    Provenance::UserProvided,
                    "id",
                    false,
                )),
                "covers" => Some((
                    COVERS_NAMESPACE,
                    512 * 1024 * 1024,
                    Provenance::AppProvided,
                    "id",
                    false,
                )),
                "artist_images" => Some((
                    ARTIST_IMAGES_NAMESPACE,
                    256 * 1024 * 1024,
                    Provenance::AppProvided,
                    "id",
                    false,
                )),
                "people" => Some((
                    PEOPLE_NAMESPACE,
                    64 * 1024 * 1024,
                    Provenance::AppProvided,
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
            if let Some((namespace, budget, provenance, id, once)) = expected {
                let file = table.files().unwrap();
                assert_eq!(file.namespace(), namespace);
                assert_eq!(file.cache_budget(), budget);
                assert_eq!(file.provenance(), &provenance);
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

    /// Tables whose rows never leave this device. A table the schema creates
    /// is either declared in `synced_tables()` or named here, so a new table
    /// is a decision rather than a row that silently stays on one device.
    const DEVICE_LOCAL_TABLES: &[&str] = &[
        // This device's own copies: original paths of local releases, the
        // folder each release was imported from, plays before a person
        // exists, and what this device was playing.
        "local_release_files",
        "local_release_origin",
        "local_history",
        "playback_state",
        // Import: the folders this device watches, what scanning found in
        // them, and each candidate's state while it is imported.
        "watched_import_folders",
        "release_grouping",
        "release_grouping_member",
        "skipped_import_candidates",
        "folder_discovery",
        "folder_scan_generation_sequence",
        "folder_scan_roots",
        "folder_scan_directory",
        "scan_candidate",
        "scan_candidate_file",
        "scan_candidate_tag_snapshot",
        "scan_candidate_file_tag",
        "scan_candidate_part",
        "scan_skipped_file",
        "scan_skipped_tags",
        "scan_sidecar",
        "scan_sidecar_file",
        "scan_cue_sheet",
        "scan_cue_track",
        "scan_cue_index",
        "scan_sheet_audio_file",
        "import_candidate_state",
        "import_candidate_folder",
        "import_candidate_edit",
        "import_candidate_album_artist_assignment",
        "import_candidate_track",
        "import_candidate_track_artist_assignment",
        "import_candidate_track_role",
        "import_candidate_release_role",
        "import_candidate_work",
        "import_candidate_work_composer",
        "import_candidate_work_part",
        "import_candidate_track_work",
        "import_candidate_sheet_disc",
        "import_candidate_sheet_reference",
        "import_candidate_cover",
        "import_candidate_cover_copy",
        "import_candidate_cover_option",
        "import_candidate_cover_option_copy",
        "import_candidate_remote_cover_asset",
        "import_candidate_artist_asset",
        "import_candidate_asset_preparation",
        "import_candidate_draft_provenance",
        "import_candidate_release_link",
        "import_candidate_release_link_partner",
        "import_candidate_release_link_album",
        "import_candidate_session",
        "import_candidate_failure",
        "import_candidate_artist_identity_conflict",
        "import_candidate_artist_identity_choice",
        "import_candidate_signals",
        "import_candidate_signal_value",
        "import_candidate_text_line",
        "import_candidate_lookup_choices",
        "import_candidate_chosen_catalog",
        "import_candidate_discounted_catalog",
        "import_candidate_excluded_barcode",
        "import_candidate_verdict",
        "import_candidate_match",
        "import_candidate_match_barcode",
        "import_candidate_match_cover_copy",
        "import_candidate_match_link",
        "import_candidate_match_album_link",
        "import_candidate_match_medium",
        // Catalog releases this device fetched; any device can fetch them.
        "source_release",
        "source_release_album_artist",
        "source_release_link",
        "source_release_album_link",
        "source_release_format",
        "source_release_record",
        "release_group_album_link",
        "source_release_cover",
        "source_release_cover_copy",
        "source_release_archive_group",
        "source_release_role",
        "source_release_medium",
        "source_release_medium_disc",
        "source_release_entry",
        "source_release_entry_credit",
        "source_release_entry_role",
        "source_release_work",
        "source_release_work_composer",
        "source_release_unfetched",
    ];

    #[test]
    fn every_schema_table_is_synced_or_declared_device_local() {
        let create = regex::Regex::new(r"(?m)^\s*CREATE TABLE (?:IF NOT EXISTS )?(\w+)").unwrap();
        let schema: BTreeSet<&str> = create
            .captures_iter(include_str!("../../migrations/001_initial.sql"))
            .map(|captures| captures.get(1).unwrap().as_str())
            .collect();
        let tables = synced_tables();
        let synced: BTreeSet<&str> = tables.iter().map(SyncedTable::name).collect();
        let local: BTreeSet<&str> = DEVICE_LOCAL_TABLES.iter().copied().collect();
        assert_eq!(
            local.len(),
            DEVICE_LOCAL_TABLES.len(),
            "a device-local table is listed twice"
        );
        let undeclared: Vec<_> = schema
            .iter()
            .filter(|table| !synced.contains(*table) && !local.contains(*table))
            .collect();
        assert!(
            undeclared.is_empty(),
            "tables neither synced nor listed as device-local: {undeclared:?}"
        );
        let both: Vec<_> = synced.intersection(&local).collect();
        assert!(
            both.is_empty(),
            "tables both synced and device-local: {both:?}"
        );
        let missing: Vec<_> = synced
            .union(&local)
            .filter(|table| !schema.contains(*table))
            .collect();
        assert!(
            missing.is_empty(),
            "declared tables the schema doesn't create: {missing:?}"
        );
    }
}
