use super::*;
use chrono::{TimeZone, Utc};
use coven::{Coven, CovenError, FixedClock, MigrationError, StoreDir};
use serial_test::serial;
use std::sync::Arc;

#[path = "migrations_tests/migration_fifteen.rs"]
mod migration_fifteen;
#[path = "migrations_tests/migration_fourteen.rs"]
mod migration_fourteen;
#[path = "migrations_tests/migration_sixteen.rs"]
mod migration_sixteen;
#[path = "migrations_tests/migration_ten.rs"]
mod migration_ten;
#[path = "migrations_tests/migration_twelve.rs"]
mod migration_twelve;

fn config(store_id: &str) -> coven::Config {
    coven::Config::with_defaults(
        store_id.to_string(),
        "migration-device".to_string(),
        "Migration Test".to_string(),
    )
}

fn open(
    store_dir: StoreDir,
    store_id: &str,
    migrations: Vec<coven::Migration>,
) -> Result<coven::CovenHandle, CovenError> {
    crate::config::install_test_keyring();
    Coven::builder(store_dir, config(store_id))
        .synced_tables(crate::sync::synced_tables())
        .coven_migration_policy(coven::CovenMigrationPolicy::ApplyPending)
        .clock(Arc::new(FixedClock(
            Utc.with_ymd_and_hms(2026, 1, 2, 3, 4, 5)
                .single()
                .expect("valid migration test instant"),
        )))
        .oauth_clients(crate::oauth::clients())
        .migrations(migrations)
        .open()
}

fn version_one() -> Vec<coven::Migration> {
    vec![coven::Migration::sql(
        1,
        "initial",
        include_str!("../migrations/001_initial.sql"),
    )]
}

fn version_two() -> Vec<coven::Migration> {
    let mut migrations = all();
    migrations.truncate(2);
    migrations
}

fn version_three() -> Vec<coven::Migration> {
    let mut migrations = all();
    migrations.truncate(3);
    migrations
}

fn version_four() -> Vec<coven::Migration> {
    let mut migrations = all();
    migrations.truncate(4);
    migrations
}

fn version_five() -> Vec<coven::Migration> {
    let mut migrations = all();
    migrations.truncate(5);
    migrations
}

fn version_six() -> Vec<coven::Migration> {
    let mut migrations = all();
    migrations.truncate(6);
    migrations
}

fn version_seven() -> Vec<coven::Migration> {
    let mut migrations = all();
    migrations.truncate(7);
    migrations
}

fn version_eight() -> Vec<coven::Migration> {
    let mut migrations = all();
    migrations.truncate(8);
    migrations
}

fn version_nine() -> Vec<coven::Migration> {
    let mut migrations = all();
    migrations.truncate(9);
    migrations
}

fn version_eleven() -> Vec<coven::Migration> {
    let mut migrations = all();
    migrations.truncate(11);
    migrations
}

fn version_thirteen() -> Vec<coven::Migration> {
    let mut migrations = all();
    migrations.truncate(13);
    migrations
}

fn version_fourteen() -> Vec<coven::Migration> {
    let mut migrations = all();
    migrations.truncate(14);
    migrations
}

#[tokio::test]
#[serial]
async fn migration_eight_preserves_pressing_year_and_adds_blank_album_year() {
    let temp = tempfile::tempdir().expect("temp store");
    let store_dir = StoreDir::new_ephemeral(temp.path());
    let handle = open(store_dir.clone(), "migration-album-year", version_seven())
        .expect("open version seven");
    handle
        .write(|sql| {
            sql.execute_batch(
                "INSERT INTO import_candidate_state (content_hash, folder_path)
                     VALUES ('candidate-hash', '/music/release');
                 INSERT INTO import_candidate_edit (
                     content_hash, album_title, year, format, label,
                     catalog_number, country, barcode
                 ) VALUES (
                     'candidate-hash', 'Album Title', '2004', '', '', '', '', ''
                 );",
            )?;
            Ok(())
        })
        .await
        .expect("seed version-seven draft");
    drop(handle);

    let handle =
        open(store_dir, "migration-album-year", version_eight()).expect("migrate to version eight");
    handle
        .read(|sql| {
            let values: (String, String) = sql.query_row(
                "SELECT album_year, year FROM import_candidate_edit WHERE content_hash = 'candidate-hash'",
                [],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )?;
            assert_eq!(values, (String::new(), "2004".to_string()));
            Ok(())
        })
        .await
        .expect("read migrated draft");
}

#[tokio::test]
#[serial]
async fn migration_nine_preserves_a_remote_cover_as_explicitly_unprepared() {
    let temp = tempfile::tempdir().expect("temp store");
    let store_dir = StoreDir::new_ephemeral(temp.path());
    let handle = open(
        store_dir.clone(),
        "migration-prepared-assets",
        version_eight(),
    )
    .expect("open version eight");
    handle
        .write(|sql| {
            sql.execute_batch(
                "INSERT INTO import_candidate_state (content_hash, folder_path) \
                     VALUES ('candidate-hash', '/candidate'); \
                 INSERT INTO import_candidate_cover \
                     (content_hash, kind, file_id, url, source) \
                     VALUES ('candidate-hash', 'remote', NULL, \
                             'https://example.invalid/cover', 'discogs');",
            )?;
            Ok(())
        })
        .await
        .expect("seed version-eight remote cover");
    drop(handle);

    let handle = open(store_dir, "migration-prepared-assets", version_nine())
        .expect("migrate prepared assets");
    handle
        .read(|sql| {
            let version: i64 = sql.query_row("PRAGMA user_version", [], |row| row.get(0))?;
            assert_eq!(version, 9);
            let cover: (String, String) = sql.query_row(
                "SELECT url, source FROM import_candidate_cover \
                 WHERE content_hash = 'candidate-hash'",
                [],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )?;
            assert_eq!(
                cover,
                (
                    "https://example.invalid/cover".to_string(),
                    "discogs".to_string()
                )
            );
            let prepared: i64 = sql.query_row(
                "SELECT COUNT(*) FROM import_candidate_remote_cover_asset \
                 WHERE content_hash = 'candidate-hash'",
                [],
                |row| row.get(0),
            )?;
            assert_eq!(prepared, 0);
            let preparation: i64 = sql.query_row(
                "SELECT COUNT(*) FROM import_candidate_asset_preparation \
                 WHERE content_hash = 'candidate-hash'",
                [],
                |row| row.get(0),
            )?;
            assert_eq!(preparation, 0);
            Ok(())
        })
        .await
        .expect("verify version-nine prepared assets");
}

#[tokio::test]
#[serial]
async fn migration_two_preserves_candidate_graph_and_normalizes_artist_edits() {
    let temp = tempfile::tempdir().expect("temp store");
    let store_dir = StoreDir::new_ephemeral(temp.path());
    let handle =
        open(store_dir.clone(), "migration-preserves", version_one()).expect("open version one");
    handle
        .write(|sql| {
            sql.execute_batch(
                "INSERT INTO import_candidate_state (
                     content_hash, folder_path, verdict_kind, verdict_track_count,
                     probed_total_duration_ms, identified_at, pick_kind, pick_source,
                     pick_release_id, identity_pick_author
                 ) VALUES
                     ('external-hash', '/candidate/external', 'found', 1, 1000,
                      '2026-01-02T03:04:05Z', 'release', 'musicbrainz',
                      'source-release', 'user'),
                     ('tags-hash', '/candidate/tags', NULL, NULL, NULL, NULL,
                      'unknown', NULL, NULL, 'user'),
                     ('neutral-hash', '/candidate/neutral', NULL, NULL, NULL, NULL,
                      NULL, NULL, NULL, NULL);

                 INSERT INTO import_candidate_match (
                     content_hash, position, source, release_id, title, artist, year,
                     format, label, catalog_number, country, cover_url,
                     cover_thumbnail_url, cover_label, cover_source, source_group_id,
                     source_tracks_kind, source_tracks_count, source_tracks_total_ms,
                     by_disc_id, by_barcode, by_catalog
                 ) VALUES (
                     'external-hash', 0, 'musicbrainz', 'source-release', 'Album Alpha',
                     'Artist Alpha', 2001, 'CD', 'Label Alpha', 'CAT-1', 'US',
                     NULL, NULL, NULL, NULL, 'source-group', 'listed', 1, 1000, 1, 0, 0
                 );
                 INSERT INTO import_candidate_file_edit
                     (content_hash, relative_path, role_choice)
                     VALUES ('external-hash', '01.flac', 'audio');
                 INSERT INTO import_candidate_file_duration
                     (content_hash, kind, relative_path, duration_ms)
                     VALUES ('external-hash', 'file', '01.flac', 1000);
                 INSERT INTO import_candidate_signals (
                     content_hash, disc_id_state, disc_id, disc_id_source_file,
                     track_count, barcode_state, text_state
                 ) VALUES (
                     'external-hash', 'computed', 'disc-id', 'rip.log', 1,
                     'settled', 'settled'
                 );
                 INSERT INTO import_candidate_signal_value
                     (content_hash, list, position, value, origin, origin_path)
                     VALUES ('external-hash', 'free_text', 0, 'query text', NULL, NULL);
                 INSERT INTO import_candidate_failure
                     (content_hash, error, failed_at)
                     VALUES ('external-hash', 'failed import', '2026-01-02T03:04:05Z');
                 INSERT INTO import_candidate_cover
                     (content_hash, kind, file_id, url, source)
                     VALUES ('external-hash', 'local', 'cover.jpg', NULL, NULL);
                 INSERT INTO import_candidate_edit (
                     content_hash, album_title, album_artist_text
                 ) VALUES (
                     'external-hash', 'Edited Album', ' Artist Alpha, Artist Beta '
                 );
                 INSERT INTO import_candidate_track_edit (
                     content_hash, track_id, dropped, title, artist_text, side,
                     track_number, file_kind, file_id, sheet_id, slice_index
                 ) VALUES (
                     'external-hash', 'import-track:0', 0, 'Edited Track',
                     'Artist Gamma, Artist Delta', 1, 1, 'standalone', '01.flac',
                     NULL, NULL
                 );",
            )?;
            Ok(())
        })
        .await
        .expect("seed version-one candidate graph");
    drop(handle);

    let handle =
        open(store_dir, "migration-preserves", version_two()).expect("migrate to version two");
    handle
        .read(|sql| {
            let version: i64 = sql.query_row("PRAGMA user_version", [], |row| row.get(0))?;
            assert_eq!(version, 2);
            let seeds = sql.query(
                "SELECT content_hash, seed_kind, seed_source, seed_release_id,
                        metadata_seed_author
                 FROM import_candidate_state ORDER BY content_hash",
                [],
                |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, Option<String>>(1)?,
                        row.get::<_, Option<String>>(2)?,
                        row.get::<_, Option<String>>(3)?,
                        row.get::<_, Option<String>>(4)?,
                    ))
                },
            )?;
            assert_eq!(
                seeds,
                vec![
                    (
                        "external-hash".to_string(),
                        Some("external_release".to_string()),
                        Some("musicbrainz".to_string()),
                        Some("source-release".to_string()),
                        Some("user".to_string()),
                    ),
                    ("neutral-hash".to_string(), None, None, None, None),
                    (
                        "tags-hash".to_string(),
                        Some("file_tags".to_string()),
                        None,
                        None,
                        Some("user".to_string()),
                    ),
                ]
            );
            let album_artists = sql.query(
                "SELECT position, name FROM import_candidate_album_artist_assignment
                 WHERE content_hash = 'external-hash' ORDER BY position",
                [],
                |row| Ok((row.get::<_, i64>(0)?, row.get::<_, String>(1)?)),
            )?;
            assert_eq!(
                album_artists,
                vec![
                    (0, "Artist Alpha".to_string()),
                    (1, "Artist Beta".to_string())
                ]
            );
            let track_artists = sql.query(
                "SELECT position, name FROM import_candidate_track_artist_assignment
                 WHERE content_hash = 'external-hash' AND track_id = 'import-track:0'
                 ORDER BY position",
                [],
                |row| Ok((row.get::<_, i64>(0)?, row.get::<_, String>(1)?)),
            )?;
            assert_eq!(
                track_artists,
                vec![
                    (0, "Artist Gamma".to_string()),
                    (1, "Artist Delta".to_string())
                ]
            );
            for table in [
                "import_candidate_match",
                "import_candidate_file_edit",
                "import_candidate_signals",
                "import_candidate_signal_value",
                "import_candidate_failure",
                "import_candidate_cover",
                "import_candidate_edit",
                "import_candidate_track_edit",
            ] {
                let count: i64 =
                    sql.query_row(&format!("SELECT COUNT(*) FROM {table}"), [], |row| {
                        row.get(0)
                    })?;
                assert_eq!(count, 1, "{table} row preserved");
            }
            let violations = sql.query("PRAGMA foreign_key_check", [], |row| {
                row.get::<_, String>(0)
            })?;
            assert!(violations.is_empty());
            Ok(())
        })
        .await
        .expect("read migrated candidate graph");
}

#[tokio::test]
#[serial]
async fn migration_two_renames_file_tag_track_edit_ids() {
    let temp = tempfile::tempdir().expect("temp store");
    let store_dir = StoreDir::new_ephemeral(temp.path());
    let handle =
        open(store_dir.clone(), "migration-track-ids", version_one()).expect("open version one");
    handle
        .write(|sql| {
            sql.execute_batch(
                "INSERT INTO import_candidate_state (
                     content_hash, folder_path, pick_kind, identity_pick_author
                 ) VALUES ('tags-hash', '/candidate/tags', 'unknown', 'user');
                 INSERT INTO import_candidate_track_edit (
                     content_hash, track_id, dropped, title, artist_text, side,
                     track_number, file_kind, file_id, sheet_id, slice_index
                 ) VALUES (
                     'tags-hash', 'unknown-track-0', 0, 'Track Title',
                     'Artist Name', 1, 1, 'standalone', '01.flac', NULL, NULL
                 );",
            )?;
            Ok(())
        })
        .await
        .expect("seed version-one File Tags edit");
    drop(handle);

    let handle =
        open(store_dir, "migration-track-ids", version_two()).expect("migrate File Tags edit");
    handle
        .read(|sql| {
            let track_id: String = sql.query_row(
                "SELECT track_id FROM import_candidate_track_edit",
                [],
                |row| row.get(0),
            )?;
            assert_eq!(track_id, "file-tag-track-0");
            let assignment_track_id: String = sql.query_row(
                "SELECT track_id FROM import_candidate_track_artist_assignment",
                [],
                |row| row.get(0),
            )?;
            assert_eq!(assignment_track_id, "file-tag-track-0");
            Ok(())
        })
        .await
        .expect("read migrated File Tags edit");
}

#[tokio::test]
#[serial]
async fn rejected_artist_backfill_rolls_back_the_whole_migration() {
    let temp = tempfile::tempdir().expect("temp store");
    let store_dir = StoreDir::new_ephemeral(temp.path());
    let handle =
        open(store_dir.clone(), "migration-rollback", version_one()).expect("open version one");
    handle
        .write(|sql| {
            sql.execute(
                "INSERT INTO import_candidate_state (content_hash, folder_path)
                 VALUES ('invalid-edit', '/candidate/invalid')",
                [],
            )?;
            sql.execute(
                "INSERT INTO import_candidate_edit (content_hash, album_artist_text)
                 VALUES ('invalid-edit', ' , , ')",
                [],
            )?;
            Ok(())
        })
        .await
        .expect("seed invalid version-one edit");
    drop(handle);

    let error = match open(store_dir.clone(), "migration-rollback", version_two()) {
        Ok(_) => panic!("empty artist override must reject migration"),
        Err(error) => error,
    };
    assert!(matches!(
        error,
        CovenError::Migration(MigrationError::Failed { version: 2, .. })
    ));

    let connection =
        coven::rusqlite::Connection::open(store_dir.db_path()).expect("open rolled-back database");
    let version: i64 = connection
        .pragma_query_value(None, "user_version", |row| row.get(0))
        .expect("read rolled-back version");
    assert_eq!(version, 1);
    let columns: Vec<String> = connection
        .prepare("PRAGMA table_info(import_candidate_state)")
        .expect("prepare column read")
        .query_map([], |row| row.get(1))
        .expect("read columns")
        .collect::<Result<_, _>>()
        .expect("collect columns");
    assert!(columns.iter().any(|column| column == "pick_kind"));
    assert!(!columns.iter().any(|column| column == "seed_kind"));
}

#[tokio::test]
#[serial]
async fn migration_three_preserves_library_and_watched_folder_intent() {
    let temp = tempfile::tempdir().expect("temp store");
    let store_dir = StoreDir::new_ephemeral(temp.path());
    let handle =
        open(store_dir.clone(), "migration-drafts", version_two()).expect("open version two");
    handle
        .write(|sql| {
            sql.execute_batch(
                "INSERT INTO artists (id, name, _updated_at, created_at)
                     VALUES ('11111111-1111-4111-8111-111111111111', 'Artist Name', '2026-01-02T03:04:05Z',
                             '2026-01-02T03:04:05Z');
                 INSERT INTO albums (
                     id, title, artist_id, is_compilation, _updated_at, created_at
                 ) VALUES (
                     '22222222-2222-4222-8222-222222222222', 'Album Title',
                     '11111111-1111-4111-8111-111111111111', 0,
                     '2026-01-02T03:04:05Z', '2026-01-02T03:04:05Z'
                 );
                 INSERT INTO releases (
                     id, album_id, metadata_source, remote, _updated_at, created_at
                 ) VALUES (
                     '33333333-3333-4333-8333-333333333333',
                     '22222222-2222-4222-8222-222222222222', 'file_tags', 1,
                     '2026-01-02T03:04:05Z', '2026-01-02T03:04:05Z'
                 );
                 INSERT INTO tracks (
                     id, release_id, title, side, _updated_at, created_at
                 ) VALUES (
                     '44444444-4444-4444-8444-444444444444',
                     '33333333-3333-4333-8333-333333333333', 'Track Title', 1,
                     '2026-01-02T03:04:05Z', '2026-01-02T03:04:05Z'
                 );
                 INSERT INTO watched_import_folders (path, position)
                     VALUES ('/music', 0);
                 INSERT INTO folder_release_decisions (
                     watched_folder_path, relative_folder_path, decision, author
                 ) VALUES (
                     '/music', 'collection', 'keep_as_separate_releases', 'user'
                 );
                 INSERT INTO folder_scan_roots (
                     watched_folder_path, generation, status
                 ) VALUES ('/music', 1, 'complete');
                 INSERT INTO import_candidate_state (content_hash, folder_path)
                     VALUES ('candidate-hash', '/music/collection/release');",
            )?;
            Ok(())
        })
        .await
        .expect("seed version-two state");
    drop(handle);

    let handle =
        open(store_dir, "migration-drafts", version_three()).expect("migrate to version three");
    handle
        .read(|sql| {
            let version: i64 = sql.query_row("PRAGMA user_version", [], |row| row.get(0))?;
            assert_eq!(version, 3);
            for (table, expected) in [
                ("artists", 1),
                ("albums", 1),
                ("releases", 1),
                ("tracks", 1),
                ("watched_import_folders", 1),
                ("folder_release_decisions", 1),
                ("folder_scan_roots", 0),
                ("import_candidate_state", 0),
            ] {
                let count: i64 =
                    sql.query_row(&format!("SELECT COUNT(*) FROM {table}"), [], |row| {
                        row.get(0)
                    })?;
                assert_eq!(count, expected, "{table} row count");
            }
            Ok(())
        })
        .await
        .expect("read migrated state");
    handle
        .write(|sql| {
            sql.execute(
                "INSERT INTO releases (
                     id, album_id, metadata_source, remote, _updated_at, created_at
                 ) VALUES (
                     '55555555-5555-4555-8555-555555555555',
                     '22222222-2222-4222-8222-222222222222', 'none', 1,
                     '2026-01-02T03:04:05Z', '2026-01-02T03:04:05Z'
                 )",
                [],
            )?;
            Ok(())
        })
        .await
        .expect("insert source-less release");
}

#[tokio::test]
#[serial]
async fn migration_four_adds_conflict_storage_without_merging_existing_artists() {
    let temp = tempfile::tempdir().expect("temp store");
    let store_dir = StoreDir::new_ephemeral(temp.path());
    let handle = open(store_dir.clone(), "migration-conflicts", version_three())
        .expect("open version three");
    handle
        .write(|sql| {
            sql.execute_batch(
                "INSERT INTO artists (
                     id, name, discogs_artist_id, musicbrainz_artist_id,
                     _updated_at, created_at
                 ) VALUES
                     ('11111111-1111-4111-8111-111111111111', 'Artist Name', 'discogs-1', NULL,
                      '2026-01-02T03:04:05Z', '2026-01-02T03:04:05Z'),
                     ('22222222-2222-4222-8222-222222222222', 'Artist Name', NULL, 'mb-1',
                      '2026-01-02T03:04:05Z', '2026-01-02T03:04:05Z');",
            )?;
            Ok(())
        })
        .await
        .expect("seed separate provider-linked artists");
    drop(handle);

    let handle =
        open(store_dir, "migration-conflicts", version_four()).expect("migrate to version four");
    handle
        .read(|sql| {
            let version: i64 = sql.query_row("PRAGMA user_version", [], |row| row.get(0))?;
            assert_eq!(version, 4);
            let artist_count: i64 =
                sql.query_row("SELECT COUNT(*) FROM artists", [], |row| row.get(0))?;
            assert_eq!(artist_count, 2);
            let conflict_table_count: i64 = sql.query_row(
                "SELECT COUNT(*) FROM sqlite_schema \
                 WHERE type = 'table' AND name = 'import_candidate_artist_identity_conflict'",
                [],
                |row| row.get(0),
            )?;
            assert_eq!(conflict_table_count, 1);
            Ok(())
        })
        .await
        .expect("read version-four state");
}

#[tokio::test]
#[serial]
async fn migration_five_installs_source_audio_schema() {
    let temp = tempfile::tempdir().expect("temp store");
    let store_dir = StoreDir::new_ephemeral(temp.path());
    let handle = open(store_dir.clone(), "migration-audio-facts", version_four())
        .expect("open version four");
    drop(handle);

    let handle =
        open(store_dir, "migration-audio-facts", version_five()).expect("migrate to version five");
    handle
        .read(|sql| {
            let version: i64 = sql.query_row("PRAGMA user_version", [], |row| row.get(0))?;
            assert_eq!(version, 5);
            let columns: Vec<String> = sql.query(
                "SELECT name FROM pragma_table_info('scan_candidate_file') ORDER BY cid",
                [],
                |row| row.get(0),
            )?;
            for expected in [
                "modified_at_ns",
                "content_digest",
                "audio_content_type",
                "audio_duration_ms",
                "audio_sample_rate_hz",
                "audio_bits_per_sample",
                "audio_bitrate_kbps",
                "audio_channels",
            ] {
                assert!(
                    columns.iter().any(|column| column == expected),
                    "{expected}"
                );
            }
            let candidate_columns: Vec<String> = sql.query(
                "SELECT name FROM pragma_table_info('scan_candidate') ORDER BY cid",
                [],
                |row| row.get(0),
            )?;
            assert!(!candidate_columns
                .iter()
                .any(|column| column == "format_label"));
            let release_file_columns: Vec<String> = sql.query(
                "SELECT name FROM pragma_table_info('release_files') ORDER BY cid",
                [],
                |row| row.get(0),
            )?;
            for expected in [
                "source_audio_layout",
                "source_audio_content_type",
                "source_audio_duration_ms",
                "source_audio_sample_rate_hz",
                "source_audio_bits_per_sample",
                "source_audio_bitrate_kbps",
                "source_audio_channels",
            ] {
                assert!(
                    release_file_columns.iter().any(|column| column == expected),
                    "{expected}"
                );
            }
            let indexes: Vec<(String, i64)> = sql.query(
                "SELECT name, partial FROM pragma_index_list('scan_candidate') ORDER BY name",
                [],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )?;
            assert!(indexes.contains(&("idx_scan_candidate_path".to_string(), 0)));
            assert!(indexes.contains(&("idx_scan_candidate_content_hash".to_string(), 1,)));
            let duration_table: i64 = sql.query_row(
                "SELECT COUNT(*) FROM sqlite_master \
                 WHERE type = 'table' AND name = 'import_candidate_file_duration'",
                [],
                |row| row.get(0),
            )?;
            assert_eq!(duration_table, 0);
            let violations = sql.query("PRAGMA foreign_key_check", [], |row| {
                row.get::<_, String>(0)
            })?;
            assert!(violations.is_empty());
            Ok(())
        })
        .await
        .expect("read migrated source-audio schema");
}

#[tokio::test]
#[serial]
async fn migration_five_rejects_nonempty_scan_state_without_erasing_it() {
    let temp = tempfile::tempdir().expect("temp store");
    let store_dir = StoreDir::new_ephemeral(temp.path());
    let handle = open(
        store_dir.clone(),
        "migration-audio-nonempty",
        version_four(),
    )
    .expect("open version four");
    handle
        .write(|sql| {
            sql.execute_batch(
                "INSERT INTO watched_import_folders (path, position) VALUES ('/music', 0);
                 INSERT INTO folder_scan_roots (watched_folder_path, generation, status)
                     VALUES ('/music', 7, 'complete');
                 INSERT INTO folder_scan_directory (watched_folder_path, path, modified_at)
                     VALUES ('/music', '/music/release', 1234);",
            )?;
            Ok(())
        })
        .await
        .expect("seed version-four scan state");
    drop(handle);

    let error = match open(
        store_dir.clone(),
        "migration-audio-nonempty",
        version_five(),
    ) {
        Ok(_) => panic!("nonempty version-four scan state must reject migration"),
        Err(error) => error,
    };
    assert!(matches!(
        error,
        CovenError::Migration(MigrationError::Failed { version: 5, .. })
    ));

    let connection =
        coven::rusqlite::Connection::open(store_dir.db_path()).expect("open rolled-back database");
    let version: i64 = connection
        .pragma_query_value(None, "user_version", |row| row.get(0))
        .expect("read rolled-back version");
    assert_eq!(version, 4);
    let root_count: i64 = connection
        .query_row("SELECT COUNT(*) FROM folder_scan_roots", [], |row| {
            row.get(0)
        })
        .expect("count preserved scan roots");
    let directory_count: i64 = connection
        .query_row("SELECT COUNT(*) FROM folder_scan_directory", [], |row| {
            row.get(0)
        })
        .expect("count preserved scan directories");
    assert_eq!((root_count, directory_count), (1, 1));
}

#[tokio::test]
#[serial]
async fn migration_five_enforces_complete_codec_specific_audio_facts() {
    let temp = tempfile::tempdir().expect("temp store");
    let store_dir = StoreDir::new_ephemeral(temp.path());
    let handle =
        open(store_dir, "migration-audio-constraints", version_five()).expect("open version five");
    handle
        .write(|sql| {
            sql.execute_batch(
                "INSERT INTO watched_import_folders (path, position) VALUES ('/music', 0);
                 INSERT INTO folder_scan_roots (watched_folder_path, generation, status)
                     VALUES ('/music', 1, 'complete');
                 INSERT INTO scan_candidate (
                     watched_folder_path, path, generation, kind, name, display_path,
                     file_root, scope, content_hash, initial_metadata_source
                 ) VALUES (
                     '/music', '/music/release', 1, 'valid', 'release', 'release',
                     '/music/release', 'direct', 'candidate-hash', 'none'
                 );",
            )?;

            let invalid_lossless = sql.execute(
                "INSERT INTO scan_candidate_file (
                     watched_folder_path, candidate_path, relative_path, position,
                     absolute_path, size, modified_at_ns, content_digest,
                     audio_content_type, audio_duration_ms, audio_sample_rate_hz,
                     audio_bits_per_sample, audio_bitrate_kbps, audio_channels,
                     file_name, proposed_audio, role
                 ) VALUES (
                     '/music', '/music/release', 'both.flac', 0,
                     '/music/release/both.flac', 100, 1, ?1,
                     'audio/flac', 1000, 44100, 16, 900, 2,
                     'both.flac', 1, 'audio'
                 )",
                coven::rusqlite::params!["0".repeat(64)],
            );
            assert!(invalid_lossless.is_err(), "lossless facts cannot carry bitrate");

            let invalid_lossy = sql.execute(
                "INSERT INTO scan_candidate_file (
                     watched_folder_path, candidate_path, relative_path, position,
                     absolute_path, size, modified_at_ns, content_digest,
                     audio_content_type, audio_duration_ms, audio_sample_rate_hz,
                     audio_bits_per_sample, audio_bitrate_kbps, audio_channels,
                     file_name, proposed_audio, role
                 ) VALUES (
                     '/music', '/music/release', 'neither.mp3', 1,
                     '/music/release/neither.mp3', 100, 1, ?1,
                     'audio/mpeg', 1000, 44100, NULL, NULL, 2,
                     'neither.mp3', 1, 'audio'
                 )",
                coven::rusqlite::params!["1".repeat(64)],
            );
            assert!(invalid_lossy.is_err(), "lossy facts require bitrate");

            sql.execute(
                "INSERT INTO scan_candidate_file (
                     watched_folder_path, candidate_path, relative_path, position,
                     absolute_path, size, modified_at_ns, content_digest,
                     audio_content_type, audio_duration_ms, audio_sample_rate_hz,
                     audio_bits_per_sample, audio_bitrate_kbps, audio_channels,
                     file_name, proposed_audio, role
                 ) VALUES (
                     '/music', '/music/release', 'valid.flac', 2,
                     '/music/release/valid.flac', 100, 1, ?1,
                     'audio/flac', 1000, 44100, 16, NULL, 2,
                     'valid.flac', 1, 'audio'
                 )",
                coven::rusqlite::params!["2".repeat(64)],
            )?;
            sql.execute(
                "INSERT INTO scan_candidate_file (
                     watched_folder_path, candidate_path, relative_path, position,
                     absolute_path, size, modified_at_ns, content_digest,
                     audio_content_type, audio_duration_ms, audio_sample_rate_hz,
                     audio_bits_per_sample, audio_bitrate_kbps, audio_channels,
                     file_name, proposed_audio, role
                 ) VALUES (
                     '/music', '/music/release', 'valid.mp3', 3,
                     '/music/release/valid.mp3', 100, 1, ?1,
                     'audio/mpeg', 1000, 44100, NULL, 320, 2,
                     'valid.mp3', 1, 'audio'
                 )",
                coven::rusqlite::params!["3".repeat(64)],
            )?;

            sql.execute_batch(
                "INSERT INTO artists (id, name, _updated_at, created_at)
                     VALUES ('11111111-1111-4111-8111-111111111111', 'Artist', 'stamp', '2026-01-01T00:00:00Z');
                 INSERT INTO albums (
                     id, title, artist_id, is_compilation, _updated_at, created_at
                 ) VALUES (
                     '22222222-2222-4222-8222-222222222222', 'Album',
                     '11111111-1111-4111-8111-111111111111', 0, 'stamp', '2026-01-01T00:00:00Z'
                 );
                 INSERT INTO releases (
                     id, album_id, metadata_source, remote, _updated_at, created_at
                 ) VALUES (
                     '33333333-3333-4333-8333-333333333333',
                     '22222222-2222-4222-8222-222222222222', 'none', 0,
                     'stamp', '2026-01-01T00:00:00Z'
                 );",
            )?;
            let incomplete_release_audio = sql.execute(
                "INSERT INTO release_files (
                     id, release_id, original_filename, file_size, content_type,
                     source_audio_layout, source_audio_content_type,
                     source_audio_duration_ms, source_audio_sample_rate_hz,
                     source_audio_bits_per_sample, source_audio_bitrate_kbps,
                     source_audio_channels, hash, _updated_at, created_at
                 ) VALUES (
                     '44444444-4444-4444-8444-444444444444',
                     '33333333-3333-4333-8333-333333333333', 'incomplete.flac', 100,
                     'audio/flac', 'cue', NULL, NULL, NULL, NULL, NULL, NULL,
                     ?1, 'stamp', '2026-01-01T00:00:00Z'
                 )",
                coven::rusqlite::params!["4".repeat(64)],
            );
            assert!(
                incomplete_release_audio.is_err(),
                "a release-file layout cannot exist without complete audio facts"
            );
            let contradictory_release_audio = sql.execute(
                "INSERT INTO release_files (
                     id, release_id, original_filename, file_size, content_type,
                     source_audio_layout, source_audio_content_type,
                     source_audio_duration_ms, source_audio_sample_rate_hz,
                     source_audio_bits_per_sample, source_audio_bitrate_kbps,
                     source_audio_channels, hash, _updated_at, created_at
                 ) VALUES (
                     '55555555-5555-4555-8555-555555555555',
                     '33333333-3333-4333-8333-333333333333', 'both.flac', 100,
                     'audio/flac', 'file', 'audio/flac', 1000, 44100, 16, 900, 2,
                     ?1, 'stamp', '2026-01-01T00:00:00Z'
                 )",
                coven::rusqlite::params!["5".repeat(64)],
            );
            assert!(
                contradictory_release_audio.is_err(),
                "lossless release-file facts cannot carry bitrate"
            );
            Ok(())
        })
        .await
        .expect("verify version-five audio constraints");
}

#[tokio::test]
#[serial]
async fn migration_six_rebuilds_scan_cache_without_byte_digests() {
    let temp = tempfile::tempdir().expect("temp store");
    let store_dir = StoreDir::new_ephemeral(temp.path());
    let handle = open(store_dir.clone(), "migration-scan-metadata", version_five())
        .expect("open version five");
    handle
        .write(|sql| {
            sql.execute_batch(
                "INSERT INTO watched_import_folders (path, position) VALUES ('/music', 0);
                 INSERT INTO folder_scan_roots (watched_folder_path, generation, status)
                     VALUES ('/music', 1, 'complete');",
            )?;
            Ok(())
        })
        .await
        .expect("seed version-five scan cache");
    drop(handle);

    let handle =
        open(store_dir, "migration-scan-metadata", version_six()).expect("migrate to version six");
    handle
        .read(|sql| {
            let counts: (i64, i64) = sql.query_row(
                "SELECT (SELECT COUNT(*) FROM watched_import_folders),
                        (SELECT COUNT(*) FROM folder_scan_roots)",
                [],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )?;
            assert_eq!(counts, (1, 0));
            let columns: Vec<String> = sql.query(
                "SELECT name FROM pragma_table_info('scan_candidate_file') ORDER BY cid",
                [],
                |row| row.get(0),
            )?;
            assert!(!columns.iter().any(|column| column == "content_digest"));
            Ok(())
        })
        .await
        .expect("read migrated scan schema");
}
