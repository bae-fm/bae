use super::*;

fn version_twenty() -> Vec<coven::Migration> {
    all().into_iter().take(20).collect()
}

#[tokio::test]
#[serial]
async fn migration_twenty_one_preserves_documents_and_backfills_optional_references() {
    let temp = tempfile::tempdir().expect("temp store");
    let directory = StoreDir::new_ephemeral(temp.path());
    let handle =
        open(directory.clone(), "document-migration", version_twenty()).expect("old schema");
    handle.write(|sql| {
        for (source, id, json) in [
            ("musicbrainz", "release", r#"{"id":"release","release-group":{"id":"group","relations":[{"url":{"resource":"https://www.discogs.com/release/123-Album-Title"}}]}}"#),
            ("discogs", "123", r#"{"id":123,"master_id":456}"#),
            ("discogs_master", "456", r#"{"id":456,"year":1990}"#),
            ("musicbrainz_release_group", "group", "{}"),
            ("musicbrainz_discogs_xref", "123", "{}"),
        ] {
            sql.execute("INSERT INTO source_release_payloads (source, source_release_id, json, fetched_at) VALUES (?, ?, ?, 'stamp')",
                coven::rusqlite::params![source, id, json])?;
        }
        Ok(())
    }).await.expect("archive previous documents");
    drop(handle);
    let handle = open(directory, "document-migration", all()).expect("migrate documents");
    handle.read(|sql| {
        let references = sql.query("SELECT source, source_release_id, target_source, target_id FROM source_document_reference ORDER BY source, target_source", [],
            |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?, row.get::<_, String>(2)?, row.get::<_, String>(3)?)))?;
        let expected = [
            ("discogs", "123", "discogs_master", "456"),
            ("discogs", "123", "musicbrainz_discogs_xref", "123"),
            ("musicbrainz", "release", "discogs", "123"),
            ("musicbrainz", "release", "musicbrainz_release_group", "group"),
        ].map(|(a,b,c,d)| (a.to_string(), b.to_string(), c.to_string(), d.to_string()));
        assert_eq!(references, expected);
        let groups = sql.query("SELECT source, source_group_id FROM source_release_payloads WHERE source_group_id IS NOT NULL ORDER BY source", [],
            |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?)))?;
        assert_eq!(groups, vec![("discogs".to_string(), "456".to_string()), ("musicbrainz".to_string(), "group".to_string())]);
        let counts: (i64, i64) = sql.query_row("SELECT COUNT(*), COUNT(*) FILTER (WHERE fetched_at = 'stamp') FROM source_release_payloads", [], |row| Ok((row.get(0)?, row.get(1)?)))?;
        assert_eq!(counts, (5,5));
        let json: String = sql.query_row("SELECT json FROM source_release_payloads WHERE source = 'discogs_master'", [], |row| row.get(0))?;
        assert_eq!(json, r#"{"id":456,"year":1990}"#);
        Ok(())
    }).await.expect("check preserved documents and references");
    handle
        .write(|sql| {
            sql.execute(
                "DELETE FROM source_release_payloads WHERE source = 'discogs'",
                [],
            )?;
            let count: i64 = sql.query_row(
                "SELECT COUNT(*) FROM source_document_reference WHERE source = 'discogs'",
                [],
                |row| row.get(0),
            )?;
            assert_eq!(count, 0, "owner deletion cascades its references");
            let count: i64 = sql.query_row(
                "SELECT COUNT(*) FROM source_document_reference WHERE target_source = 'discogs'",
                [],
                |row| row.get(0),
            )?;
            assert_eq!(count, 1, "an unfetched target remains a valid reference");
            Ok(())
        })
        .await
        .expect("verify optional target ownership");
}

#[tokio::test]
#[serial]
async fn migration_twenty_one_rejects_unreadable_relationships_without_changing_schema_or_rows() {
    let temp = tempfile::tempdir().expect("temp store");
    let directory = StoreDir::new_ephemeral(temp.path());
    let handle = open(
        directory.clone(),
        "document-migration-error",
        version_twenty(),
    )
    .expect("old schema");
    handle.write(|sql| {
        sql.execute("INSERT INTO source_release_payloads (source, source_release_id, json, fetched_at) VALUES ('musicbrainz', 'invalid', '{\"relations\":42}', 'stamp')", [])?;
        Ok(())
    }).await.expect("archive unreadable relationship fields");
    drop(handle);
    let error = match open(directory.clone(), "document-migration-error", all()) {
        Ok(_) => panic!("unreadable relationships must fail migration"),
        Err(error) => error,
    };
    assert!(matches!(
        error,
        CovenError::Migration(MigrationError::Failed { version: 21, .. })
    ));
    let sql = coven::rusqlite::Connection::open(directory.db_path())
        .expect("inspect rolled-back test store");
    let version: i64 = sql
        .pragma_query_value(None, "user_version", |row| row.get(0))
        .expect("schema version");
    assert_eq!(version, 20);
    let references: i64 = sql
        .query_row(
            "SELECT COUNT(*) FROM sqlite_master WHERE name = 'source_document_reference'",
            [],
            |row| row.get(0),
        )
        .expect("new table absent");
    assert_eq!(references, 0);
    let group_column: i64 = sql.query_row("SELECT COUNT(*) FROM pragma_table_info('source_release_payloads') WHERE name = 'source_group_id'", [], |row| row.get(0)).expect("new column absent");
    assert_eq!(group_column, 0);
    let json: String = sql
        .query_row("SELECT json FROM source_release_payloads", [], |row| {
            row.get(0)
        })
        .expect("preserved payload");
    assert_eq!(json, "{\"relations\":42}");
}
