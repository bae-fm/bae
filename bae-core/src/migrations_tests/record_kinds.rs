use super::*;
use coven::rusqlite::session::Session;

fn open_synced_history(
    directory: StoreDir,
    migrations: Vec<coven::Migration>,
) -> Result<coven::CovenHandle, CovenError> {
    crate::config::install_test_keyring();
    Coven::builder(directory, config("record-history"))
        .synced_tables(crate::sync::synced_tables())
        .coven_migration_policy(coven::CovenMigrationPolicy::ApplyPending)
        .clock(Arc::new(coven::SystemClock))
        .oauth_clients(crate::oauth::clients())
        .migrations(migrations)
        .open()
}

pub(super) fn open_record_history_fixture(
    directory: &StoreDir,
    version: usize,
) -> coven::rusqlite::Connection {
    let handle = open_synced_history(directory.clone(), all().into_iter().take(version).collect())
        .expect("open historical record schema");
    drop(handle);
    let connection =
        coven::rusqlite::Connection::open(directory.db_path()).expect("open historical capture");
    connection
        .execute_batch(
            "PRAGMA foreign_keys = ON;
             INSERT INTO artists (id, name, _updated_at, created_at)
             VALUES ('11111111-1111-4111-8111-111111111111', 'Artist Name', '1700000000000-0000-record-history', '2026-01-01T00:00:00Z');
             INSERT INTO albums (id, title, artist_id, is_compilation, _updated_at, created_at)
             VALUES ('22222222-2222-4222-8222-222222222222', 'Album Title', '11111111-1111-4111-8111-111111111111', 0, '1700000000000-0000-record-history', '2026-01-01T00:00:00Z');
             INSERT INTO releases (id, album_id, remote, _updated_at, created_at)
             VALUES ('33333333-3333-4333-8333-333333333333', '22222222-2222-4222-8222-222222222222', 0, '1700000000000-0000-record-history', '2026-01-01T00:00:00Z');",
        )
        .expect("seed the historical record's existing parents");
    connection
}

pub(super) fn migrate_record_history_fixture(directory: &StoreDir) -> coven::rusqlite::Connection {
    let handle = open_synced_history(directory.clone(), all()).expect("migrate record schema");
    drop(handle);
    let connection =
        coven::rusqlite::Connection::open(directory.db_path()).expect("open migrated receiver");
    connection
        .pragma_update(None, "foreign_keys", "ON")
        .expect("enforce receiver foreign keys");
    connection
}

#[tokio::test]
#[serial]
async fn historical_record_insert_applies_after_record_kind_migration() {
    for version in 38..=41 {
        let temp = tempfile::tempdir().expect("temp store");
        let directory = StoreDir::new_ephemeral(temp.path());
        let connection = open_record_history_fixture(&directory, version);
        let mut session = Session::new(&connection).expect("capture historical record insert");
        session
            .attach(Some("release_records"))
            .expect("attach actual record table");
        connection.execute(
        "INSERT INTO release_records (id, release_id, catalog, key, group_key, url, reads_draft, _updated_at, created_at)
         VALUES ('44444444-4444-4444-8444-444444444444', '33333333-3333-4333-8333-333333333333', 'discogs', '42', '84', 'https://www.discogs.com/release/42', 1, '1700000000000-0000-record-history', '2026-01-01T00:00:00Z')",
        [],
    ).expect("write the historical pressing");
        let mut changeset = Vec::new();
        session
            .changeset_strm(&mut changeset)
            .expect("capture original session bytes");
        drop(session);
        connection
            .execute("DELETE FROM release_records", [])
            .expect("retain the receiver's pre-insert state");
        drop(connection);

        let connection = migrate_record_history_fixture(&directory);
        // This wrapper builds TableSchema from the migrated database, validates
        // row identities, and executes Coven's actual conflict/application path.
        let applied = coven::resolve_and_apply_historical_changeset(
            &connection,
            &directory,
            all(),
            version as u32,
            &changeset,
            &crate::sync::synced_tables(),
            1_700_000_000_002,
        )
        .expect("admit and apply the historical record changeset");
        assert!(
            applied.constraint_conflict_tables.is_empty(),
            "historical pressing insertion must apply: {:?}",
            applied.constraint_conflict_tables
        );
        assert!(!applied.had_fk_violations);
        let record: (String, Option<String>) = connection
        .query_row(
            "SELECT kind, album_key FROM release_records WHERE id = '44444444-4444-4444-8444-444444444444'",
            [],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .expect("read imported pressing identity");
        assert_eq!(record, ("pressing".to_string(), Some("84".to_string())));
    }
}

#[tokio::test]
#[serial]
async fn historical_record_update_does_not_restore_an_unknown_parent_sentinel() {
    let temp = tempfile::tempdir().expect("temp store");
    let directory = StoreDir::new_ephemeral(temp.path());
    let connection = open_record_history_fixture(&directory, 41);
    connection.execute(
        "INSERT INTO release_records (id, release_id, catalog, key, group_key, url, reads_draft, _updated_at, created_at)
         VALUES ('44444444-4444-4444-8444-444444444444', '33333333-3333-4333-8333-333333333333', 'discogs', '42', '84', 'https://www.discogs.com/release/42', 1, '1700000000000-0000-record-history', '2026-01-01T00:00:00Z')",
        [],
    ).expect("seed a known parent before the historical update");
    let mut session = Session::new(&connection).expect("capture historical record update");
    session
        .attach(Some("release_records"))
        .expect("attach actual record table");
    connection
        .execute(
            "UPDATE release_records SET group_key = key, _updated_at = '1700000000001-0000-record-history' WHERE id = '44444444-4444-4444-8444-444444444444'",
            [],
        )
        .expect("capture the historical unknown-parent representation");
    let mut changeset = Vec::new();
    session
        .changeset_strm(&mut changeset)
        .expect("capture original session bytes");
    drop(session);
    connection.execute(
        "UPDATE release_records SET group_key = '84', _updated_at = '1700000000000-0000-record-history' WHERE id = '44444444-4444-4444-8444-444444444444'",
        [],
    ).expect("retain the receiver's pre-update state");
    drop(connection);

    let connection = migrate_record_history_fixture(&directory);
    let applied = coven::resolve_and_apply_historical_changeset(
        &connection,
        &directory,
        all(),
        41,
        &changeset,
        &crate::sync::synced_tables(),
        1_700_000_000_002,
    )
    .expect("admit and apply the historical record update");
    assert!(applied.constraint_conflict_tables.is_empty());
    assert!(!applied.had_fk_violations);
    assert_eq!(applied.winning_rows.len(), 1, "historical update wins");
    let record: (String, Option<String>) = connection
        .query_row(
            "SELECT kind, album_key FROM release_records WHERE id = '44444444-4444-4444-8444-444444444444'",
            [],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .expect("read updated pressing identity");
    assert_eq!(
        record,
        ("pressing".to_string(), None),
        "an uncorroborated equal key does not identify a parent album"
    );
}

#[tokio::test]
#[serial]
async fn origin_and_record_migrations_preserve_the_existing_sync_routing_contract() {
    let temp = tempfile::tempdir().expect("temp store");
    let directory = StoreDir::new_ephemeral(temp.path());
    crate::config::install_test_keyring();
    let open_synced = |migrations| {
        Coven::builder(directory.clone(), config("record-routing"))
            .synced_tables(crate::sync::synced_tables())
            .coven_migration_policy(coven::CovenMigrationPolicy::ApplyPending)
            .clock(Arc::new(coven::SystemClock))
            .oauth_clients(crate::oauth::clients())
            .migrations(migrations)
            .open()
    };
    let handle = open_synced(all().into_iter().take(38).collect())
        .expect("open and pin the existing synced schema");
    drop(handle);
    let handle = open_synced(all()).expect("upgrade without changing the pinned sync contract");
    handle
        .read(|sql| {
            let clock_column: i64 = sql.query_row(
                "SELECT cid FROM pragma_table_info('release_records') WHERE name = '_updated_at'",
                [],
                |row| row.get(0),
            )?;
            assert_eq!(
                clock_column, 7,
                "the routed clock retains its column ordinal"
            );
            Ok(())
        })
        .await
        .expect("read the migrated record clock");
}

#[tokio::test]
#[serial]
async fn record_kinds_use_canonical_parent_evidence_independently_of_local_archives() {
    let temp = tempfile::tempdir().expect("temp store");
    let directory = StoreDir::new_ephemeral(temp.path());
    let handle = open(
        directory.clone(),
        "record-kinds",
        all().into_iter().take(41).collect(),
    )
    .expect("open pressing-only records");
    handle.write(|sql| {
        sql.execute_batch(
            "INSERT INTO artists (id, name, _updated_at, created_at)
             VALUES ('artist', 'Artist Name', 'stamp', '2026-01-01T00:00:00Z');
             INSERT INTO albums (id, title, artist_id, is_compilation, _updated_at, created_at)
             VALUES ('album', 'Album Title', 'artist', 0, 'stamp', '2026-01-01T00:00:00Z');"
        )?;
        for (id, catalog, key, parent, document) in [
            ("equal-proven", "discogs", "42", "42", Some("{\"id\":42,\"master_id\":42}")),
            ("equal-unknown", "discogs", "43", "43", None),
            ("different", "musicbrainz", "release", "group", None),
            ("archived-parent", "musicbrainz", "release2", "release2", Some("{\"id\":\"release2\",\"release-group\":{\"id\":\"group2\"}}")),
            ("archived-ungrouped", "discogs", "44", "44", Some("{\"id\":44,\"master_id\":0}")),
            ("album-link", "allmusic", "mw42", "mw42", None),
            ("archived-alias", "musicbrainz", "release3", "release3", None),
        ] {
            sql.execute("INSERT INTO releases (id, album_id, remote, _updated_at, created_at) VALUES (?, 'album', 0, 'stamp', '2026-01-01T00:00:00Z')", [id])?;
            sql.execute("INSERT INTO release_records (id, release_id, catalog, key, group_key, url, reads_draft, _updated_at, created_at) VALUES (?, ?, ?, ?, ?, ?, ?, 'stamp', '2026-01-01T00:00:00Z')",
                coven::rusqlite::params![id, id, catalog, key, parent, format!("https://example.invalid/{id}"), catalog != "allmusic"])?;
            if let Some(document) = document {
                sql.execute("INSERT INTO source_release_payloads (source, source_release_id, json, fetched_at) VALUES (?, ?, ?, '2026-01-01T00:00:00Z')", coven::rusqlite::params![catalog, key, document])?;
            }
        }
        sql.execute("INSERT INTO source_release_payloads (source, source_release_id, json, fetched_at) VALUES ('musicbrainz_discogs_xref', '45', '{\"id\":\"release3\",\"release-group\":{\"id\":\"group3\"}}', '2026-01-01T00:00:00Z')", [])?;
        Ok(())
    }).await.expect("seed records and archived evidence");
    drop(handle);
    let handle = open(directory, "record-kinds", all()).expect("migrate record identities");
    handle.read(|sql| {
        let rows = sql.query("SELECT id, kind, album_key, reads_draft FROM release_records ORDER BY id", [], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?, row.get::<_, Option<String>>(2)?, row.get::<_, bool>(3)?))
        })?;
        let expected = [
            ("album-link", "album", None, false),
            ("archived-alias", "pressing", None, true),
            ("archived-parent", "pressing", None, true),
            ("archived-ungrouped", "pressing", None, true),
            ("different", "pressing", Some("group"), true),
            ("equal-proven", "pressing", None, true),
            ("equal-unknown", "pressing", None, true),
        ].map(|(id, kind, parent, reads)| (id.to_string(), kind.to_string(), parent.map(str::to_string), reads));
        assert_eq!(rows, expected);
        let preserved: i64 = sql.query_row("SELECT COUNT(*) FROM release_records rr JOIN releases r ON rr.release_id = r.id WHERE r.album_id = 'album' AND rr.id = r.id AND rr._updated_at = 'stamp' AND rr.created_at = '2026-01-01T00:00:00Z' AND rr.url = 'https://example.invalid/' || rr.id", [], |row| row.get(0))?;
        assert_eq!(preserved, 7, "preserve row identities, memberships, timestamps, and links");
        let documents: i64 = sql.query_row("SELECT COUNT(*) FROM source_release_payloads", [], |row| row.get(0))?;
        assert_eq!(documents, 4);
        assert!(sql.query::<String, _, _>("PRAGMA foreign_key_check", [], |row| row.get(0))?.is_empty());
        Ok(())
    }).await.expect("verify record upgrade");
}

#[tokio::test]
#[serial]
async fn historical_record_deletions_remove_pressings_and_album_links_after_migration() {
    for catalog in ["discogs", "allmusic"] {
        let temp = tempfile::tempdir().unwrap();
        let directory = StoreDir::new_ephemeral(temp.path());
        let connection = open_record_history_fixture(&directory, 41);
        let insert = "INSERT INTO release_records (id, release_id, catalog, key, group_key, url, reads_draft, _updated_at, created_at)
            VALUES ('44444444-4444-4444-8444-444444444444', '33333333-3333-4333-8333-333333333333', ?, '42', '42', 'https://example.invalid/42', 1, '1700000000000-0000-record-history', '2026-01-01T00:00:00Z')";
        connection.execute(insert, [catalog]).unwrap();
        let mut session = Session::new(&connection).unwrap();
        session.attach(Some("release_records")).unwrap();
        connection
            .execute("DELETE FROM release_records", [])
            .unwrap();
        let mut bytes = Vec::new();
        session.changeset_strm(&mut bytes).unwrap();
        drop(session);
        connection.execute(insert, [catalog]).unwrap();
        drop(connection);
        let connection = migrate_record_history_fixture(&directory);
        let applied = coven::resolve_and_apply_historical_changeset(
            &connection,
            &directory,
            all(),
            41,
            &bytes,
            &crate::sync::synced_tables(),
            1_700_000_000_002,
        )
        .unwrap();
        assert!(applied.constraint_conflict_tables.is_empty(), "{catalog}");
        assert!(!applied.had_fk_violations, "{catalog}");
        let count: i64 = connection
            .query_row("SELECT COUNT(*) FROM release_records", [], |row| row.get(0))
            .unwrap();
        assert_eq!(count, 0, "{catalog}");
    }
}
