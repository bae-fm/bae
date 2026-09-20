use super::*;
use coven::rusqlite::types::Value;

const ORIGIN_COLUMNS: [&str; 8] = [
    "album_title_origin",
    "album_year_origin",
    "year_origin",
    "format_origin",
    "label_origin",
    "catalog_number_origin",
    "country_origin",
    "barcode_origin",
];

fn retained_rows(
    sql: &coven::SqlReadContext<'_>,
    table: &str,
) -> Result<(Vec<String>, Vec<Vec<Value>>), coven::DbError> {
    let columns: Vec<String> = sql.query(
        &format!("SELECT name FROM pragma_table_info('{table}') ORDER BY cid"),
        [],
        |row| row.get(0),
    )?;
    let columns: Vec<_> = columns
        .into_iter()
        .filter(|column| !ORIGIN_COLUMNS.contains(&column.as_str()))
        .collect();
    let rows = sql.query(
        &format!("SELECT {} FROM {table} ORDER BY 1", columns.join(", ")),
        [],
        |row| (0..columns.len()).map(|index| row.get(index)).collect(),
    )?;
    Ok((columns, rows))
}

#[tokio::test]
#[serial]
async fn removing_field_origins_preserves_values_sources_and_audio() {
    let temp = tempfile::tempdir().expect("temp store");
    let store_dir = StoreDir::new_ephemeral(temp.path());
    let mut migrations = all();
    migrations.truncate(40);
    let handle = open(store_dir.clone(), "remove-field-origins", migrations)
        .expect("open the schema with field origins");
    handle.write(|sql| {
        sql.execute_batch(
            "INSERT INTO artists (id, name, _updated_at, created_at)
                 VALUES ('aaaaaaaa-0000-4000-8000-000000000001', 'Artist Name', 'stamp', '2026-01-01T00:00:00Z');
             INSERT INTO albums (id, title, artist_id, year, is_compilation, _updated_at, created_at)
                 VALUES ('bbbbbbbb-0000-4000-8000-000000000001', 'Edited Album',
                         'aaaaaaaa-0000-4000-8000-000000000001', 1998, 0, 'stamp', '2026-01-01T00:00:00Z');
             INSERT INTO releases (id, album_id, release_name, year, format, label,
                 catalog_number, country, barcode, draft_from_tags, identified_by,
                 remote, source_folder_name, content_hash, album_loudness_lufs,
                 album_peak_linear, _updated_at, created_at,
                 album_title_origin, album_year_origin, year_origin, format_origin,
                 label_origin, catalog_number_origin, country_origin, barcode_origin)
                 VALUES ('cccccccc-0000-4000-8000-000000000001',
                         'bbbbbbbb-0000-4000-8000-000000000001', 'Edited Pressing',
                         2001, 'Vinyl', 'Label Name', 'CAT-1', 'US', NULL, 0, 'catalog_number',
                         0, 'Album Folder', 'library-hash', -12.5, 0.9, 'stamp', '2026-01-01T00:00:00Z',
                         'typed', 'tags', 'record:discogs', 'record:discogs',
                         'typed', 'record:discogs', 'typed', NULL);
             INSERT INTO release_records (id, release_id, catalog, key, group_key, url,
                 reads_draft, _updated_at, created_at)
                 VALUES ('dddddddd-0000-4000-8000-000000000001',
                         'cccccccc-0000-4000-8000-000000000001', 'discogs', 'release-1',
                         'master-1', 'https://example.invalid/release-1', 1, 'stamp', '2026-01-01T00:00:00Z');
             INSERT INTO release_marks (id, release_id, position, kind, value, origin,
                 origin_path, corroborated, _updated_at, created_at)
                 VALUES ('eeeeeeee-0000-4000-8000-000000000001',
                         'cccccccc-0000-4000-8000-000000000001', 0, 'catalog', 'CAT-1',
                         'artwork', 'back.jpg', 1, 'stamp', '2026-01-01T00:00:00Z');
             INSERT INTO tracks (id, release_id, title, side, track_number, position, duration_ms,
                 discogs_position, _updated_at, created_at)
                 VALUES ('ffffffff-0000-4000-8000-000000000001',
                         'cccccccc-0000-4000-8000-000000000001', 'Edited Track', NULL, 1, 0,
                         180000, '1', 'stamp', '2026-01-01T00:00:00Z');
             INSERT INTO import_candidate_state (content_hash, folder_path, metadata_revision, edit_revision)
                 VALUES ('candidate-hash', '/music/Album', 8, 3);
             INSERT INTO import_candidate_edit (content_hash, album_title, album_year, year,
                 format, label, catalog_number, country, barcode,
                 album_title_origin, album_year_origin, year_origin, format_origin,
                 label_origin, catalog_number_origin, country_origin, barcode_origin)
                 VALUES ('candidate-hash', 'Typed Album', '1999', '', 'CD', 'Typed Label', 'CAT-2', '', '1234',
                         'typed', 'tags', NULL, 'record:musicbrainz', 'typed', 'record:musicbrainz', NULL, 'tags');
             INSERT INTO import_candidate_draft_provenance (content_hash, kind, source, release_id, author)
                 VALUES ('candidate-hash', 'external_release', 'musicbrainz', 'release-2', 'user');
             INSERT INTO import_candidate_provenance_partner (content_hash, source, release_id)
                 VALUES ('candidate-hash', 'discogs', 'release-3');
             INSERT INTO import_candidate_track (content_hash, track_id, position, title,
                 artist_assignment_kind, side, track_number, source_index, file_kind, file_id, sheet_id, slice_index)
                 VALUES ('candidate-hash', 'track-1', 0, 'Typed Track', 'album_artists', 1, 1, 0,
                         'sheet_slice', 'disc.flac', 'disc.cue', 0),
                        ('candidate-hash', 'track-2', 1, 'Another Track', 'album_artists', NULL, 2, 1,
                         'standalone', '02.flac', NULL, NULL);
             INSERT INTO import_candidate_applied_source (content_hash, snapshot)
                 VALUES ('candidate-hash', '{\"document\":\"preserved\"}');
             INSERT INTO source_release_payloads (source, source_release_id, json, fetched_at)
                 VALUES ('musicbrainz', 'release-2', '{\"title\":\"Source Title\"}', '2026-01-01T00:00:00Z');"
        )?;
        Ok(())
    }).await.expect("seed editable metadata and its independent source records");
    const TABLES: [&str; 11] = [
        "artists",
        "albums",
        "releases",
        "release_records",
        "release_marks",
        "tracks",
        "import_candidate_state",
        "import_candidate_edit",
        "import_candidate_draft_provenance",
        "import_candidate_provenance_partner",
        "import_candidate_track",
    ];
    let before = handle
        .read(|sql| {
            TABLES
                .into_iter()
                .chain(["import_candidate_applied_source", "source_release_payloads"])
                .map(|table| retained_rows(&sql, table))
                .collect::<Result<Vec<_>, _>>()
                .map_err(CovenError::from)
        })
        .await
        .expect("capture values before removing origins");
    drop(handle);

    let handle = open(store_dir, "remove-field-origins", all()).expect("remove field origins");
    handle
        .read(move |sql| {
            for (table, expected) in TABLES
                .into_iter()
                .chain(["import_candidate_applied_source", "source_release_payloads"])
                .zip(before)
            {
                assert_eq!(retained_rows(&sql, table)?, expected, "preserve {table}");
            }
            for table in ["releases", "import_candidate_edit"] {
                let columns: Vec<String> = sql.query(
                    &format!("SELECT name FROM pragma_table_info('{table}')"),
                    [],
                    |row| row.get(0),
                )?;
                for column in ORIGIN_COLUMNS {
                    assert!(
                        !columns.iter().any(|name| name == column),
                        "remove {table}.{column}"
                    );
                }
            }
            let violations: Vec<String> =
                sql.query("PRAGMA foreign_key_check", [], |row| row.get(0))?;
            assert!(violations.is_empty());
            Ok(())
        })
        .await
        .expect("verify data survives without field origins");
}
