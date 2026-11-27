//! A playlist's page finds its entries along their index.

use super::*;

#[tokio::test]
async fn the_page_reads_its_entries_along_their_index() {
    let (db, _temp) = bae_store::test_fixtures::temp_db(crate::TEST_SCHEMA).await;
    let plan = db
        .read(|sql| {
            Ok(sql.query(
                &format!("EXPLAIN QUERY PLAN {}", PlaylistPage::SQL),
                rusqlite::named_params! { ":playlist": "id" },
                |row| row.get::<_, String>(3),
            )?)
        })
        .await
        .unwrap();
    assert!(
        !plan.iter().any(|step| step.starts_with("SCAN")
            && (step.contains("entry") || step.contains("playlist_entries"))),
        "the page scans playlist entries: {plan:?}"
    );
}
