//! bae's sync boundary around coven.
//!
//! coven owns sync, keys, storage and database connections. bae declares its
//! shared rows, the columns that reference coven's files, and its kinds of
//! file with their cache budgets here.

// The sync substrate lives in coven; these resolve `crate::sync::<item>`
// unchanged. Cloud file operations use the exact stored file reference.
pub use coven::{decode_code_info, restore_from_code};

pub mod membership;
pub mod restore_code;

#[cfg(test)]
mod identity_tests;
#[cfg(test)]
pub(crate) mod test_devices;

use coven::{FileColumn, Namespace, Provenance, RowIdentity, SyncedTable};

/// S3 configuration data for save_s3_config.
pub struct S3ConfigData {
    pub bucket: String,
    pub region: String,
    pub endpoint: Option<String>,
    /// The folder in the bucket the library lives under, parsed from what the
    /// person typed; slashes at its ends mean nothing, and empty is the
    /// bucket's root.
    pub key_prefix: coven::S3Prefix,
    pub access_key: String,
    pub secret_key: String,
    pub profile: Option<coven::S3Profile>,
}

impl S3ConfigData {
    /// The coven location these settings name, and the key that reaches it.
    pub fn into_location(
        self,
    ) -> Result<(coven::StorageConfig, coven::S3Credentials), url::ParseError> {
        let endpoint = self.endpoint.map(|value| value.parse()).transpose()?;
        Ok((
            coven::StorageConfig::S3 {
                bucket: self.bucket,
                region: self.region,
                endpoint,
                prefix: self.key_prefix,
                profile: self.profile,
            },
            coven::S3Credentials {
                access_key_id: self.access_key,
                secret_access_key: coven::SecretText::new(self.secret_key),
            },
        ))
    }
}

/// The person's own files of a release: audio, cue sheets, scans, logs.
pub const RELEASE_FILES_NAMESPACE: &str = "release_files";
/// The album cover bae makes for a release.
pub const COVERS_NAMESPACE: &str = "covers";
/// The picture bae keeps for an artist.
pub const ARTIST_IMAGES_NAMESPACE: &str = "artist_images";
/// Profile pictures.
pub const AVATARS_NAMESPACE: &str = "avatars";

/// bae's kinds of file, each with this device's cache budget in bytes. Each
/// namespace evicts against its own budget, so audio pressure never wipes the
/// cover cache: `release_files` takes the bulk since audio dominates, and
/// `covers` (grid art), `artist_images` and `avatars` each keep a reserved
/// slice. A cover downloads when it is first read and stays cached until its
/// namespace's budget evicts it; covers are not pinned.
pub fn namespaces() -> Vec<Namespace> {
    vec![
        Namespace::new(
            RELEASE_FILES_NAMESPACE,
            Provenance::UserProvided,
            20 * 1024 * 1024 * 1024, // 20 GiB
        ),
        Namespace::new(
            COVERS_NAMESPACE,
            Provenance::AppProvided,
            512 * 1024 * 1024, // 512 MiB
        ),
        Namespace::new(
            ARTIST_IMAGES_NAMESPACE,
            Provenance::AppProvided,
            256 * 1024 * 1024,
        ),
        Namespace::new(
            AVATARS_NAMESPACE,
            Provenance::AppProvided,
            64 * 1024 * 1024, // 64 MiB
        ),
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
        SyncedTable::new("covers", RowIdentity::SharedKey).file_column(FileColumn::new("file")),
        SyncedTable::new("artist_images", RowIdentity::SharedKey)
            .file_column(FileColumn::new("file")),
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
            .file_column(FileColumn::new("avatar_id")),
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
                "artist_credits",
                "albums",
                "album_artists",
                "releases",
                "release_records",
                "release_mediums",
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
                "audio_segment_scrub_points",
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
                    | "artist_credits"
                    | "albums"
                    | "album_artists"
                    | "release_records"
                    | "release_mediums"
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
            let keys: &[&str] = match table.name() {
                "playlist_entries" => &["playlist_id", "position_id"],
                "artist_credits" => &["artist_id", "name_key"],
                "audio_segment_scrub_points" => &["audio_format_segment_id"],
                _ => &["id"],
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
            let columns: Vec<(&str, bool)> = table
                .file_columns()
                .iter()
                .map(|column| (column.column(), column.is_write_once()))
                .collect();
            let expected: &[(&str, bool)] = match table.name() {
                "release_files" | "covers" | "artist_images" => &[("file", false)],
                "people" => &[("avatar_id", false)],
                _ => &[],
            };
            assert_eq!(columns, expected, "{}", table.name());
        }
        let declared = namespaces();
        let namespaces: Vec<(&str, &Provenance, u64)> = declared
            .iter()
            .map(|namespace| (namespace.name(), namespace.kind(), namespace.cache_budget()))
            .collect();
        assert_eq!(
            namespaces,
            [
                (
                    RELEASE_FILES_NAMESPACE,
                    &Provenance::UserProvided,
                    20 * 1024 * 1024 * 1024
                ),
                (
                    COVERS_NAMESPACE,
                    &Provenance::AppProvided,
                    512 * 1024 * 1024
                ),
                (
                    ARTIST_IMAGES_NAMESPACE,
                    &Provenance::AppProvided,
                    256 * 1024 * 1024
                ),
                (
                    AVATARS_NAMESPACE,
                    &Provenance::AppProvided,
                    64 * 1024 * 1024
                ),
            ]
        );
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
        // exists, and what this device plays: its queue and where it stood.
        "local_release_files",
        "local_release_origin",
        "local_history",
        "playback_queue",
        "playback_manual",
        "playback_context",
        "playback_state",
        // The releases this device is downloading, exporting or saving, one
        // at a time.
        "release_queues",
        "download_queue",
        "output_queue",
        // What library lists sort, filter and count by, kept from the synced
        // rows by local triggers.
        "artist_shown_as",
        "composer_work",
        "composer_release",
        "composer_unlinked_credit",
        "composer_listing",
        "album_listing",
        "album_artist_link",
        "artist_listing",
        "release_listing",
        "release_listing_totals",
        // How often each of those lists' rows have moved, which says whether
        // a list's read may start from where its last read found rows.
        "list_order_versions",
        // Library search's folded titles and names.
        "album_search",
        "track_search",
        "work_search",
        "artist_search",
        "artist_credit_search",
        "album_search_entry",
        "track_search_entry",
        "work_search_entry",
        "artist_search_entry",
        "artist_credit_search_entry",
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
        "import_candidate_medium",
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
        "import_candidate_album_pick",
        "import_candidate_cover_copy",
        "import_candidate_cover_option",
        "import_candidate_cover_option_copy",
        "import_candidate_remote_cover_asset",
        "import_candidate_draft_provenance",
        "import_candidate_release_link",
        "import_candidate_release_link_partner",
        "import_candidate_release_link_album",
        "import_candidate_session",
        "import_candidate_failure",
        "import_candidate_standing",
        "import_identification_batch",
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
        // Where each row of the import list sits, worked out from the rows
        // above.
        "import_list_placement",
        "import_list_tab_count",
        "import_list_entry_count",
        "import_list_group_entry_count",
        "import_list_search",
        // The import lists open on this device: each one's view, its folded
        // groups and its selection.
        "import_list",
        "import_list_collapsed",
        "import_list_selection",
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
        let create =
            regex::Regex::new(r"(?m)^\s*CREATE (?:VIRTUAL )?TABLE (?:IF NOT EXISTS )?(\w+)")
                .unwrap();
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
