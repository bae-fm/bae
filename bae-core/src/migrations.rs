//! bae's synced-schema migration ladder, registered on the coven builder.
//! coven runs each migration whose version exceeds the db's `PRAGMA user_version`
//! at open; the applied version becomes the wire `schema_version`.

const IMPORT_METADATA_SEEDS_SQL: &str = include_str!("../migrations/002_import_metadata_seeds.sql");
const VERSION_ONE_FILE_TAG_TRACK_PREFIX: &str = "unknown-track-";
const FILE_TAG_TRACK_PREFIX: &str = "file-tag-track-";

struct VersionOneTrackEdit {
    content_hash: String,
    old_track_id: String,
    track_id: String,
    artist_names: Option<Vec<String>>,
}

/// The ordered migration ladder. Versions are 1-based and contiguous.
pub fn all() -> Vec<coven::Migration> {
    vec![
        coven::Migration::sql(1, "initial", include_str!("../migrations/001_initial.sql")),
        coven::Migration::run(2, "import_metadata_seeds", migrate_import_metadata_seeds),
    ]
}

fn migrate_import_metadata_seeds(sql: &coven::MigrationContext<'_>) -> Result<(), coven::DbError> {
    let album_artist_edits = sql.query(
        "SELECT content_hash, album_artist_text FROM import_candidate_edit \
         WHERE album_artist_text IS NOT NULL ORDER BY content_hash",
        [],
        |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?)),
    )?;
    let track_edits = sql.query(
        "SELECT edit.content_hash, edit.track_id, state.pick_kind, edit.artist_text \
         FROM import_candidate_track_edit AS edit \
         JOIN import_candidate_state AS state USING (content_hash) \
         ORDER BY edit.content_hash, edit.track_id",
        [],
        |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, Option<String>>(2)?,
                row.get::<_, Option<String>>(3)?,
            ))
        },
    )?;

    let album_artist_edits = parse_v1_artist_edits(album_artist_edits, "album")?;
    let track_edits = track_edits
        .into_iter()
        .map(|(content_hash, old_track_id, pick_kind, artist_text)| {
            let track_id = version_two_track_id(pick_kind.as_deref(), &old_track_id);
            let artist_names = artist_text.map(|text| parse_v1_artist_text(&text));
            VersionOneTrackEdit {
                content_hash,
                old_track_id,
                track_id,
                artist_names,
            }
        })
        .collect::<Vec<_>>();

    sql.execute_batch(IMPORT_METADATA_SEEDS_SQL)?;

    for edit in &track_edits {
        if edit.old_track_id != edit.track_id {
            sql.execute(
                "UPDATE import_candidate_track_edit SET track_id = ? \
                 WHERE content_hash = ? AND track_id = ?",
                coven::rusqlite::params![edit.track_id, edit.content_hash, edit.old_track_id],
            )?;
        }
    }

    for (content_hash, names) in album_artist_edits {
        for (position, name) in names.into_iter().enumerate() {
            sql.execute(
                "INSERT INTO import_candidate_album_artist_assignment \
                 (content_hash, position, assignment_kind, artist_id, name, sort_name, \
                  musicbrainz_artist_id, discogs_artist_id) \
                 VALUES (?, ?, 'new', NULL, ?, NULL, NULL, NULL)",
                coven::rusqlite::params![content_hash, position as i64, name],
            )?;
        }
    }
    for edit in track_edits {
        if let Some(names) = edit.artist_names {
            for (position, name) in names.into_iter().enumerate() {
                sql.execute(
                    "INSERT INTO import_candidate_track_artist_assignment \
                     (content_hash, track_id, position, assignment_kind, artist_id, name, sort_name, \
                      musicbrainz_artist_id, discogs_artist_id) \
                     VALUES (?, ?, ?, 'new', NULL, ?, NULL, NULL, NULL)",
                    coven::rusqlite::params![edit.content_hash, edit.track_id, position as i64, name],
                )?;
            }
        }
    }

    sql.execute_batch(
        "DROP TABLE import_candidate_signal_value_v1; \
         DROP TABLE import_candidate_match_v1; \
         DROP TABLE import_candidate_file_edit_v1; \
         DROP TABLE import_candidate_file_duration_v1; \
         DROP TABLE import_candidate_failure_v1; \
         DROP TABLE import_candidate_cover_v1; \
         DROP TABLE import_candidate_edit_v1; \
         DROP TABLE import_candidate_track_edit_v1; \
         DROP TABLE import_candidate_signals_v1; \
         DROP TABLE import_candidate_state_v1;",
    )?;

    let violations = sql.query("PRAGMA foreign_key_check", [], |row| {
        Ok((
            row.get::<_, String>(0)?,
            row.get::<_, Option<i64>>(1)?,
            row.get::<_, String>(2)?,
            row.get::<_, i64>(3)?,
        ))
    })?;
    if !violations.is_empty() {
        return Err(coven::DbError::Message(format!(
            "migration 2 produced foreign-key violations: {violations:?}"
        )));
    }
    Ok(())
}

fn version_two_track_id(pick_kind: Option<&str>, track_id: &str) -> String {
    match (
        pick_kind,
        track_id.strip_prefix(VERSION_ONE_FILE_TAG_TRACK_PREFIX),
    ) {
        (Some("unknown"), Some(index)) => format!("{FILE_TAG_TRACK_PREFIX}{index}"),
        _ => track_id.to_string(),
    }
}

fn parse_v1_artist_edits(
    edits: Vec<(String, String)>,
    field: &str,
) -> Result<Vec<(String, Vec<String>)>, coven::DbError> {
    edits
        .into_iter()
        .map(|(content_hash, text)| {
            let names = parse_v1_artist_text(&text);
            if names.is_empty() {
                return Err(coven::DbError::Message(format!(
                    "candidate {content_hash} has an empty version-1 {field} artist edit"
                )));
            }
            Ok((content_hash, names))
        })
        .collect()
}

fn parse_v1_artist_text(raw: &str) -> Vec<String> {
    raw.split(',')
        .map(str::trim)
        .filter(|name| !name.is_empty())
        .map(str::to_string)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{TimeZone, Utc};
    use coven::{Coven, CovenError, FixedClock, MigrationError, StoreDir};
    use serial_test::serial;
    use std::sync::Arc;

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

    #[tokio::test]
    #[serial]
    async fn migration_two_preserves_candidate_graph_and_normalizes_artist_edits() {
        let temp = tempfile::tempdir().expect("temp store");
        let store_dir = StoreDir::new_ephemeral(temp.path());
        let handle = open(store_dir.clone(), "migration-preserves", version_one())
            .expect("open version one");
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

        let handle = open(store_dir, "migration-preserves", all()).expect("migrate to version two");
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
                    "import_candidate_file_duration",
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
        let handle = open(store_dir.clone(), "migration-track-ids", version_one())
            .expect("open version one");
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

        let handle = open(store_dir, "migration-track-ids", all()).expect("migrate File Tags edit");
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

        let error = match open(store_dir.clone(), "migration-rollback", all()) {
            Ok(_) => panic!("empty artist override must reject migration"),
            Err(error) => error,
        };
        assert!(matches!(
            error,
            CovenError::Migration(MigrationError::Failed { version: 2, .. })
        ));

        let connection = coven::rusqlite::Connection::open(store_dir.db_path())
            .expect("open rolled-back database");
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
}
