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
    // This device's own copies: plays before a person exists, and what this
    // device plays: its queue, where it stood, what it is doing and where it
    // is.
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
    "release_queue_versions",
    "download_queue",
    "output_queue",
    // The storage transition this process runs for each release.
    "release_storage_action",
    // The cast receivers discovery found.
    "cast_device",
    // The artist each artist shows as, kept from the artists and their
    // merges by local triggers.
    "artist_shown_as",
    // What library lists sort, filter and count by, kept from the synced
    // rows by local triggers.
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
    "library_list_versions",
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
