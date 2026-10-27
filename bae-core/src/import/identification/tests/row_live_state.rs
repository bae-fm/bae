// ── What is running for a row, read with the page ──────────────────────────
//
// Where each candidate stands is written to the standings table and read with
// the rows, so a change to it delivers the page again at the same view
// revision, its rows carrying it. The filter's counts are read the same way.

/// A list subscription over the Pending tab, asking for `windows`.
async fn list_subscription(
    fixture: &Fixture,
    windows: crate::library::LibraryPageWindows,
) -> crate::import::ImportListSubscription {
    crate::import::ImportListSubscription::open(
        fixture.manager.clone(),
        crate::import::ImportListView::default(),
        windows,
    )
    .await
    .expect("the list opens")
}

fn page_window(offset: u64, limit: u64) -> crate::library::LibraryPageWindows {
    std::iter::once(crate::library::LibraryPageWindow { offset, limit }).collect()
}

/// The list's next snapshot.
async fn next_page(
    list: &crate::import::ImportListSubscription,
) -> crate::import::ImportListSnapshot {
    tokio::time::timeout(Duration::from_secs(20), list.next())
        .await
        .expect("the list delivers")
        .expect("the list answers")
}

/// How many of Found's rows a snapshot counts under `filter`.
fn counted(
    snapshot: &crate::import::ImportListSnapshot,
    filter: crate::import::PendingFilter,
) -> u32 {
    snapshot
        .pending_filter_entries
        .iter()
        .find(|entry| entry.filter == filter)
        .expect("every entry is counted")
        .count
}

/// What is running for each candidate row a snapshot's page holds, by key.
fn page_live(
    snapshot: &crate::import::ImportListSnapshot,
) -> BTreeMap<String, crate::import::CandidateLiveState> {
    snapshot
        .windows
        .iter()
        .flat_map(|window| &window.items)
        .filter_map(|item| match item {
            crate::import::ImportListItem::Candidate { row, live, .. } => {
                Some((row.candidate_key.clone(), live.clone()))
            }
            _ => None,
        })
        .collect()
}

fn preparing(import_id: &str) -> crate::import::ImportProgress {
    crate::import::ImportProgress::Preparing {
        import_id: import_id.to_string(),
        step: crate::import::PrepareStep::ValidatingSourceFiles,
    }
}

fn running(phase: crate::import::ImportPhase, percent: Option<u8>) -> crate::import::ImportProgress {
    crate::import::ImportProgress::Progress {
        id: "release-1".to_string(),
        percent,
        phase,
        import_id: "import-1".to_string(),
    }
}

fn reported(key: &str, progress: crate::import::ImportProgress) -> ImportEvent {
    ImportEvent::ImportProgress {
        candidate_key: key.to_string(),
        progress,
    }
}

/// An import claiming a candidate writes its standing, so the list is read
/// again at the same request revision, with the row saying the import owns it
/// — waiting for the worker, then taken up, then writing — and what it offers
/// at each. A progress tick within the running import is not written and
/// delivers nothing. The page the claim delivers counts the row In Progress.
#[tokio::test(flavor = "multi_thread")]
async fn a_claimed_import_reaches_its_row_on_the_page() {
    let fixture = Fixture::new("live-state-on-the-page").await;
    let dir = fixture.disc_id_candidate("Album Title");
    let key = dir.to_string_lossy().into_owned();
    fixture.scan(1).await;

    let list = list_subscription(&fixture, page_window(0, 50)).await;
    let initial = next_page(&list).await;
    let idle = page_live(&initial)
        .remove(&key)
        .expect("the scanned candidate has a row");
    assert!(!idle.facts.importing());
    assert!(!idle.actions.is_empty(), "an idle row offers its commands");
    assert_eq!(counted(&initial, crate::import::PendingFilter::All), 1);
    assert_eq!(counted(&initial, crate::import::PendingFilter::InProgress), 0);

    fixture.import.claim_candidate_for_import(&key, "import-1").await;
    let claimed = next_page(&list).await;
    assert_eq!(
        claimed.view_revision, initial.view_revision,
        "a claimed import changed the list's request"
    );
    assert_eq!(
        claimed.cause,
        coven::LiveQueryCause::Write
    );
    let queued = &page_live(&claimed)[&key];
    assert_eq!(
        queued.facts.import,
        Some(crate::import::ImportStanding::Queued)
    );
    assert_eq!(
        queued.actions,
        vec![
            crate::import::CandidateAction::CancelImport,
            crate::import::CandidateAction::RevealFolder
        ],
        "a claimed import offers only its cancel, beside showing its folder"
    );
    assert_eq!(counted(&claimed, crate::import::PendingFilter::All), 1);
    assert_eq!(
        counted(&claimed, crate::import::PendingFilter::InProgress),
        1,
        "the claimed import is counted In Progress"
    );

    // The worker takes the import up: it writes where the import stands,
    // then reports it.
    fixture
        .import
        .move_candidate_import_for_test(&key, "import-1", crate::import::ImportStanding::Running)
        .await;
    fixture
        .import
        .emit_event_for_test(reported(&key, preparing("import-1")));
    let taken_up = &page_live(&next_page(&list).await)[&key];
    assert_eq!(
        taken_up.facts.import,
        Some(crate::import::ImportStanding::Running),
        "the worker's first report takes the import off the queue"
    );
    assert_eq!(taken_up.actions, queued.actions);

    fixture.import.emit_event_for_test(reported(
        &key,
        running(crate::import::ImportPhase::MeasuringLoudness, Some(40)),
    ));
    fixture
        .import
        .move_candidate_import_for_test(&key, "import-1", crate::import::ImportStanding::Writing)
        .await;
    fixture.import.emit_event_for_test(reported(
        &key,
        running(crate::import::ImportPhase::Finalizing, None),
    ));
    let writing = next_page(&list).await;
    let writing_live = &page_live(&writing)[&key];
    assert_eq!(
        writing_live.facts.import,
        Some(crate::import::ImportStanding::Writing),
        "the progress tick before it delivered a page of its own"
    );
    assert_eq!(
        writing_live.actions,
        vec![crate::import::CandidateAction::RevealFolder],
        "an import writing its release offers no cancel it would refuse"
    );

    // A window move is one more request to the same query, and the rows it
    // reads carry where each stands.
    list.set_windows(page_window(0, 49)).unwrap();
    let moved = next_page(&list).await;
    assert_eq!(moved.view_revision, initial.view_revision);
    assert_eq!(
        moved.cause,
        coven::LiveQueryCause::Request
    );
    assert_eq!(
        moved.windows[0].items, writing.windows[0].items,
        "the window move read the rows as they were delivered"
    );
}

/// Where a candidate off the page stands reaches no row: it delivers the page
/// again only for the count it moves. Once a new window puts the candidate on
/// the page, its row carries its standing and the same subscription delivers
/// its changes.
#[tokio::test(flavor = "multi_thread")]
async fn a_row_off_the_page_reaches_its_row_once_the_window_holds_it() {
    let fixture = Fixture::new("live-state-off-the-page").await;
    let first = fixture.disc_id_candidate("First");
    let second = fixture.disc_id_candidate("Second");
    std::fs::write(second.join("notes.txt"), "distinct candidate").unwrap();
    fixture.scan(2).await;

    let list = list_subscription(&fixture, page_window(0, 1)).await;
    let initial = next_page(&list).await;
    let on_page = page_live(&initial);
    assert_eq!(on_page.len(), 1, "the window holds one row");
    let on_key = on_page.into_keys().next().expect("one row");
    let off_key = [first, second]
        .into_iter()
        .map(|dir| dir.to_string_lossy().into_owned())
        .find(|key| *key != on_key)
        .expect("the other candidate is off the page");

    fixture
        .import
        .claim_candidate_for_import(&off_key, "import-off")
        .await;
    let counted_off = next_page(&list).await;
    assert_eq!(counted_off.view_revision, initial.view_revision);
    assert_eq!(
        counted_off.windows, initial.windows,
        "the claim off the page reached no row on it"
    );
    assert_eq!(
        counted(&counted_off, crate::import::PendingFilter::InProgress),
        1
    );

    fixture
        .import
        .claim_candidate_for_import(&on_key, "import-on")
        .await;
    let claimed = next_page(&list).await;
    assert_eq!(claimed.view_revision, initial.view_revision);
    assert_eq!(
        page_live(&claimed)[&on_key].facts.import,
        Some(crate::import::ImportStanding::Queued)
    );
    assert_eq!(counted(&claimed, crate::import::PendingFilter::InProgress), 2);

    list.set_windows(page_window(0, 2)).unwrap();
    let widened = next_page(&list).await;
    assert_eq!(
        widened.view_revision,
        initial.view_revision,
        "a window is the query's request, not the list's view"
    );
    assert_eq!(
        widened.cause,
        coven::LiveQueryCause::Request
    );
    assert_eq!(
        page_live(&widened)[&off_key].facts.import,
        Some(crate::import::ImportStanding::Queued),
        "the row the window now holds carries where it stands"
    );

    fixture
        .import
        .move_candidate_import_for_test(
            &off_key,
            "import-off",
            crate::import::ImportStanding::Running,
        )
        .await;
    fixture
        .import
        .emit_event_for_test(reported(&off_key, preparing("import-off")));
    let taken_up = next_page(&list).await;
    assert_eq!(taken_up.view_revision, widened.view_revision);
    let live = page_live(&taken_up);
    assert_eq!(
        live[&off_key].facts.import,
        Some(crate::import::ImportStanding::Running)
    );
    assert_eq!(
        live[&on_key].facts.import,
        Some(crate::import::ImportStanding::Queued)
    );
}

/// "Identify selected" asks for each row in turn. The page shows each row
/// waiting and then running, at the list's one request revision.
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
    // Identified only when asked: nothing found here is identified on its own.
    fixture
        .manager
        .set_identify_automatically(false)
        .await
        .unwrap();
    fixture.scan(2).await;

    let list = list_subscription(&fixture, page_window(0, 50)).await;
    let initial = next_page(&list).await;
    let keys = [&first, &second].map(|dir| dir.to_string_lossy().into_owned());
    for key in &keys {
        assert_eq!(page_live(&initial)[key].facts.identification, None);
    }

    for key in &keys {
        fixture.identification().rerun_identify(key.clone());
    }

    let mut seen: BTreeMap<String, Vec<crate::import::IdentificationStatus>> = BTreeMap::new();
    let ran = |seen: &BTreeMap<String, Vec<crate::import::IdentificationStatus>>| {
        keys.iter().all(|key| {
            seen.get(key).is_some_and(|statuses| {
                statuses.contains(&crate::import::IdentificationStatus::Running)
            })
        })
    };
    while !ran(&seen) {
        let snapshot = next_page(&list).await;
        assert_eq!(
            snapshot.view_revision, initial.view_revision,
            "identifying changed the list's request"
        );
        for (key, live) in page_live(&snapshot) {
            let statuses = seen.entry(key).or_default();
            if let Some(status) = live.facts.identification {
                if statuses.last() != Some(&status) {
                    statuses.push(status);
                }
            }
        }
    }
    for key in &keys {
        let statuses = &seen[key];
        let queued = statuses
            .iter()
            .position(|status| *status == crate::import::IdentificationStatus::Queued);
        let running = statuses
            .iter()
            .position(|status| *status == crate::import::IdentificationStatus::Running);
        assert!(
            matches!((queued, running), (Some(queued), Some(running)) if queued < running),
            "{key} showed queued, then running: {statuses:?}"
        );
    }
}

/// A verdict stored for a row off the page reaches no row on it, but moves
/// the filter's counts, so the list delivers the page again with them.
#[tokio::test(flavor = "multi_thread")]
async fn a_verdict_off_the_page_delivers_the_counts_it_moves() {
    let fixture = Fixture::new("verdict-off-the-page").await;
    let first = fixture.disc_id_candidate("First");
    let second = fixture.disc_id_candidate("Second");
    std::fs::write(second.join("notes.txt"), "distinct candidate").unwrap();
    fixture
        .manager
        .set_identify_automatically(false)
        .await
        .unwrap();
    fixture.scan(2).await;

    let list = list_subscription(&fixture, page_window(0, 1)).await;
    let initial = next_page(&list).await;
    let on_key = page_live(&initial)
        .into_keys()
        .next()
        .expect("the window holds one row");
    let off_dir = [&first, &second]
        .into_iter()
        .find(|dir| dir.to_string_lossy() != on_key)
        .expect("the other candidate is off the page")
        .clone();

    let probed = fixture.probed_total_ms(&off_dir);
    fixture
        .archive("mb-off-page", "rg-off-page", &[probed, 0])
        .await;
    fixture
        .store_settled_verdict(&off_dir, "mb-off-page", "rg-off-page")
        .await;
    let moved = next_page(&list).await;
    assert_ne!(
        moved.pending_filter_entries, initial.pending_filter_entries,
        "the verdict moved a count"
    );
    assert_eq!(counted(&moved, crate::import::PendingFilter::All), 2);
    assert_eq!(
        moved.windows, initial.windows,
        "the verdict off the page reached no row on it"
    );
    assert_eq!(page_live(&moved).into_keys().collect::<Vec<_>>(), vec![on_key]);
}
