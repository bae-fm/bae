//! A playlist's page finds its entries along their index.

use super::*;
use bae_store::test_fixtures::query_plan::query_plan;

#[tokio::test]
async fn the_page_reads_its_entries_along_their_index() {
    let (db, _temp) = bae_store::test_fixtures::temp_db(crate::TEST_SCHEMA).await;
    let plan = query_plan(&db, PlaylistPageParams { playlist: Some("id".into()) }).await;
    assert!(
        !plan.iter().any(|step| step.starts_with("SCAN")
            && (step.contains("entry") || step.contains("playlist_entries"))),
        "the page scans playlist entries: {plan:?}"
    );
}
