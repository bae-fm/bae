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

/// Returns each row of the runtime's typed search as its lead release's ID and
/// whether bae linked the candidate to that row.
fn chosen_rows(runtime: &crate::CandidateRuntimeSnapshot) -> Vec<(String, bool)> {
    let mut rows: Vec<(String, bool)> = runtime
        .search
        .iter()
        .flat_map(|search| &search.groups)
        .flat_map(|group| &group.sections)
        .flat_map(|section| &section.pressings)
        .map(|pressing| (pressing.lead().release_id.clone(), pressing.choice().chosen))
        .collect();
    rows.sort();
    rows
}

/// The typed search's SQL query marks the row that holds the release bae
/// linked the candidate to. It marks no row before the person picks a release,
/// and it marks the picked row after bae stores the link.
#[tokio::test(flavor = "multi_thread")]
async fn the_row_holding_the_linked_release_reads_as_chosen() {
    let fixture = Fixture::new("search-row-chosen").await;
    fixture.set_identify_automatically(false).await;
    let dir = fixture.disc_id_candidate("Album");
    let key = crate::CandidateKey::folder(&dir);
    let probed = fixture.probed_total_ms(&dir);
    fixture.provider.route(
        "/release?",
        200,
        r#"{"releases":[
            {"id":"mb-picked","title":"Album","artist-credit":[{"name":"Artist"}],
             "release-group":{"id":"rg-typed"},"label-info":[]},
            {"id":"mb-other","title":"Album","artist-credit":[{"name":"Artist"}],
             "release-group":{"id":"rg-typed"},"label-info":[]}]}"#
            .to_string(),
    );
    fixture.provider.route(
        "/release/mb-picked?",
        200,
        mb_release_json("mb-picked", "rg-typed", &[probed, 0]),
    );
    fixture.scan(1).await;

    fixture
        .import
        .start_candidate_search(
            key.clone(),
            SearchQuery::General { artist: "Artist".to_string(), album: "Album".to_string() },
        )
        .await
        .unwrap();
    let rows =
        |picked: bool| vec![("mb-other".to_string(), false), ("mb-picked".to_string(), picked)];
    fixture.await_runtime(&key, |runtime| chosen_rows(runtime) == rows(false)).await;

    fixture
        .import
        .select_candidate_release(
            key.clone(),
            bae_identify::PressingLink {
                record: bae_catalogs::records::LookupRef::new(
                    LookupCatalog::MusicBrainz,
                    "mb-picked".to_string(),
                ),
                partners: vec![],
            },
            crate::CatalogApplySteps::unread(),
        )
        .await
        .unwrap();
    fixture.await_runtime(&key, |runtime| chosen_rows(runtime) == rows(true)).await;
}
