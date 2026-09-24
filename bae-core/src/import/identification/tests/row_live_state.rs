// ── What is running for a row, beside the list ──────────────────────────────
//
// The list reads the tables and nothing else; what is running for each
// candidate reaches its row through that row's own live-state subscription.

/// Every candidate row the Pending tab holds, by key.
async fn pending_rows(
    fixture: &Fixture,
    expected: usize,
) -> std::collections::HashMap<String, crate::import::TriageRow> {
    fixture
        .import
        .wait_for_list(crate::import::ImportListView::default(), |snapshot| {
            snapshot.summary.counts.pending as usize == expected
        })
        .await
        .windows
        .into_iter()
        .flat_map(|window| window.items)
        .filter_map(|item| match item {
            crate::import::ImportListItem::Candidate { row, .. } => {
                Some((row.candidate_key.clone(), row))
            }
            _ => None,
        })
        .collect()
}

/// The next value a row's live-state subscription delivers.
async fn next_live_state(
    live: &mut tokio::sync::mpsc::UnboundedReceiver<crate::import::CandidateLiveState>,
) -> crate::import::CandidateLiveState {
    tokio::time::timeout(Duration::from_secs(20), live.recv())
        .await
        .expect("the row's live state moves")
        .expect("the row's live-state subscription stays open")
}

/// An import claiming a candidate moves no row: the list delivers nothing and
/// reads nothing again, and the row's own subscription is what says the import
/// owns it — and that it offers no command while it does.
#[tokio::test(flavor = "multi_thread")]
async fn a_claimed_import_reaches_its_row_and_not_the_list() {
    let fixture = Fixture::new("live-state-not-the-list").await;
    let dir = fixture.disc_id_candidate("Album Title");
    let key = dir.to_string_lossy().into_owned();
    fixture.scan(1).await;

    let request = crate::import::ImportListRequest {
        view: crate::import::ImportListView::default(),
        windows: std::iter::once(crate::library::LibraryPageWindow {
            offset: 0,
            limit: 50,
        })
        .collect(),
        upload_standing: Default::default(),
    };
    let list = crate::import::ImportListSubscription::start(
        fixture.manager.subscribe_import_list(request.clone()),
        fixture.manager.subscribe_folder_scan_progress(),
        request,
        fixture.manager.subscribe_outbox_values(),
        &tokio::runtime::Handle::current(),
    );
    let initial = list.next().await.expect("the list answers");
    let row = initial
        .windows
        .iter()
        .flat_map(|window| &window.items)
        .find_map(|item| match item {
            crate::import::ImportListItem::Candidate { row, .. } if row.candidate_key == key => {
                Some(row.clone())
            }
            _ => None,
        })
        .expect("the scanned candidate has a row");

    let mut live = fixture
        .import
        .subscribe_candidate_live_state(key.clone(), row.action_basis.clone());
    let idle = next_live_state(&mut live).await;
    assert!(!idle.facts.importing);
    assert!(!idle.actions.is_empty(), "an idle row offers its commands");

    fixture.import.claim_candidate_for_import(&key).await;

    let claimed = next_live_state(&mut live).await;
    assert!(claimed.facts.importing);
    assert!(claimed.actions.is_empty());
    assert!(
        tokio::time::timeout(Duration::from_millis(500), list.next())
            .await
            .is_err(),
        "a claimed import delivers no list value"
    );
}

/// "Identify selected" asks for each row in turn. Each row's own subscription
/// shows it waiting and then running — the list, which none of it moves, has
/// nothing to say about either.
#[tokio::test(flavor = "multi_thread")]
async fn a_batch_identify_shows_each_row_queued_then_running() {
    let fixture = Fixture::new("batch-identify-rows").await;
    let first = fixture.disc_id_candidate("First");
    let second = fixture.disc_id_candidate("Second");
    std::fs::write(second.join("notes.txt"), "distinct candidate").unwrap();
    let probed = fixture.probed_total_ms(&first);
    fixture.provider.route(
        "/discid/",
        200,
        discid_json("mb-batch", "rg-batch", &[probed, 0]),
    );
    fixture.provider.route(
        "/release/mb-batch?",
        200,
        release_json("mb-batch", "rg-batch", &[probed, 0]),
    );
    fixture.scan(2).await;
    let rows = pending_rows(&fixture, 2).await;

    let keys = [&first, &second].map(|dir| dir.to_string_lossy().into_owned());
    let mut subscriptions = Vec::new();
    for key in &keys {
        let mut live = fixture
            .import
            .subscribe_candidate_live_state(key.clone(), rows[key].action_basis.clone());
        let idle = next_live_state(&mut live).await;
        assert_eq!(idle.facts.identification, None);
        subscriptions.push(live);
    }

    for key in &keys {
        fixture.identification().rerun_identify(key.clone());
    }

    for (key, live) in keys.iter().zip(&mut subscriptions) {
        let mut seen = Vec::new();
        while !seen.contains(&crate::import::IdentificationStatus::Running) {
            if let Some(status) = next_live_state(live).await.facts.identification {
                seen.push(status);
            }
        }
        let queued = seen
            .iter()
            .position(|status| *status == crate::import::IdentificationStatus::Queued);
        let running = seen
            .iter()
            .position(|status| *status == crate::import::IdentificationStatus::Running);
        assert!(
            matches!((queued, running), (Some(queued), Some(running)) if queued < running),
            "{key} showed queued, then running: {seen:?}"
        );
    }
}
