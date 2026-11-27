//! The import origin's table: one hash is one release on this device, and a
//! lookup by the hash uses its unique index.

use bae_records::test_fixtures::live_query::{live_db, RELEASE_ID};

/// Whether this device imported a folder's files is a lookup by its hash,
/// which the hash's uniqueness indexes.
#[test]
fn imported_content_hash_lookup_uses_its_unique_index() {
    let conn = rusqlite::Connection::open_in_memory().expect("an in-memory database opens");
    crate::TEST_SCHEMA.open_bare_library(&conn);
    let plan: Vec<String> = conn
        .prepare(
            "EXPLAIN QUERY PLAN \
             SELECT release_id FROM local_release_origin WHERE content_hash = ?",
        )
        .unwrap()
        .query_map(["hash"], |row| row.get(3))
        .unwrap()
        .collect::<Result<_, _>>()
        .unwrap();

    assert!(
        plan.iter()
            .any(|detail| detail.contains("INDEX") && detail.contains("local_release_origin")),
        "query plan did not use the content-hash index: {plan:?}"
    );
}

/// One hash is one release on this device: a second release recorded against
/// files this device already imported is refused.
#[tokio::test]
async fn one_folder_hash_records_one_release() {
    let (db, _temp) = live_db(crate::TEST_SCHEMA).await;
    let records = bae_records::test_fixtures::test_records(&db);
    let first = db
        .call(|sql| super::record_on(sql, RELEASE_ID, "folder-hash"))
        .await
        .expect("the release's origin records");
    assert!(matches!(first, super::OriginRecord::Recorded));
    let mut copy = records.find_release_by_id(RELEASE_ID).await.unwrap().unwrap();
    copy.id = bae_util::test_ids::test_uuid("second-imported-copy");
    records.insert_release(&copy).await.unwrap();
    let second = copy.id.clone();
    // The answer leaves the write apart from its result: the insert it comes
    // from failed, which fails the write.
    let (answered, answer) = std::sync::mpsc::channel();
    let written = db
        .call(move |sql| {
            answered.send(super::record_on(sql, &second, "folder-hash")?).expect("the test waits");
            Ok(())
        })
        .await;
    assert!(written.is_err(), "a second release of the same files is refused");
    assert!(
        matches!(
            answer.recv(),
            Ok(super::OriginRecord::Taken { release_id }) if release_id == RELEASE_ID
        ),
        "the refusal names the release that holds the files"
    );
}
