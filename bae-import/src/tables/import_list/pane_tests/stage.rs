//! What a candidate of a grouping or a skip is, as the pane reads it.

use super::*;

/// The release of a grouping reads as a candidate; the person's skip of a
/// release reads as skipped, and the skip of a grouping skips the grouping's
/// release.
#[tokio::test]
async fn the_pane_reads_a_grouped_and_a_skipped_candidate() {
    let lib = Library::new().await;
    lib.run(lib.scan("one") + lib.scan("two") + lib.scan("three")).await;
    lib.run(lib.combine(&["one", "two"], "pair")).await;
    lib.run(lib.skip("three")).await;
    let tables = crate::tables::ImportTables::new(lib.db.clone());
    let read = |key: crate::CandidateKey| {
        let tables = tables.clone();
        async move {
            tables
                .load_import_candidate(&key)
                .await
                .unwrap()
                .expect("the list lists the key")
                .resolve()
        }
    };

    let grouping = read(crate::CandidateKey::grouping("pair")).await;
    assert!(grouping.actionable);
    assert!(!grouping.skipped);
    assert!(matches!(grouping.stage, crate::CandidateStage::Drafting { .. }));

    assert!(read(lib.key("three")).await.skipped);

    lib.run(lib.grouping_skipped("pair", true)).await;
    assert!(read(crate::CandidateKey::grouping("pair")).await.skipped);
}
