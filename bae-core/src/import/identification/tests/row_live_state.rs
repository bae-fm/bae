// ── What is running for a row, joined to the page ───────────────────────────
//
// Under Found's All the list reads the tables and nothing else; what is
// running for each candidate is joined to its row on the page in memory, and a
// change to it for a row on the page delivers the page again at the same
// request revision. The filter menu counts what is running when it opens, so
// a run or an import starting or ending reads nothing again for it either.

/// A list subscription over the Pending tab, asking for `windows`.
fn list_subscription(
    fixture: &Fixture,
    windows: crate::library::LibraryPageWindows,
) -> crate::import::ImportListSubscription {
    let request = crate::import::ImportListRequest {
        view: crate::import::ImportListView::default(),
        windows,
        upload_standing: Default::default(),
        live_standings: Default::default(),
    };
    crate::import::ImportListSubscription::start(
        fixture.manager.subscribe_import_list(request.clone()),
        fixture.manager.subscribe_folder_scan_progress(),
        request,
        fixture.manager.subscribe_outbox_values(),
        fixture.import.watch_runtime_facts(),
        &tokio::runtime::Handle::current(),
    )
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

/// How many of Found's rows the filter menu, opened now, counts under
/// `filter`.
fn counted(
    list: &crate::import::ImportListSubscription,
    filter: crate::import::PendingFilter,
) -> u32 {
    list.pending_filter_entries()
        .into_iter()
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
        album_title: String::new(),
        artist_name: String::new(),
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

/// An import claiming a candidate moves no row, so the list reads nothing
/// again: the page it already read is delivered again, at the same request
/// revision, with the row saying the import owns it — waiting for the worker,
/// then taken up, then writing — and what it offers at each. A progress tick
/// within the running import changes neither and delivers nothing. The filter
/// menu, opened while the import goes, counts the row In Progress.
#[tokio::test(flavor = "multi_thread")]
async fn a_claimed_import_reaches_its_row_on_the_page() {
    let fixture = Fixture::new("live-state-on-the-page").await;
    let dir = fixture.disc_id_candidate("Album Title");
    let key = dir.to_string_lossy().into_owned();
    fixture.scan(1).await;

    let list = list_subscription(&fixture, page_window(0, 50));
    let initial = next_page(&list).await;
    let idle = page_live(&initial)
        .remove(&key)
        .expect("the scanned candidate has a row");
    assert!(!idle.facts.importing());
    assert!(!idle.actions.is_empty(), "an idle row offers its commands");
    assert_eq!(counted(&list, crate::import::PendingFilter::All), 1);
    assert_eq!(counted(&list, crate::import::PendingFilter::InProgress), 0);

    fixture.import.claim_candidate_for_import(&key, "import-1").await;
    let claimed = next_page(&list).await;
    assert_eq!(
        claimed.request_revision, initial.request_revision,
        "a claimed import read the list again"
    );
    assert_eq!(
        claimed.cause,
        coven::ReconfigurableLiveQueryCause::DatabaseChanged
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
    assert_eq!(counted(&list, crate::import::PendingFilter::All), 1);
    assert_eq!(
        counted(&list, crate::import::PendingFilter::InProgress),
        1,
        "the menu counts the claimed import with no read of the list"
    );

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
    // reads are joined with what is running as it stands.
    list.set_windows(page_window(0, 49)).unwrap();
    let moved = next_page(&list).await;
    assert_eq!(moved.request_revision, initial.request_revision + 1);
    assert_eq!(
        moved.cause,
        coven::ReconfigurableLiveQueryCause::RequestChanged
    );
    assert_eq!(
        moved.windows[0].items, writing.windows[0].items,
        "the window move read the rows as they were delivered"
    );
}

/// What is running for a candidate off the page delivers nothing; once a new
/// window puts it on the page, the same subscription joins it and delivers its
/// changes.
#[tokio::test(flavor = "multi_thread")]
async fn a_row_off_the_page_delivers_nothing_until_the_window_holds_it() {
    let fixture = Fixture::new("live-state-off-the-page").await;
    let first = fixture.disc_id_candidate("First");
    let second = fixture.disc_id_candidate("Second");
    std::fs::write(second.join("notes.txt"), "distinct candidate").unwrap();
    fixture.scan(2).await;

    let list = list_subscription(&fixture, page_window(0, 1));
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
    fixture
        .import
        .claim_candidate_for_import(&on_key, "import-on")
        .await;
    let claimed = next_page(&list).await;
    assert_eq!(claimed.request_revision, initial.request_revision);
    assert_eq!(
        page_live(&claimed)[&on_key].facts.import,
        Some(crate::import::ImportStanding::Queued),
        "the claim off the page delivered a page of its own"
    );

    list.set_windows(page_window(0, 2)).unwrap();
    let widened = next_page(&list).await;
    assert_eq!(
        widened.request_revision,
        initial.request_revision + 1,
        "one more request to the same query"
    );
    assert_eq!(
        widened.cause,
        coven::ReconfigurableLiveQueryCause::RequestChanged
    );
    assert_eq!(
        page_live(&widened)[&off_key].facts.import,
        Some(crate::import::ImportStanding::Queued),
        "the row the window now holds is joined with what is running for it"
    );

    fixture
        .import
        .emit_event_for_test(reported(&off_key, preparing("import-off")));
    let taken_up = next_page(&list).await;
    assert_eq!(taken_up.request_revision, widened.request_revision);
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
/// waiting and then running — the list, which none of it moves, reads nothing
/// again for either.
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

    let list = list_subscription(&fixture, page_window(0, 50));
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
            snapshot.request_revision, initial.request_revision,
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

/// A verdict stored for a row off the page moves the filter menu's counts and
/// delivers nothing: the next page the list delivers is the one a change on
/// the page brings.
#[tokio::test(flavor = "multi_thread")]
async fn a_verdict_off_the_page_moves_the_counts_and_delivers_nothing() {
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

    let list = list_subscription(&fixture, page_window(0, 1));
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
    let before = list.pending_filter_entries();

    let probed = fixture.probed_total_ms(&off_dir);
    fixture
        .archive("mb-off-page", "rg-off-page", &[probed, 0])
        .await;
    // The list reads while a caller waits on its next page, as the app always
    // does; the verdict and the claim land while this one waits.
    let (next, ()) = tokio::join!(next_page(&list), async {
        fixture
            .store_settled_verdict(&off_dir, "mb-off-page", "rg-off-page")
            .await;
        let deadline = tokio::time::Instant::now() + Duration::from_secs(20);
        while list.pending_filter_entries() == before {
            assert!(
                tokio::time::Instant::now() < deadline,
                "the verdict never reached the menu's counts"
            );
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        fixture
            .import
            .claim_candidate_for_import(&on_key, "import-on")
            .await;
    });
    assert_eq!(
        page_live(&next)[&on_key].facts.import,
        Some(crate::import::ImportStanding::Queued),
        "the verdict off the page delivered a page of its own"
    );
}
