//! A typed search's answers landing on the stored search.

use super::*;
use crate::identification::tests::*;
use bae_catalogs::test_releases::{mb_release_json, mb_search_json};

/// A source's answer that cannot be saved is stored as that source's failure,
/// so its row stops searching and Retry asks it again.
#[tokio::test(flavor = "multi_thread")]
async fn an_answer_that_cannot_be_saved_fails_its_source() {
    let fixture = Fixture::new("search-answer-unsaved").await;
    fixture.set_identify_automatically(false).await;
    let dir = fixture.disc_id_candidate("Album");
    let key = crate::CandidateKey::folder(&dir);
    let probed = fixture.probed_total_ms(&dir);
    fixture.provider.route("/release?", 200, mb_search_json("mb-typed", "rg-typed"));
    fixture.provider.route(
        "/release/mb-typed?",
        200,
        mb_release_json("mb-typed", "rg-typed", &[probed, 0]),
    );
    fixture.scan(1).await;
    crate::tables::ImportTables::fail_search_match_writes_for_test(fixture._temp.path()).await;

    fixture
        .import
        .start_candidate_search(
            key.clone(),
            SearchQuery::General { artist: "Artist".to_string(), album: "Album".to_string() },
        )
        .await
        .unwrap();

    fixture
        .await_runtime(&key, |runtime| {
            runtime.search.as_ref().is_some_and(|search| {
                matches!(
                    search.source(LookupCatalog::MusicBrainz),
                    Some(crate::SourceSearch::Failed(bae_catalogs::Failure::Internal(failure)))
                        if failure.detail.contains("the disk is full")
                )
            })
        })
        .await;
}
