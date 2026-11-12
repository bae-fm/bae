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
        );
        assert_eq!(
            table.identity(),
            if shared { &RowIdentity::SharedKey } else { &RowIdentity::IndependentUuid }
        );
        let keys: &[&str] = match table.name() {
            "artist_credits" => &["artist_id", "name_key"],
            "audio_segment_scrub_points" => &["audio_format_segment_id"],
            // A track's medium is named with its release, so the key includes both.
            "release_mediums" => &["id", "release_id"],
            _ => &["id"],
        };
        assert_eq!(table.get_key_columns(), keys);
        assert_eq!(table.get_audience_column(), None);
        assert_eq!(table.get_audience_from(), None);
        assert!(!table.is_owner_only(), "{}", table.name());
        let columns: Vec<(&str, &Binding, bool)> = table
            .file_columns()
            .iter()
            .map(|column| (column.column(), column.binding(), column.is_write_once()))
            .collect();
        let bound = Binding::Bound { limit: coven::BOUND_FILE_LIMIT };
        let expected: &[(&str, &Binding, bool)] = match table.name() {
            "release_files" => &[("file", &Binding::Trailing, false)],
            "covers" | "artist_images" => &[("file", &bound, false)],
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
                crate::CacheLimit::DEFAULT.bytes()
            ),
            (COVERS_NAMESPACE, &Provenance::AppProvided, 512 * 1024 * 1024),
            (ARTIST_IMAGES_NAMESPACE, &Provenance::AppProvided, 256 * 1024 * 1024),
        ]
    );
    assert_eq!(
        tables.iter().find(|t| t.name() == "covers").unwrap().identity(),
        &RowIdentity::SharedKey
    );
}

/// Tables of this crate's parts of the schema whose rows never leave this
/// device. A table the parts create is either declared in `synced_tables()` or
/// named here, so a new table is a decision rather than a row that silently
/// stays on one device.
const DEVICE_LOCAL_TABLES: &[&str] = &[
    // The artist each artist shows as, kept from the artists and their
    // merges by local triggers.
    "artist_shown_as",
    // Where each release's files are on this device, kept from them and
    // coven's records by local triggers.
    "release_storage",
];

#[test]
fn every_schema_table_is_synced_or_declared_device_local() {
    let create =
        regex::Regex::new(r"(?m)^\s*CREATE (?:VIRTUAL )?TABLE (?:IF NOT EXISTS )?(\w+)").unwrap();
    let schema: BTreeSet<&str> = bae_store::Schema::new(&[crate::SCHEMA_PARTS])
        .sql()
        .flat_map(|sql| create.captures_iter(sql))
        .map(|captures| captures.get(1).unwrap().as_str())
        .collect();
    let tables = bae_store::Schema::new(&[crate::SCHEMA_PARTS]).synced_tables();
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
