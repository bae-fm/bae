use super::*;
use coven::Binding;
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
            "release_media",
            "release_barcodes",
            "labels",
            "label_merges",
            "release_labels",
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
                | "release_media"
                | "release_barcodes"
                | "labels"
                | "label_merges"
                | "release_labels"
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
            if shared { &RowIdentity::SharedKey } else { &RowIdentity::IndependentUuid }
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
        let columns: Vec<(&str, &Binding, bool)> = table
            .file_columns()
            .iter()
            .map(|column| (column.column(), column.binding(), column.is_write_once()))
            .collect();
        let bound = Binding::Bound { limit: coven::BOUND_FILE_LIMIT };
        let expected: &[(&str, &Binding, bool)] = match table.name() {
            "release_files" => &[("file", &Binding::Trailing, false)],
            "covers" | "artist_images" => &[("file", &bound, false)],
            "people" => &[("avatar_id", &bound, false)],
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
            (RELEASE_FILES_NAMESPACE, &Provenance::UserProvided, 20 * 1024 * 1024 * 1024),
            (COVERS_NAMESPACE, &Provenance::AppProvided, 512 * 1024 * 1024),
            (ARTIST_IMAGES_NAMESPACE, &Provenance::AppProvided, 256 * 1024 * 1024),
            (AVATARS_NAMESPACE, &Provenance::AppProvided, 64 * 1024 * 1024),
        ]
    );
    assert_eq!(
        tables.iter().find(|t| t.name() == "covers").unwrap().identity(),
        &RowIdentity::SharedKey
    );
}

/// Tables of bae-db's part of the schema whose rows never leave this device.
/// A table its part creates is either declared in `synced_tables()` or named
/// here, so a new table is a decision rather than a row that silently stays
/// on one device. A synced table is one of bae-db's part, so every table of
/// another crate's part stays on the device.
const DEVICE_LOCAL_TABLES: &[&str] = &[
    // This device's own copies: the folder each release was imported
    // from, plays before a person exists, and what this device plays:
    // its queue, where it stood, what it is doing and where it is.
    "local_release_origin",
    "local_history",
    // The profile this device's person asked for, while their private
    // circle is being made.
    "profile_request",
    // What each admin change this device asked for changes, while coven
    // lists its operation.
    "admin_operations",
    "playback_queue",
    "playback_manual",
    "playback_context",
    "playback_state",
    "playback_now",
    "playback_timeline",
    // This device's settings for the library.
    "preferences",
    "save_preset",
    "save_preset_filename_token",
    // The releases this device is downloading, exporting or saving, one
    // at a time.
    "release_queues",
    "download_queue",
    "output_queue",
    // The storage transition this process runs for each release.
    "release_storage_action",
    // The cast receivers discovery found.
    "cast_device",
    // What library lists sort, filter and count by, kept from the synced
    // rows by local triggers.
    "artist_shown_as",
    "composer_work",
    "composer_release",
    "composer_unlinked_credit",
    "composer_listing",
    "composer_detail_row",
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
    // Import: what scanning found in the folders this device watches, and
    // each candidate's state while it is imported.
    "release_grouping",
    "release_grouping_member",
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
    "import_candidate_edit_label",
    "import_candidate_edit_barcode",
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
    "import_candidate_remote_cover_asset",
    "import_candidate_draft_provenance",
    "import_candidate_release_link",
    "import_candidate_release_link_partner",
    "import_candidate_release_link_album",
    "import_candidate_session",
    "import_candidate_failure",
    "import_candidate_standing",
    "import_identification_batch",
    "import_identify_run",
    "import_candidate_search",
    "import_candidate_search_source",
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
    "import_candidate_match_label",
    "import_candidate_match_cover_copy",
    "import_candidate_match_link",
    "import_candidate_match_album_link",
    "import_candidate_match_medium",
    "import_candidate_lookup_provider",
    "import_candidate_lookup_isrc",
    "import_candidate_lookup_offered_catalog",
    "import_candidate_lookup_value",
    "import_candidate_lookup",
    "import_candidate_lookup_release",
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
    "source_release_barcode",
    "source_release_label",
    // Why this device's last attempt to start sync failed.
    "sync_start_failure",
];

#[test]
fn every_schema_table_is_synced_or_declared_device_local() {
    let create =
        regex::Regex::new(r"(?m)^\s*CREATE (?:VIRTUAL )?TABLE (?:IF NOT EXISTS )?(\w+)").unwrap();
    let schema: BTreeSet<&str> = crate::test_fixtures::TEST_SCHEMA
        .sql()
        .flat_map(|sql| create.captures_iter(sql))
        .map(|captures| captures.get(1).unwrap().as_str())
        .collect();
    let tables = synced_tables();
    let synced: BTreeSet<&str> = tables.iter().map(SyncedTable::name).collect();
    let local: BTreeSet<&str> = DEVICE_LOCAL_TABLES.iter().copied().collect();
    assert_eq!(local.len(), DEVICE_LOCAL_TABLES.len(), "a device-local table is listed twice");
    let undeclared: Vec<_> =
        schema.iter().filter(|table| !synced.contains(*table) && !local.contains(*table)).collect();
    assert!(
        undeclared.is_empty(),
        "tables neither synced nor listed as device-local: {undeclared:?}"
    );
    let both: Vec<_> = synced.intersection(&local).collect();
    assert!(both.is_empty(), "tables both synced and device-local: {both:?}");
    let missing: Vec<_> = synced.union(&local).filter(|table| !schema.contains(*table)).collect();
    assert!(missing.is_empty(), "declared tables the schema doesn't create: {missing:?}");
}
