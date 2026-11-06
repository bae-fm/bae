//! The import origin's table: one hash is one release on this device, and a
//! lookup by the hash uses its unique index.

use bae_records::test_fixtures::live_query::{live_db, RELEASE_ID};
use bae_store::test_fixtures::exec;

/// Whether this device imported a folder's files is a lookup by its hash,
/// which the hash's uniqueness indexes.
#[test]
fn imported_content_hash_lookup_uses_its_unique_index() {
    let conn = crate::tables::tests::bare_library();
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
    exec(
        &db,
        "INSERT INTO local_release_origin (release_id, content_hash) VALUES (?1, ?2)",
        &[RELEASE_ID, "folder-hash"],
    )
    .await;
    let mut copy = records.find_release_by_id(RELEASE_ID).await.unwrap().unwrap();
    copy.id = bae_util::test_ids::test_uuid("second-imported-copy");
    records.insert_release(&copy).await.unwrap();
    let second = copy.id.clone();
    let refused = db
        .call(move |conn| {
            conn.execute(
                "INSERT INTO local_release_origin (release_id, content_hash) VALUES (?1, ?2)",
                rusqlite::params![second, "folder-hash"],
            )
            .map(|_| ())
            .map_err(bae_store::DbError::from)
        })
        .await;
    assert!(refused.is_err(), "a second release of the same files is refused");
}
