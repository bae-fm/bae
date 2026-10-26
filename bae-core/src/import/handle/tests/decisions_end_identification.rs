//! A person's decision about a candidate ends its identification inside the
//! decision's own write: no delivery of the list shows the decision beside
//! the candidate still being identified.

use super::*;
use crate::import::{ImportListSnapshot, ImportListView, PendingFilter, TriageRow, TriageTab};

/// Every candidate row a snapshot holds, with whether it reads as being
/// identified.
fn rows(snapshot: &ImportListSnapshot) -> Vec<(TriageRow, bool)> {
    snapshot
        .windows
        .iter()
        .flat_map(|window| &window.items)
        .filter_map(|item| match item {
            crate::import::ImportListItem::Candidate { row, live, .. } => {
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
    let list = handle.subscribe_whole_list(view);
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
        let identifying: Vec<&str> = rows
            .iter()
            .filter(|(_, identifying)| *identifying)
            .map(|(row, _)| row.candidate_key.as_str())
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
async fn identifying(handle: &ImportServiceHandle, key: &str) {
    admit(handle, key).await;
    start_run(handle, key, crate::identify::IdentifyRunId::for_test(1)).await;
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
            deciding
                .select_candidate_file_tags(decided_key)
                .await
                .unwrap();
        },
        |rows| {
            rows.iter().any(|(row, _)| {
                row.candidate_key == key
                    && row.metadata_provenance
                        == Some(crate::import::MetadataProvenance::FileMetadata)
            })
        },
    )
    .await;
    shut_down(handle).await;
}

#[tokio::test(flavor = "multi_thread")]
async fn a_pick_ends_identification_in_its_write() {
    let (handle, _tmp, key, _hash) = pane_fixture().await;
    handle
        .library_manager
        .set_discogs_key("test-discogs-token", crate::config::DiscogsValidation::Valid)
        .await
        .unwrap();
    let release_id = "70000301";
    super::metadata_application::seed_discogs_pressing(
        handle.library_manager.providers(),
        release_id,
        "Label Name",
        "CAT-1",
    );
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
                    crate::import::PressingLink {
                        record: crate::import::MetadataRef::new(
                            crate::catalog::Catalog::Discogs,
                            release_id.to_string(),
                        ),
                        partners: vec![],
                    },
                )
                .await
                .unwrap();
        },
        |rows| {
            rows.iter().any(|(row, _)| {
                row.candidate_key == key
                    && matches!(row.release_link, Some(crate::import::ReleaseLink::Pressing(_)))
            })
        },
    )
    .await;
    shut_down(handle).await;
}

#[tokio::test(flavor = "multi_thread")]
async fn linking_the_shared_album_ends_identification_in_its_write() {
    let (handle, _tmp, key, hash) = pane_fixture().await;
    super::shared_album::store_two_pressings(&handle, &hash).await;
    identifying(&handle, &key).await;
    let deciding = handle.clone();
    let decided_key = key.clone();
    decision_never_beside_identifying(
        &handle,
        found(),
        async move {
            deciding
                .link_candidate_shared_album(decided_key)
                .await
                .unwrap();
        },
        |rows| {
            rows.iter().any(|(row, _)| {
                row.candidate_key == key
                    && matches!(row.release_link, Some(crate::import::ReleaseLink::Album(_)))
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
        ImportListView {
            tab: TriageTab::Skipped,
            ..ImportListView::default()
        },
        async move {
            deciding
                .set_candidate_skipped(decided_key, true)
                .await
                .unwrap();
        },
        |rows| rows.iter().any(|(row, _)| row.candidate_key == key),
    )
    .await;
    shut_down(handle).await;
}

#[tokio::test(flavor = "multi_thread")]
async fn combining_ends_its_folders_identification_in_its_write() {
    let (manager, _library) = setup_test_manager().await;
    let first_root = TempDir::new().unwrap();
    let second_root = TempDir::new().unwrap();
    let (_, first, _) = picked_candidate(&manager, &first_root, "Volume B").await;
    let (_, second, _) = picked_candidate(&manager, &second_root, "Volume A").await;
    let handle = manager
        .start_import_service(tokio::runtime::Handle::current())
        .await
        .unwrap();
    // A folder waiting for or running a run is refused a combination; one
    // whose answer is being written is combined, and the answer ends.
    for key in [&first, &second] {
        handle
            .hold_answer_for_test(
                key,
                crate::identify::IdentifyRunId::for_test(1),
                &crate::import::candidate_runtime::tests::manual_only(),
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
        |rows| {
            rows.iter().any(|(row, _)| row.candidate_key.starts_with("grouping:"))
        },
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
