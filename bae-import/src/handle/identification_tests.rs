//! A person's decision about a candidate ends its identification in the
//! decision's own write: no delivery of the list shows the decision beside the
//! candidate still being identified. A cancel whose end cannot be written
//! fails and ends nothing.

use super::*;
use crate::handle::tests::*;
use crate::{ImportListSnapshot, ImportListView, PendingFilter, TriageRow, TriageTab};
use bae_catalogs::fake_catalogs::FakeCatalogs;
use bae_catalogs::test_releases::seed_discogs_pressing;
use bae_util::test_channels::HANG_GUARD;
use tempfile::TempDir;

/// Every candidate row a snapshot holds, with whether it reads as being
/// identified.
fn rows(snapshot: &ImportListSnapshot) -> Vec<(TriageRow, bool)> {
    snapshot
        .windows
        .iter()
        .flat_map(|window| &window.rows)
        .filter_map(|item| match item {
            crate::ImportListItem::Candidate { row, live, .. } => {
                Some((row.clone(), live.facts.identifying()))
            }
            _ => None,
        })
        .collect()
}

/// Follow the list under `view` while `decide` runs, until a delivery shows
/// the decision as `decided` reads it, and assert that delivery shows no
/// candidate still being identified — counted In Progress included, on Found.
async fn decision_never_beside_identifying(
    handle: &ImportServiceHandle,
    view: ImportListView,
    decide: impl std::future::Future<Output = ()> + Send + 'static,
    decided: impl Fn(&[(TriageRow, bool)]) -> bool,
) {
    let list = handle.subscribe_whole_list(view).await;
    tokio::time::timeout(HANG_GUARD, list.next())
        .await
        .expect("the list delivers")
        .expect("the list answers");
    let decision = tokio::spawn(decide);
    loop {
        let snapshot = tokio::time::timeout(HANG_GUARD, list.next())
            .await
            .expect("the list delivers the decision")
            .expect("the list answers");
        let rows = rows(&snapshot);
        if !decided(&rows) {
            continue;
        }
        let identifying: Vec<&crate::CandidateKey> = rows
            .iter()
            .filter(|(_, identifying)| *identifying)
            .map(|(row, _)| &row.candidate_key)
            .collect();
        assert!(
            identifying.is_empty(),
            "a delivery shows the decision beside {identifying:?} still being identified"
        );
        assert!(
            snapshot
                .pending_filter_entries
                .iter()
                .all(|entry| entry.filter != PendingFilter::InProgress || entry.count == 0),
            "a delivery counts a decided candidate In Progress"
        );
        break;
    }
    decision.await.expect("the decision ran");
}

/// Put `key` mid-run: queued, then its run started.
async fn identifying(handle: &ImportServiceHandle, key: &crate::CandidateKey) {
    admit(handle, key).await;
    start_run(handle, key, crate::IdentifyRunId::for_test(1)).await;
    assert!(handle.standing_facts_for_test(key).await.identifying());
}

fn found() -> ImportListView {
    ImportListView::default()
}

#[tokio::test(flavor = "multi_thread")]
async fn clearing_the_draft_ends_identification_in_its_write() {
    let (handle, _tmp, key, _hash) = pane_fixture().await;
    identifying(&handle, &key).await;
    let deciding = handle.clone();
    let decided_key = key.clone();
    decision_never_beside_identifying(
        &handle,
        found(),
        async move {
            deciding.clear_candidate_metadata(decided_key).await.unwrap();
        },
        |rows| {
            rows.iter()
                .any(|(row, _)| row.candidate_key == key && row.metadata_provenance.is_none())
        },
    )
    .await;
    shut_down(handle).await;
}

#[tokio::test(flavor = "multi_thread")]
async fn reading_the_files_tags_ends_identification_in_its_write() {
    let fixture = stored_candidate().await;
    let (handle, key) = (fixture.handle, fixture.key);
    identifying(&handle, &key).await;
    let deciding = handle.clone();
    let decided_key = key.clone();
    decision_never_beside_identifying(
        &handle,
        found(),
        async move {
            deciding.select_candidate_file_tags(decided_key).await.unwrap();
        },
        |rows| {
            rows.iter().any(|(row, _)| {
                row.candidate_key == key
                    && row.metadata_provenance == Some(crate::MetadataProvenance::FileMetadata)
            })
        },
    )
    .await;
    shut_down(handle).await;
}

#[tokio::test(flavor = "multi_thread")]
async fn a_pick_ends_identification_in_its_write() {
    let fakes = FakeCatalogs::offline();
    let (handle, _tmp, key, _hash) =
        pane_fixture_with(fakes.providers(), bae_util::http::Http::for_test()).await;
    handle
        .catalogs
        .store_discogs_key("test-discogs-token", bae_preferences::DiscogsValidation::Valid)
        .await
        .unwrap();
    let release_id = "70000301";
    seed_discogs_pressing(&fakes, release_id, "Label Name", "CAT-1");
    identifying(&handle, &key).await;
    let deciding = handle.clone();
    let decided_key = key.clone();
    decision_never_beside_identifying(
        &handle,
        found(),
        async move {
            deciding
                .select_candidate_release(
                    decided_key,
                    bae_identify::PressingLink {
                        record: bae_catalogs::records::LookupRef::new(
                            bae_catalogs::catalog::LookupCatalog::Discogs,
                            release_id.to_string(),
                        ),
                        partners: vec![],
                    },
                    crate::CatalogApplySteps::unread(),
                )
                .await
                .unwrap();
        },
        |rows| {
            rows.iter().any(|(row, _)| {
                row.candidate_key == key
                    && matches!(row.release_link, Some(crate::ReleaseLink::Pressing(_)))
            })
        },
    )
    .await;
    shut_down(handle).await;
}

#[tokio::test(flavor = "multi_thread")]
async fn linking_the_shared_album_ends_identification_in_its_write() {
    let fakes = FakeCatalogs::offline();
    let (handle, _tmp, key, hash) =
        pane_fixture_with(fakes.providers(), bae_util::http::Http::for_test()).await;
    store_two_pressings(&handle, &fakes, &hash).await;
    identifying(&handle, &key).await;
    let deciding = handle.clone();
    let decided_key = key.clone();
    decision_never_beside_identifying(
        &handle,
        found(),
        async move {
            deciding.link_candidate_shared_album(decided_key).await.unwrap();
        },
        |rows| {
            rows.iter().any(|(row, _)| {
                row.candidate_key == key
                    && matches!(row.release_link, Some(crate::ReleaseLink::Album(_)))
            })
        },
    )
    .await;
    shut_down(handle).await;
}

#[tokio::test(flavor = "multi_thread")]
async fn a_skip_ends_identification_in_its_write() {
    let (handle, _tmp, key, _hash) = pane_fixture().await;
    identifying(&handle, &key).await;
    let deciding = handle.clone();
    let decided_key = key.clone();
    decision_never_beside_identifying(
        &handle,
        ImportListView { tab: TriageTab::Skipped, ..ImportListView::default() },
        async move {
            deciding.set_candidate_skipped(decided_key, true).await.unwrap();
        },
        |rows| rows.iter().any(|(row, _)| row.candidate_key == key),
    )
    .await;
    shut_down(handle).await;
}

#[tokio::test(flavor = "multi_thread")]
async fn combining_ends_its_folders_identification_in_its_write() {
    let (parts, db, _library) = setup_test_import().await;
    let tables = crate::tables::ImportTables::new(db);
    let first_root = TempDir::new().unwrap();
    let second_root = TempDir::new().unwrap();
    let (_, first, _) = picked_candidate(&tables, &first_root, "Volume B").await;
    let (_, second, _) = picked_candidate(&tables, &second_root, "Volume A").await;
    let handle = parts.start().await.unwrap();
    // A folder waiting for or running a run is refused a combination; one
    // whose answer is being written is combined, and the answer ends.
    // Each run has its own id, as the identify service hands them out.
    for (run, key) in [(1, &first), (2, &second)] {
        handle
            .hold_answer_for_test(
                key,
                crate::IdentifyRunId::for_test(run),
                &crate::candidate_runtime::tests::manual_only(),
            )
            .await;
        assert!(handle.standing_facts_for_test(key).await.identifying());
    }
    let deciding = handle.clone();
    let members = vec![first.clone(), second.clone()];
    decision_never_beside_identifying(
        &handle,
        found(),
        async move {
            deciding.combine_candidates(members).await.unwrap();
        },
        |rows| rows.iter().any(|(row, _)| row.candidate_key.as_bytes().starts_with(b"grouping:")),
    )
    .await;
    for key in [&first, &second] {
        assert_eq!(
            handle.standing_facts_for_test(key).await.identification,
            None,
            "{key}'s identification ended with the combination"
        );
    }
    shut_down(handle).await;
}

/// A cancel of an identification whose end cannot be written fails to the
/// person who asked, and the candidate still reads as being identified.
#[tokio::test(flavor = "multi_thread")]
async fn a_cancel_whose_end_cannot_be_written_fails_and_ends_nothing() {
    let (handle, tmp, key, _hash) = pane_fixture().await;
    admit(&handle, &key).await;
    start_run(&handle, &key, crate::IdentifyRunId::for_test(1)).await;
    crate::tables::ImportTables::fail_standing_writes_leaving_for_test(
        tmp.path(),
        crate::tables::import_state::StandingColumn::Identification,
        "running",
    )
    .await;

    let error = handle.cancel_identification(&key).await.unwrap_err();

    assert!(error.to_string().contains("the disk is full"), "{error}");
    assert_eq!(
        handle.standing_facts_for_test(&key).await.identification,
        Some(crate::IdentificationStatus::Running)
    );
    shut_down(handle).await;
}
