// ── Where a row stands, ended with the result that ends it ──────────────────
//
// A run's verdict and the end of its run are one write, as are an import's
// release and the end of its import: every delivery of the list shows the
// row still going or already answered, never neither. The library opens with
// no standing, since no run or import outlives the process that held it.

/// Each candidate row a snapshot's page holds, with its live state, by key.
fn page_rows(
    snapshot: &crate::import::ImportListSnapshot,
) -> BTreeMap<String, (crate::import::TriageRow, crate::import::CandidateLiveState)> {
    snapshot
        .windows
        .iter()
        .flat_map(|window| &window.items)
        .filter_map(|item| match item {
            crate::import::ImportListItem::Candidate { row, live, .. } => {
                Some((row.candidate_key.clone(), (row.clone(), live.clone())))
            }
            _ => None,
        })
        .collect()
}

/// A run reads as identifying in every delivery until the one that carries
/// its stored verdict: no delivery shows the row neither going nor answered,
/// or both.
#[tokio::test(flavor = "multi_thread")]
async fn a_run_reads_as_identifying_until_its_verdict_is_stored() {
    let fixture = Fixture::new("standing-run-to-verdict").await;
    fixture
        .manager
        .set_identify_automatically(false)
        .await
        .unwrap();
    let dir = fixture.disc_id_candidate("Album");
    let key = dir.to_string_lossy().into_owned();
    fixture.route_disc_id_match(&dir, "mb-standing", "rg-standing", 2);
    fixture.scan(1).await;

    let list = list_subscription(&fixture, page_window(0, 50));
    let initial = next_page(&list).await;
    let (_, at_rest) = &page_rows(&initial)[&key];
    assert_eq!(at_rest.facts.identification, None);
    assert_eq!(at_rest.standing, Some(crate::import::PendingStanding::NotLookedUp));

    fixture.identification().rerun_identify(key.clone());
    let mut identifying = 0;
    let answered = loop {
        let snapshot = next_page(&list).await;
        let (row, live) = page_rows(&snapshot).remove(&key).expect("the row stays on Found");
        if live.facts.identifying() {
            assert!(
                row.action_basis.lookup.is_none(),
                "a delivery shows the stored verdict beside the run still going: {live:?}"
            );
            identifying += 1;
            continue;
        }
        if identifying == 0 {
            continue;
        }
        assert!(
            row.action_basis.lookup.is_some(),
            "a delivery shows the row neither identifying nor answered: {live:?}"
        );
        break live;
    };
    assert!(identifying > 0, "the run read as identifying");
    assert_eq!(answered.facts.identification, None);
    assert_eq!(answered.standing, Some(crate::import::PendingStanding::Identified));
}

/// An import reads as importing in every delivery until the one in which its
/// release takes the row to Done: no delivery shows the row on Found with its
/// import ended, gone from Found with no release, or on Done still importing.
#[tokio::test(flavor = "multi_thread")]
async fn an_import_reads_as_importing_until_its_release_is_written() {
    let fixture = Fixture::importing("standing-import-to-done").await;
    let dir = fixture.disc_id_candidate("Album");
    let key = dir.to_string_lossy().into_owned();
    fixture
        .identify_ready(&dir, "mb-standing-import", "rg-standing-import")
        .await;

    let list = list_subscription(&fixture, page_window(0, 50));
    let initial = next_page(&list).await;
    assert!(!page_rows(&initial)[&key].1.facts.importing());
    assert_eq!(initial.summary.counts.done, 0);
    // The pane reads where the candidate stands and whether this device
    // imported it in one read; it is followed beside the list so no read of
    // it is passed over.
    let mut pane = fixture.import.subscribe_candidate_pane(&key);
    pane.next().await.result.unwrap();
    let pane_saw_both = tokio::spawn(async move {
        loop {
            let detail = pane
                .next()
                .await
                .result
                .unwrap()
                .expect("the candidate stays stored")
                .resolve();
            if detail.is_added {
                return detail.live.facts.importing();
            }
        }
    });

    fixture.import.start_import(&key).await.unwrap();
    let mut importing = 0;
    loop {
        let snapshot = next_page(&list).await;
        match page_rows(&snapshot).remove(&key) {
            Some((_, live)) if live.facts.importing() => importing += 1,
            Some((_, live)) => assert_eq!(
                importing, 0,
                "a delivery shows the row on Found with its import ended: {live:?}"
            ),
            None => {
                assert!(importing > 0, "the import read as importing");
                assert_eq!(
                    snapshot.summary.counts.done, 1,
                    "the row left Found without its release"
                );
                break;
            }
        }
    }
    assert!(
        !tokio::time::timeout(Duration::from_secs(20), pane_saw_both)
            .await
            .expect("the pane reads the release")
            .expect("the pane is followed to the end"),
        "a read shows the release beside the import still going"
    );
}

/// A standing a crash left behind is gone once the library opens again.
#[tokio::test(flavor = "multi_thread")]
async fn the_library_opens_with_no_standing_a_crash_left() {
    let fixture = Fixture::new("standings-cleared-on-open").await;
    fixture
        .database
        .write_candidate_standings(vec![
            (
                "/watched/Running".to_string(),
                crate::import::TriageRuntimeFacts {
                    identification: Some(crate::import::IdentificationStatus::Running),
                    import: None,
                },
            ),
            (
                "/watched/Writing".to_string(),
                crate::import::TriageRuntimeFacts {
                    identification: None,
                    import: Some(crate::import::ImportStanding::Writing),
                },
            ),
        ])
        .await
        .unwrap();
    assert_eq!(fixture.database.load_candidate_standings().await.unwrap().len(), 2);

    let reopened = fixture
        .manager
        .start_import_service(tokio::runtime::Handle::current())
        .await
        .unwrap();

    assert!(fixture.database.load_candidate_standings().await.unwrap().is_empty());
    tokio::task::spawn_blocking(move || reopened.stop_and_join())
        .await
        .expect("the reopened import service stops");
}
