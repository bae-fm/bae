/// The whole point of the task: nothing is selected, no view is open, and the
/// candidate still ends up with a stored verdict that classifies as Ready.
///
/// The provider answers the disc-ID lookup with exactly one release whose track
/// lengths are the fixture audio's own, so the Ready rule's every clause is
/// exercised for real: one match, not in the library, counts agreeing, totals
/// agreeing.
#[tokio::test(flavor = "multi_thread")]
#[serial(musicbrainz)]
async fn a_candidate_nobody_selected_acquires_a_verdict() {
    let fixture = Fixture::new("acquires-verdict").await;
    let dir = fixture.disc_id_candidate("Album");
    let probed = fixture.probed_total_ms(&dir);
    fixture.provider.route(
        "/discid/",
        200,
        discid_json(
            "mb-ready-1",
            "rg-ready-1",
            &[probed / 2, probed - probed / 2],
        ),
    );
    fixture.provider.route(
        "/release/mb-ready-1?",
        200,
        release_json(
            "mb-ready-1",
            "rg-ready-1",
            &[probed / 2, probed - probed / 2],
        ),
    );
    fixture.scan(1).await;

    // Nobody selects anything. The sweep is the only actor.
    fixture.sweep_once().await;

    let row = fixture.stored_for(&dir).await.expect("a verdict is stored");
    assert_eq!(
        row.folder_path,
        dir.to_string_lossy(),
        "the row names where the candidate was last seen"
    );
    let identify = identify_result(&row);
    assert_eq!(
        identify.probed_total_duration_ms as u64, probed,
        "the probed total rode the fast pass into the row"
    );
    assert_eq!(
        identify.identified_at,
        fixed_now(),
        "the row is stamped from the injected clock"
    );
    assert_eq!(
        fixture.classification_for(&dir).await,
        QueueClassification::Ready,
        "one match, not in the library, counts and totals agreeing"
    );
}

#[tokio::test(flavor = "multi_thread")]
#[serial(musicbrainz)]
async fn a_planned_candidate_is_queued_before_its_driver_reports() {
    let fixture = Fixture::new("queued-before-driver").await;
    let dir = fixture.disc_id_candidate("Candidate");
    let key = dir.to_string_lossy().into_owned();
    fixture.provider.route("/discid/", 200, "{}");
    fixture.provider.hold("/discid/");
    fixture.scan(1).await;
    let mut changes = fixture.import.subscribe_candidate_runtime().1;

    let context = fixture.context();
    let token = CancellationToken::new();
    let pass = tokio::spawn(async move { run_pass_for_test(&context, &token).await });

    let change = tokio::time::timeout(Duration::from_secs(10), changes.recv())
        .await
        .expect("the queue state is published before identification")
        .expect("candidate runtime remains open");
    let crate::import::CandidateRuntimeChange::Reset { runtimes } = change else {
        panic!("the pass admits its queue atomically before starting drivers");
    };
    assert_eq!(
        crate::import::TriageRuntimeFacts::of(&runtimes[&key]).identify_phase,
        Some(crate::import::IdentifyPhase::Queued)
    );

    fixture.provider.release();
    tokio::time::timeout(Duration::from_secs(20), pass)
        .await
        .expect("the pass finishes after the provider resumes")
        .unwrap();
}

// ── 2. A stored verdict is not re-fetched ───────────────────────────────────

/// The second launch is instant because a candidate whose content hash already
/// has a verdict is never handed to the pipeline again. Two passes over the same
/// queue, and the provider sees requests only in the first.
#[tokio::test(flavor = "multi_thread")]
#[serial(musicbrainz)]
async fn a_stored_verdict_is_not_re_fetched() {
    let fixture = Fixture::new("not-re-fetched").await;
    let dir = fixture.disc_id_candidate("Album");
    let probed = fixture.probed_total_ms(&dir);
    fixture.provider.route(
        "/discid/",
        200,
        discid_json("mb-cached-1", "rg-cached-1", &[probed, 0]),
    );
    fixture.provider.route(
        "/release/mb-cached-1?",
        200,
        release_json("mb-cached-1", "rg-cached-1", &[probed, 0]),
    );
    fixture.scan(1).await;

    fixture.sweep_once().await;
    let after_first = fixture.provider.requests().len();
    assert!(
        after_first > 0,
        "the first pass has to actually ask the provider"
    );
    assert!(fixture.identified_for(&dir).await.is_some());

    fixture.sweep_once().await;

    assert_eq!(
        fixture.provider.requests().len(),
        after_first,
        "the second pass asked the provider for nothing: {:?}",
        fixture.provider.requests()
    );
}

// ── 3. A transport failure is stored until an explicit rerun ────────────────

/// The failing response is a 400 rather than a 5xx so the client's own retry
/// policy stays out of it; what is under test is what the sweep does with a
/// failure, not how many times the client repeats one.
/// A settled identify driver stays alive for the toolbar and re-broadcasts
/// its terminal state whenever a late signals snapshot reaches it -- the
/// extraction can still be running when the sweep pass that watched the run
/// has already returned. The next pass starts a fresh run for the same
/// candidate; that stale re-broadcast is not the explicit re-run's answer, and
/// the re-run must keep waiting for its own.
#[tokio::test(flavor = "multi_thread")]
#[serial(musicbrainz)]
async fn an_explicit_rerun_ignores_the_previous_run_s_terminal_state() {
    let fixture = Fixture::new("retry-ignores-stale").await;
    let dir = fixture.disc_id_candidate("Album");
    let key = dir.to_string_lossy().into_owned();
    let probed = fixture.probed_total_ms(&dir);
    fixture
        .provider
        .set_routes(vec![("/discid/", 400, "{}".to_string())]);
    fixture.scan(1).await;

    let mut events = fixture.import.subscribe_events();
    fixture.sweep_once().await;
    assert!(matches!(
        fixture.identified_for(&dir).await.map(|row| row.verdict),
        Some(TerminalVerdict::Failed { .. })
    ));
    // The failed run's own terminal event, exactly as the lingering driver
    // would broadcast it again.
    let stale = drain_events(&mut events)
        .into_iter()
        .rev()
        .find(|event| {
            matches!(
                event,
                ImportEvent::IdentifyStateChanged { candidate_key, state, .. }
                    if candidate_key == &key && state.is_terminal()
            )
        })
        .expect("the failed run settled on a terminal state");

    fixture.provider.set_routes(vec![
        (
            "/discid/",
            200,
            discid_json("mb-retry-2", "rg-retry-2", &[probed, 0]),
        ),
        (
            "/release/mb-retry-2?",
            200,
            release_json("mb-retry-2", "rg-retry-2", &[probed, 0]),
        ),
    ]);
    fixture.provider.hold("/discid/");
    fixture.sweep.rerun_for_explicit_lookup(key.clone());
    // The explicit lookup is in flight: its run owns the candidate now.
    wait_for_request(&fixture.provider, "/discid/", 2).await;
    fixture.import.emit_event_for_test(stale);
    fixture.provider.release();
    tokio::time::timeout(Duration::from_secs(15), async {
        loop {
            if matches!(
                events.recv().await,
                Ok(ImportEvent::Scan(ScanEvent::CandidateVerdictStored { candidate_key }))
                    if candidate_key == key
            ) {
                break;
            }
        }
    })
    .await
    .expect("the explicit re-run stores its verdict");

    assert!(
        matches!(
            fixture.identified_for(&dir).await.map(|row| row.verdict),
            Some(TerminalVerdict::Found { .. })
        ),
        "the explicit re-run stores its own answer, not the previous run's"
    );
}

#[tokio::test(flavor = "multi_thread")]
#[serial(musicbrainz)]
async fn a_transport_failure_is_stored_and_not_automatically_retried() {
    let fixture = Fixture::new("failure-stored").await;
    let dir = fixture.disc_id_candidate("Album");
    let probed = fixture.probed_total_ms(&dir);
    fixture
        .provider
        .set_routes(vec![("/discid/", 400, "{}".to_string())]);
    fixture.scan(1).await;

    fixture.sweep_once().await;
    let stored = fixture
        .identified_for(&dir)
        .await
        .expect("the failed outcome is stored");
    assert!(matches!(stored.verdict, TerminalVerdict::Failed { .. }));
    let requests_after_failure = fixture.provider.requests().len();

    fixture.provider.set_routes(vec![
        (
            "/discid/",
            200,
            discid_json("mb-retry-1", "rg-retry-1", &[probed, 0]),
        ),
        (
            "/release/mb-retry-1?",
            200,
            release_json("mb-retry-1", "rg-retry-1", &[probed, 0]),
        ),
    ]);
    fixture.sweep_once().await;

    assert_eq!(
        fixture.provider.requests().len(),
        requests_after_failure,
        "a stored failure waits for an explicit rerun"
    );
}

// ── 4. The interactive path is not delayed by the sweep ─────────────────────

/// The pair to the limiter's own priority test, from the producer's side. With
/// the sweep's background lookups queued on the shared limiter, a search the
/// user typed is admitted next rather than after all of them.
///
/// Eight candidates saturate the limiter's background queue at the sweep's
/// concurrency cap. Without priority the interactive search waits out every
/// queued background call at one second each; with it, one interval.
///
/// Wall time, not the deterministic clock, and deliberately: the fake provider
/// is a real socket, so `start_paused` would leave the runtime idle while a
/// response is in flight and auto-advance straight into the request's own
/// `API_TIMEOUT` — every lookup would time out before the server answered. What
/// the clock would otherwise buy is bought instead by bracketing the
/// measurement with assertions that background work really was in flight, so a
/// sweep that had died cannot make this pass by doing nothing.
#[tokio::test(flavor = "multi_thread")]
#[serial(musicbrainz)]
async fn the_interactive_path_is_not_delayed_by_the_sweep() {
    let fixture = Fixture::new("interactive-not-delayed").await;
    let mut dirs = Vec::new();
    for i in 0..8 {
        let dir = fixture.disc_id_candidate(&format!("Album {i}"));
        std::fs::write(
            dir.join(format!("playlist-{i}.m3u")),
            format!("candidate {i}"),
        )
        .unwrap();
        dirs.push(dir);
    }
    let probed = fixture.probed_total_ms(&dirs[0]);
    fixture.provider.route(
        "/discid/",
        200,
        discid_json("mb-flood-0", "rg-flood-0", &[probed, 0]),
    );
    fixture.provider.route(
        "/release/mb-flood-0?",
        200,
        release_json("mb-flood-0", "rg-flood-0", &[probed, 0]),
    );
    fixture
        .provider
        .route("/release?", 200, search_json("mb-typed", "rg-typed"));
    fixture.scan(8).await;

    let context = fixture.context();
    let token = CancellationToken::new();
    let sweep_token = token.clone();
    let sweep = tokio::spawn(async move { run_pass_for_test(&context, &sweep_token).await });

    // Let the sweep take the first slot and stack the rest behind it.
    tokio::time::sleep(Duration::from_millis(1_200)).await;
    let background_before = fixture.provider.count_containing("/discid/");
    assert!(
        (1..8).contains(&background_before),
        "the sweep must be mid-flight when the search is timed — {background_before} of 8 \
         lookups done means there is no background queue to be admitted ahead of"
    );

    let started = std::time::Instant::now();
    let typed = crate::import::search::search_mb(
        crate::musicbrainz::ReleaseSearchParams {
            artist: Some("Artist".to_string()),
            album: Some("Album".to_string()),
            ..Default::default()
        },
        CallPriority::Interactive,
    )
    .await
    .expect("the typed search succeeds");
    let waited = started.elapsed();

    // Still running, so the search really was admitted past a live background
    // queue rather than into an idle limiter. (Its count does not rise across
    // the measurement, and must not: the whole point is that the interactive
    // call took the slot the sweep would have had.)
    assert!(
        !sweep.is_finished(),
        "the sweep must still be mid-pass across the measurement — a sweep that \
         died would make this pass by doing nothing"
    );
    token.cancel();
    let _ = tokio::time::timeout(Duration::from_secs(20), sweep).await;

    assert_eq!(typed.len(), 1);
    assert!(
        waited < Duration::from_millis(2_000),
        "an interactive search waited {waited:?} behind the sweep; \
         with priority it is admitted within about one interval"
    );
}

// ── 5. Totals decide, not per-track lengths ─────────────────────────────────

fn found_verdict(track_count: u32, source: Option<SourceTracks>) -> TerminalVerdict {
    TerminalVerdict::Found {
        matches: vec![MetadataResult {
            source: crate::import::MetadataSource::MusicBrainz,
            release_id: "mb-1".to_string(),
            title: "Album".to_string(),
            artist: None,
            year: None,
            format: None,
            label: None,
            catalog_number: None,
            country: None,
            cover_art: None,
            source_group_id: Some("rg-1".to_string()),
            source_tracks: source,
        }],
        track_count,
        provenance: vec![crate::identify::ResultProvenance {
            by_disc_id: true,
            by_barcode: false,
            by_catalog: false,
        }],
        matched_barcode: None,
    }
}

/// The gate is total against total. A rip that splits a continuous piece
/// differently from the source has per-track lengths that disagree everywhere
/// and a total that agrees exactly — and it is a correct match, so it is Ready.
/// A release that is genuinely a different edition differs in the total, and is
/// not.
///
/// The per-track half is enforced by the type, not by the comparison: what the
/// source contributes is one summed total, parsed out of its response by
/// `mb_source_tracks`, so there are no per-track lengths for a future gate to
/// reach for. This drives that parse rather than hand-building the total.
#[test]
fn totals_decide_not_per_track_lengths() {
    use crate::musicbrainz::MbReleaseResponse;

    let source_response: MbReleaseResponse =
        serde_json::from_str(&release_json("mb-1", "rg-1", &[200_000, 100_000, 300_000])).unwrap();
    let source = crate::import::search::mb_source_tracks(&source_response);
    assert_eq!(
        source,
        SourceTracks::Listed {
            count: 3,
            total_duration_ms: Some(600_000)
        }
    );

    // The rip splits the same 600 s across three tracks differently. Every
    // per-track length disagrees; the total does not.
    let rip_total = 100_000 + 300_000 + 200_000;
    assert_eq!(
        classify(&found_verdict(3, Some(source.clone())), rip_total, &[]),
        QueueClassification::Ready,
        "a different split of the same running time is the same record"
    );

    // A different edition — one track longer by a minute — is not absorbed.
    let different_edition = rip_total + 60_000;
    let QueueClassification::NeedsYou(NeedsYou::DurationsDisagree { tolerance_ms, .. }) = classify(
        &found_verdict(3, Some(source.clone())),
        different_edition,
        &[],
    ) else {
        panic!("a minute of difference must not be admitted");
    };

    // The tolerance's own edges, so a change to it fails here rather than
    // silently widening what gets imported unattended.
    assert_eq!(tolerance_ms, 5_000, "3 tracks sit on the floor");
    assert_eq!(
        classify(
            &found_verdict(3, Some(source.clone())),
            600_000 + tolerance_ms,
            &[]
        ),
        QueueClassification::Ready,
        "exactly at the tolerance still agrees"
    );
    assert!(
        matches!(
            classify(
                &found_verdict(3, Some(source)),
                600_000 + tolerance_ms + 1,
                &[]
            ),
            QueueClassification::NeedsYou(NeedsYou::DurationsDisagree { .. })
        ),
        "one millisecond past it does not"
    );
}

/// The count is checked before the totals, and separately: two different
/// tracklists can add up to the same running time.
#[test]
fn a_count_disagreement_is_named_as_one() {
    let source = SourceTracks::Listed {
        count: 12,
        total_duration_ms: Some(600_000),
    };
    assert_eq!(
        classify(&found_verdict(11, Some(source)), 600_000, &[]),
        QueueClassification::NeedsYou(NeedsYou::TrackCountDisagrees {
            local: 11,
            source: 12
        })
    );
}

// ── 6. Settling a lead ──────────────────────────────────────────────────────

/// One release lookup per settled lead, whichever signal found it.
///
/// The disc-ID response already carries a tracklist, but not the rest of what
/// opening the candidate needs — the release-level relations the commit maps,
/// the release group, the cover options. A lead is settled by fetching the
/// release itself, once, and both candidates in this pass cost exactly that.
#[tokio::test(flavor = "multi_thread")]
#[serial(musicbrainz)]
async fn settling_a_lead_costs_one_release_lookup_whichever_signal_found_it() {
    let fixture = Fixture::new("settle-lead").await;
    fixture
        .extraction
        .register_analyzer(Arc::new(BarcodeAnalyzer {
            barcode: "0123456789012".to_string(),
        }));
    let disc_dir = fixture.disc_id_candidate("From Disc Id");
    let barcode_dir = fixture.barcode_candidate("From Barcode");
    let probed = fixture.probed_total_ms(&disc_dir);

    for (id, group) in [("mb-disc-1", "rg-disc-1"), ("mb-barcode-1", "rg-barcode-1")] {
        fixture.provider.route(
            &format!("/release/{id}?"),
            200,
            release_json(id, group, &[probed, 0]),
        );
    }
    fixture.provider.route(
        "/discid/",
        200,
        discid_json("mb-disc-1", "rg-disc-1", &[probed, 0]),
    );
    fixture.provider.route(
        "/release?",
        200,
        search_json("mb-barcode-1", "rg-barcode-1"),
    );
    fixture.scan(2).await;

    fixture.sweep_once().await;

    assert_eq!(
        fixture.count_release_lookups("mb-disc-1"),
        1,
        "the disc-ID lead is settled once: {:?}",
        fixture.provider.requests()
    );
    assert_eq!(
        fixture.count_release_lookups("mb-barcode-1"),
        1,
        "and so is the search lead: {:?}",
        fixture.provider.requests()
    );
    assert_eq!(
        fixture.classification_for(&disc_dir).await,
        QueueClassification::Ready
    );
    assert_eq!(
        fixture.classification_for(&barcode_dir).await,
        QueueClassification::Ready
    );
}

/// The write ordering, from the outside: a failed lead fetch stores the failure
/// without storing partial release documents, and automatic passes leave it
/// alone until an explicit re-run.
#[tokio::test(flavor = "multi_thread")]
#[serial(musicbrainz)]
async fn a_failed_settle_is_stored_without_partial_documents() {
    let fixture = Fixture::new("settle-ordering").await;
    let dir = fixture.disc_id_candidate("Album");
    let probed = fixture.probed_total_ms(&dir);
    // The disc-ID lookup answers; the release lookup that settles the lead does
    // not. The failed terminal answer must be stored without pretending the
    // release itself was settled.
    fixture.provider.route(
        "/discid/",
        200,
        discid_json("mb-order-1", "rg-order-1", &[probed, 0]),
    );
    fixture.provider.route("/release/mb-order-1?", 400, "{}");
    fixture.scan(1).await;

    fixture.sweep_once().await;

    assert!(matches!(
        fixture.identified_for(&dir).await.map(|row| row.verdict),
        Some(TerminalVerdict::Failed { .. })
    ));
    assert!(
        fixture.archived("mb-order-1").await.is_none(),
        "and nothing half-written is left behind"
    );

    let requests_after_failure = fixture.provider.requests().len();
    // The provider comes back, but an automatic pass does not replace the
    // stored failure.
    fixture.provider.set_routes(vec![
        (
            "/discid/",
            200,
            discid_json("mb-order-1", "rg-order-1", &[probed, 0]),
        ),
        (
            "/release/mb-order-1?",
            200,
            release_json("mb-order-1", "rg-order-1", &[probed, 0]),
        ),
    ]);
    fixture.sweep_once().await;

    assert_eq!(
        fixture.provider.requests().len(),
        requests_after_failure,
        "the stored failure is not retried automatically"
    );
    assert!(
        fixture.archived("mb-order-1").await.is_none(),
        "an automatic pass leaves the failed settle untouched"
    );
}

/// Explicit Lookup settles a candidate too. A person's own run answers the
/// candidate for good, and "answered" means the next launch opens it with no
/// network — so the same step runs here, before the verdict is written.
#[tokio::test(flavor = "multi_thread")]
#[serial(musicbrainz)]
async fn explicit_lookup_settles_its_lead_before_storing_the_verdict() {
    let fixture = Fixture::new("interactive-settles").await;
    fixture
        .extraction
        .register_analyzer(Arc::new(BarcodeAnalyzer {
            barcode: "0123456789012".to_string(),
        }));
    let dir = fixture.barcode_candidate("From Barcode");
    let probed = fixture.probed_total_ms(&dir);
    fixture.provider.route(
        "/release?",
        200,
        search_json("mb-interactive-1", "rg-interactive-1"),
    );
    fixture.provider.route(
        "/release/mb-interactive-1?",
        200,
        release_json("mb-interactive-1", "rg-interactive-1", &[probed, 0]),
    );
    fixture.scan(1).await;

    // Exactly what the explicit Lookup action does.
    fixture.start_explicit_lookup(&dir);
    let row = tokio::time::timeout(
        Duration::from_secs(20),
        fixture.await_identified_row(&dir),
    )
        .await
        .expect("the explicit Lookup recorder stores the verdict");

    let verdict = identify_result(&row).verdict.clone();
    let TerminalVerdict::Found { matches, .. } = &verdict else {
        panic!("expected a single-match Found, got {verdict:?}");
    };
    assert!(
        matches[0].source_tracks.is_some(),
        "the lead was settled before the verdict was written"
    );
    assert!(
        fixture.archived("mb-interactive-1").await.is_some(),
        "and its documents are archived under the release they describe"
    );
    assert_eq!(
        fixture.classification_for(&dir).await,
        QueueClassification::Ready,
        "so the row is admitted on evidence that was actually checked"
    );

    // A later sweep pass finds nothing left to buy.
    let after_lookup = fixture.provider.requests().len();
    fixture.sweep_once().await;
    assert_eq!(
        fixture.provider.requests().len(),
        after_lookup,
        "a settled row is finished: {:?}",
        fixture.provider.requests()
    );
}

/// A release document can carry a readable tracklist yet still be impossible
/// to project into candidate metadata. An explicit run stores that terminal
/// release-details failure instead of waiting for another state event that
/// will never arrive.
#[tokio::test(flavor = "multi_thread")]
#[serial(musicbrainz)]
async fn explicit_lookup_stores_a_metadata_projection_failure() {
    let fixture = Fixture::new("interactive-projection-failure").await;
    let dir = fixture.disc_id_candidate("Album");
    let probed = fixture.probed_total_ms(&dir);
    fixture.provider.route(
        "/discid/",
        200,
        discid_json("mb-projection-1", "rg-projection-1", &[probed, 0]),
    );
    let mut incomplete: serde_json::Value = serde_json::from_str(&release_json(
        "mb-projection-1",
        "rg-projection-1",
        &[probed, 0],
    ))
    .unwrap();
    incomplete
        .as_object_mut()
        .unwrap()
        .remove("release-group");
    fixture.provider.route(
        "/release/mb-projection-1?",
        200,
        incomplete.to_string(),
    );
    fixture.scan(1).await;

    fixture.start_explicit_lookup(&dir);
    let row = tokio::time::timeout(Duration::from_secs(2), fixture.await_identified_row(&dir))
        .await
        .expect("the explicit recorder stores the projection failure");

    assert!(matches!(
        identify_result(&row).verdict,
        TerminalVerdict::Failed {
            ref failures,
            ..
        } if matches!(failures.as_slice(), [crate::identify::IdentifyFailure::ReleaseDetails(_)])
    ));
}

/// The receipt for "so ready it is offline": picking a candidate whose lead is
/// settled reaches the wire for nothing and its pane draws whole — the
/// hermetic client would panic on a live lookup, and the provider would answer
/// 404. The release id is this test's own, so no session cache holds it either.
#[tokio::test(flavor = "multi_thread")]
#[serial(musicbrainz)]
async fn a_settled_candidate_opens_with_the_provider_gone() {
    let fixture = Fixture::new("offline-open").await;
    let dir = fixture.disc_id_candidate("Album");
    let probed = fixture.probed_total_ms(&dir);
    fixture.scan(1).await;

    // Nothing is routed and nothing is seeded: the archived documents are the
    // only place this release exists.
    fixture
        .archive("mb-offline-1", "rg-offline-1", &[probed, 0])
        .await;
    fixture
        .store_settled_verdict(&dir, "mb-offline-1", "rg-offline-1", probed)
        .await;
    let before = fixture.provider.requests().len();

    fixture
        .import
        .select_candidate_metadata_provenance(
            dir.to_string_lossy().into_owned(),
            crate::import::MetadataProvenance::ExternalRelease {
                source: crate::import::MetadataSource::MusicBrainz,
                release_id: "mb-offline-1".to_string(),
            },
        )
        .await
        .expect("a settled candidate opens from what identification archived");

    let detail = fixture
        .pane(&dir)
        .await
        .expect("the picked candidate reads back");
    let release = detail.release.expect("the pick names a release");
    assert_eq!(release.release_id, "mb-offline-1");
    assert_eq!(release.tracks.len(), 2);
    let edit = detail.metadata_draft;
    assert_eq!(edit.album_title, "Album");
    assert_eq!(
        release
            .cover_art
            .iter()
            .map(|cover| cover.url.as_str())
            .collect::<Vec<_>>(),
        vec![format!(
            "{}/release-group/rg-offline-1/front",
            crate::import::cover_art::archive_base_for_test()
        )],
        "the pressing states no front image of its own, so the album's is the \
         only option — and it is read off the stored document, not asked for"
    );
    assert_eq!(
        fixture.provider.requests().len(),
        before,
        "opening it reached the wire for nothing: {:?}",
        fixture.provider.requests()
    );
}

/// A settled lead whose documents are missing is a broken invariant, not a cold
/// cache. Picking it fails loudly, and stores no pick — so nothing is left
/// naming a release the pane could not draw.
#[tokio::test(flavor = "multi_thread")]
#[serial(musicbrainz)]
async fn a_settled_lead_with_no_documents_fails_loud() {
    let fixture = Fixture::new("offline-miss").await;
    let dir = fixture.disc_id_candidate("Album");
    let probed = fixture.probed_total_ms(&dir);
    fixture.scan(1).await;
    fixture
        .store_settled_verdict(&dir, "mb-missing-1", "rg-missing-1", probed)
        .await;

    let error = fixture
        .import
        .select_candidate_metadata_provenance(
            dir.to_string_lossy().into_owned(),
            crate::import::MetadataProvenance::ExternalRelease {
                source: crate::import::MetadataSource::MusicBrainz,
                release_id: "mb-missing-1".to_string(),
            },
        )
        .await
        .expect_err("a settled lead with nothing archived must not silently re-fetch");

    assert!(
        matches!(&error, crate::import::ImportError::Internal { detail }
            if detail.contains("mb-missing-1")),
        "unexpected error: {error}"
    );
}

/// A pick identification never made — another pressing on the list, a manual
/// search hit — fetches, and archives what it fetched, so opening it again is
/// local too.
#[tokio::test(flavor = "multi_thread")]
#[serial(musicbrainz)]
async fn a_pick_outside_the_verdict_archives_what_it_fetched() {
    let fixture = Fixture::new("manual-pick").await;
    let dir = fixture.disc_id_candidate("Album");
    let probed = fixture.probed_total_ms(&dir);
    fixture.scan(1).await;
    fixture.provider.route(
        "/release/mb-manual-1?",
        200,
        release_json("mb-manual-1", "rg-manual-1", &[probed, 0]),
    );

    assert!(
        fixture.archived("mb-manual-1").await.is_none(),
        "nothing has fetched this release yet"
    );

    let pick = || crate::import::MetadataProvenance::ExternalRelease {
        source: crate::import::MetadataSource::MusicBrainz,
        release_id: "mb-manual-1".to_string(),
    };
    fixture
        .import
        .select_candidate_metadata_provenance(dir.to_string_lossy().into_owned(), pick())
        .await
        .expect("a manual pick fetches");

    assert!(
        fixture.archived("mb-manual-1").await.is_some(),
        "and archives the release it fetched"
    );

    // Re-picking it costs nothing.
    let before = fixture.provider.requests().len();
    fixture
        .import
        .select_candidate_metadata_provenance(dir.to_string_lossy().into_owned(), pick())
        .await
        .expect("re-picking reads what the first pick archived");
    assert_eq!(
        fixture.provider.requests().len(),
        before,
        "the second pick reached the wire: {:?}",
        fixture.provider.requests()
    );
}

/// Skipped is a decision the user already made, so automatic identification
/// excludes it until the user explicitly unskips it.
#[tokio::test(flavor = "multi_thread")]
#[serial(musicbrainz)]
async fn a_skipped_candidate_is_not_swept() {
    let fixture = Fixture::new("skipped").await;
    let dir = fixture.disc_id_candidate("Album");
    let probed = fixture.probed_total_ms(&dir);
    fixture.provider.route(
        "/discid/",
        200,
        discid_json("mb-skipped-1", "rg-skipped-1", &[probed, 0]),
    );
    fixture.provider.route(
        "/release/mb-skipped-1?",
        200,
        release_json("mb-skipped-1", "rg-skipped-1", &[probed, 0]),
    );
    fixture.scan(1).await;
    fixture
        .import
        .set_candidate_skipped(dir.to_string_lossy().into_owned(), true)
        .await
        .unwrap();

    fixture.sweep_once().await;

    assert!(
        fixture.provider.requests().is_empty(),
        "a skipped candidate costs the provider nothing: {:?}",
        fixture.provider.requests()
    );
    assert!(
        fixture.identified_for(&dir).await.is_none(),
        "and produces no identification result"
    );
}

#[tokio::test(flavor = "multi_thread")]
#[serial(musicbrainz)]
async fn unskipping_a_stored_candidate_mid_pass_counts_it_immediately() {
    let fixture = Fixture::new("unskip-mid-pass").await;
    fixture
        .extraction
        .register_analyzer(Arc::new(BarcodeAnalyzer {
            barcode: "0123456789012".to_string(),
        }));
    let stored = fixture.barcode_candidate("Stored");
    let running = fixture.disc_id_candidate("Running");
    std::fs::write(running.join("notes.txt"), "distinct candidate").unwrap();
    let probed = fixture.probed_total_ms(&running);
    fixture.provider.route(
        "/release?",
        200,
        search_json("mb-unskip-stored", "rg-unskip-stored"),
    );
    fixture.provider.route(
        "/release/mb-unskip-stored?",
        200,
        release_json("mb-unskip-stored", "rg-unskip-stored", &[probed, 0]),
    );
    fixture.provider.route(
        "/discid/",
        200,
        discid_json("mb-unskip-running", "rg-unskip-running", &[probed, 0]),
    );
    fixture.provider.route(
        "/release/mb-unskip-running?",
        200,
        release_json("mb-unskip-running", "rg-unskip-running", &[probed, 0]),
    );
    fixture.scan(2).await;
    fixture.start_explicit_lookup(&stored);
    fixture.await_identified_row(&stored).await;
    fixture
        .import
        .set_candidate_skipped(stored.to_string_lossy().into_owned(), true)
        .await
        .unwrap();

    fixture.provider.hold("/discid/");
    let mut events = fixture.import.subscribe_events();
    let context = fixture.context();
    let token = CancellationToken::new();
    let pass = tokio::spawn(async move { run_pass_for_test(&context, &token).await });
    wait_for_request(&fixture.provider, "/discid/", 1).await;
    fixture
        .import
        .set_candidate_skipped(stored.to_string_lossy().into_owned(), false)
        .await
        .unwrap();
    fixture.provider.release();
    tokio::time::timeout(Duration::from_secs(15), pass)
        .await
        .expect("pass finishes after unskip")
        .unwrap();

    let row = fixture
        .stored_for(&stored)
        .await
        .expect("stored row remains");
    let verdict = identify_result(&row).verdict.clone();
    assert!(matches!(&verdict, TerminalVerdict::Found { matches, .. }
        if matches[0].source_tracks.is_some()));
    let progress: Vec<_> = drain_events(&mut events)
        .into_iter()
        .filter_map(|event| match event {
            ImportEvent::QueueIdentifyProgress { identified, total } => Some((identified, total)),
            _ => None,
        })
        .collect();
    assert!(
        progress.contains(&(1, 2)),
        "the stored unskipped candidate is counted immediately: {progress:?}"
    );
    assert_eq!(progress.last(), Some(&(2, 2)), "{progress:?}");
}
