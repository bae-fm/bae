//! The identify driver: a run's start over the sources it reads once, the
//! snapshots it reads, its cancel and its end, and what the step that ends it
//! keeps.

use super::*;
use crate::identification::tests::*;
use bae_catalogs::catalog::Catalog;
use bae_catalogs::records::LookupRef;
use bae_catalogs::test_releases::{
    discogs_release_json, discogs_search_json, mb_barcode_search_json, mb_release_json,
};
use bae_identify::signals::{BarcodeSignal, DiscIdSignal, Signals, TextSignal};
use bae_identify::IdentifyState;
use bae_identify::TerminalVerdict;
use bae_store::Store;
use bae_util::test_channels::HANG_GUARD;

async fn setup_inner() -> (Arc<IdentifyServiceInner>, tempfile::TempDir) {
    setup_inner_reporting_to(bae_diagnostics::Diagnostics::noop()).await
}

/// [`setup_inner`] reporting its bugs to `diagnostics`.
pub(super) async fn setup_inner_reporting_to(
    diagnostics: bae_diagnostics::Diagnostics,
) -> (Arc<IdentifyServiceInner>, tempfile::TempDir) {
    let temp_dir = tempfile::TempDir::new().unwrap();
    let db_path = temp_dir.path().join("test.db");
    let db = Store::new_test(
        crate::TEST_SCHEMA,
        db_path.to_str().unwrap(),
        Arc::new(coven::SystemClock),
    )
    .await
    .unwrap();

    let catalogs = crate::CatalogSessions::new(
        bae_catalogs::fake_catalogs::FakeCatalogs::offline().providers(),
        db.clone(),
        bae_preferences::PreferencesRow::new(db.clone()),
        bae_diagnostics::Diagnostics::noop(),
        tokio::runtime::Handle::current(),
    );
    let release_reads =
        crate::ReleaseReads::new(crate::tables::ImportTables::new(db.clone()), catalogs.clone());
    let candidates = CandidateRuntime::for_test().await;
    let event_tx = ImportEventBus::new(candidates.clone());
    let handle = IdentifyService::new(
        db.clone(),
        crate::tables::ImportTables::new(db),
        catalogs,
        release_reads,
        tokio::runtime::Handle::current(),
        event_tx,
        candidates,
        diagnostics,
    );
    (handle.inner, temp_dir)
}

/// A removal sent while a run is in flight ends it by the time the send
/// returns.
#[tokio::test]
async fn a_removed_candidates_run_ends_in_the_send_that_removes_it() {
    let (inner, _tmp) = setup_inner().await;
    let token = inner.candidates.start_work(
        CandidateWork::Identify,
        crate::CandidateKey::test("k"),
        |token, _| token,
    );

    inner.event_tx.send(ImportEvent::Scan(crate::ScanEvent::CandidateRemoved {
        candidate_key: crate::CandidateKey::test("k"),
    }));

    assert!(token.is_cancelled(), "the removed candidate's run is cancelled");
    assert!(
        !inner.candidates.is_working(CandidateWork::Identify, &crate::CandidateKey::test("k")),
        "and no longer registered"
    );
}

/// Neither a disc-ID artifact nor a barcode source, so the reducer settles on
/// `ManualOnly` with no network effect — the driver loop runs end to end
/// without touching MB or Discogs.
fn absent_signals() -> Signals {
    Signals {
        origin: bae_catalogs::AudioOrigin::default(),
        disc_id: DiscIdSignal::Absent,
        barcode: BarcodeSignal::Absent,
        text: TextSignal::Settled { catalogs: vec![], free_text: vec![] },
        text_pool: Vec::new(),
        isrcs: Vec::new(),
        track_titles: Vec::new(),
    }
}

/// Wait until every driver task has returned, so everything they report
/// is already on the bus.
pub(super) async fn drivers_ended(inner: &Arc<IdentifyServiceInner>) {
    inner.driver_tasks.close();
    tokio::time::timeout(HANG_GUARD, inner.driver_tasks.wait())
        .await
        .expect("every driver returns");
}

/// Read `k`'s states until `wanted` answers one, or the events end.
async fn await_state<T>(
    events: &mut crate::ImportEvents,
    wanted: impl Fn(&IdentifyState) -> Option<T>,
) -> Option<T> {
    tokio::time::timeout(HANG_GUARD, async {
        while let Some(event) = events.recv().await {
            if let ImportEvent::IdentifyStateChanged { candidate_key, state, .. } = event {
                if candidate_key == crate::CandidateKey::test("k") {
                    if let Some(found) = wanted(&state) {
                        return Some(found);
                    }
                }
            }
        }
        None
    })
    .await
    .ok()
    .flatten()
}

/// `k`'s states already reported.
fn reported_states(events: &mut crate::ImportEvents) -> Vec<IdentifyState> {
    std::iter::from_fn(|| events.try_recv().ok())
        .filter_map(|event| match event {
            ImportEvent::IdentifyStateChanged { candidate_key, state, .. }
                if candidate_key == crate::CandidateKey::test("k") =>
            {
                Some(state)
            }
            _ => None,
        })
        .collect()
}

fn settled_snapshot() -> SignalsSnapshot {
    SignalsSnapshot {
        signals: absent_signals(),
        audio: bae_identify::signals::AudioFacts::default(),
        artwork: bae_identify::signals::ArtworkScan::Absent,
    }
}

/// What extraction says while its artwork pass is still going: nothing a
/// run can settle on.
fn scanning_snapshot() -> SignalsSnapshot {
    SignalsSnapshot {
        signals: Signals {
            barcode: BarcodeSignal::Scanning { codes: vec![] },
            text: TextSignal::Scanning { catalogs: vec![], free_text: vec![] },
            ..absent_signals()
        },
        audio: bae_identify::signals::AudioFacts::default(),
        artwork: bae_identify::signals::ArtworkScan::Reading {
            current: None,
            position: 1,
            total: 1,
        },
    }
}

/// The run's verdict is where the run ends. Nobody cancels it and no
/// `Idle` follows: the driver deregisters itself on the terminal state,
/// so "a driver is registered" means "work is in flight" and nothing else.
#[tokio::test(flavor = "multi_thread")]
async fn a_driver_that_reaches_its_verdict_is_gone_without_a_cancel() {
    let (inner, _tmp) = setup_inner().await;
    let handle = IdentifyService { inner: inner.clone() };
    let mut bus_rx = inner.event_tx.every_event();

    let (snapshots, watch) = tokio::sync::watch::channel(None);
    assert!(handle
        .start(
            handle.new_run(),
            crate::CandidateKey::test("k"),
            RunSubject::Release,
            CallClass::REQUESTED,
            LookupChoices::default(),
            None,
            crate::extraction::ExtractionWatch::of_test_channel(watch),
        )
        .await
        .unwrap());
    // Feed the signals over the watch, as the extraction service would.
    snapshots.send_replace(Some(settled_snapshot()));

    assert!(
        await_state(&mut bus_rx, |state| {
            matches!(state, IdentifyState::ManualOnly { .. }).then_some(())
        })
        .await
        .is_some(),
        "the run reports its terminal ManualOnly state"
    );
    drivers_ended(&inner).await;
    assert!(
        !handle.is_running(&crate::CandidateKey::test("k")),
        "the run that answered is not still in flight"
    );

    // And nothing follows it: a terminal state is not chased by the `Idle`
    // a teardown would report.
    let after = reported_states(&mut bus_rx);
    assert!(after.is_empty(), "the settled run reported nothing after its verdict: {after:?}");
}

/// The watch holds the latest snapshot rather than queueing them. Two
/// land before the driver is even up — as they do when extraction is
/// quick, or the runtime is busy — and the driver reads the settled one,
/// which is the whole of what was read and the only one it needs.
#[tokio::test(flavor = "multi_thread")]
async fn a_run_reads_the_latest_snapshot_however_many_landed_before_it_looked() {
    let (inner, _tmp) = setup_inner().await;
    let handle = IdentifyService { inner: inner.clone() };
    let mut bus_rx = inner.event_tx.every_event();

    let (snapshots, watch) = tokio::sync::watch::channel(None);
    snapshots.send_replace(Some(scanning_snapshot()));
    snapshots.send_replace(Some(settled_snapshot()));
    assert!(handle
        .start(
            handle.new_run(),
            crate::CandidateKey::test("k"),
            RunSubject::Release,
            CallClass::REQUESTED,
            LookupChoices::default(),
            None,
            crate::extraction::ExtractionWatch::of_test_channel(watch),
        )
        .await
        .unwrap());

    assert!(
        await_state(&mut bus_rx, |state| {
            matches!(state, IdentifyState::ManualOnly { .. }).then_some(())
        })
        .await
        .is_some(),
        "the run settled on the snapshot that was current when it looked"
    );
}

/// The extraction's watch closing is the extraction ending, not the run:
/// the lookups it started still answer, and the run waits on them — or
/// on its cancel.
#[tokio::test(flavor = "multi_thread")]
async fn an_extraction_ending_does_not_end_the_run() {
    let (inner, _tmp) = setup_inner().await;
    let handle = IdentifyService { inner: inner.clone() };
    let mut bus_rx = inner.event_tx.every_event();

    let (snapshots, watch) = tokio::sync::watch::channel(None);
    assert!(handle
        .start(
            handle.new_run(),
            crate::CandidateKey::test("k"),
            RunSubject::Release,
            CallClass::REQUESTED,
            LookupChoices::default(),
            None,
            crate::extraction::ExtractionWatch::of_test_channel(watch),
        )
        .await
        .unwrap());
    snapshots.send_replace(Some(scanning_snapshot()));
    drop(snapshots);

    // The scanning snapshot is the run's last input; its state is the last
    // thing anything feeds the driver, and the extraction closing is not a
    // verdict.
    await_state(&mut bus_rx, |state| {
        matches!(state, IdentifyState::Triangulating { .. }).then_some(())
    })
    .await
    .expect("the scanning snapshot puts the run in flight");
    assert!(
        reported_states(&mut bus_rx).iter().all(|state| !state.is_terminal()),
        "no verdict comes of an extraction that said nothing settled"
    );
    assert!(handle.is_running(&crate::CandidateKey::test("k")), "the run is still in flight");

    handle.cancel(&crate::CandidateKey::test("k"));
    drivers_ended(&inner).await;
    assert!(
        reported_states(&mut bus_rx).iter().any(|state| matches!(state, IdentifyState::Idle)),
        "and its cancel still lands"
    );
}

/// A cancel mid-run is the other way out: `Idle` says the run wrote
/// nothing, and the driver is gone behind it.
#[tokio::test(flavor = "multi_thread")]
async fn a_cancel_mid_run_reports_idle_and_deregisters() {
    let (inner, _tmp) = setup_inner().await;
    let handle = IdentifyService { inner: inner.clone() };
    let mut bus_rx = inner.event_tx.every_event();

    let (_snapshots, watch) = tokio::sync::watch::channel(None);
    assert!(handle
        .start(
            handle.new_run(),
            crate::CandidateKey::test("k"),
            RunSubject::Release,
            CallClass::REQUESTED,
            LookupChoices::default(),
            None,
            crate::extraction::ExtractionWatch::of_test_channel(watch),
        )
        .await
        .unwrap());
    assert!(handle.is_running(&crate::CandidateKey::test("k")), "the run is in flight");
    assert_eq!(handle.running_keys(), vec![crate::CandidateKey::test("k")]);

    // No signals: the run sits in `Triangulating` until the cancel lands.
    handle.cancel(&crate::CandidateKey::test("k"));

    drivers_ended(&inner).await;
    assert!(
        reported_states(&mut bus_rx).iter().any(|state| matches!(state, IdentifyState::Idle)),
        "the cancelled run reports Idle"
    );
    assert!(handle.running_keys().is_empty());
}

/// The step that settles a run also asks to keep what its documents state
/// each album is. The run ends on that step, and what it asked for is kept
/// all the same: a stored release of the album then names the other.
#[tokio::test(flavor = "multi_thread")]
async fn what_a_run_keeps_is_kept_by_the_step_that_ends_it() {
    use bae_catalogs::album_links::AlbumLinks;
    use bae_catalogs::records::{MetadataRef, SourcePayload};
    use bae_identify::documents::{DocumentReading, ReleaseDocument, ReleaseReading};
    use bae_identify::state::{
        BarcodeProgress, CatalogProgress, DiscidProgress, IsrcProgress, SearchProgress,
        SignalsContext,
    };

    let (inner, _tmp) = setup_inner().await;
    let tables = crate::tables::ImportTables::new(inner.store.clone());
    let discogs = bae_catalogs::payloads::ReleasePayloads::for_test(
        LookupRef::new(LookupCatalog::Discogs, "4242"),
        serde_json::json!({
            "id": 4242,
            "title": "Album Title",
            "master_id": 909,
            "artists": [{ "id": 11, "name": "Artist Name" }],
            "tracklist": [{ "position": "1", "title": "Track One", "type_": "track" }]
        })
        .to_string(),
        Vec::<SourcePayload>::new(),
    )
    .extract()
    .unwrap();
    tables.save_source_release(&discogs).await.unwrap();

    let found = bae_catalogs::search::MetadataResult::for_test(
        LookupCatalog::MusicBrainz,
        "mb-release",
        Some("mb-group"),
    );
    let reading = IdentifyState::Triangulating {
        discid: DiscidProgress::Lookups {
            discs: vec![bae_identify::DiscLookup {
                disc_id: "d".to_string(),
                state: bae_identify::LookupState::Done { results: vec![found] },
            }],
        },
        barcode: BarcodeProgress::Skipped,
        catalog: CatalogProgress::Skipped,
        isrc: IsrcProgress::Skipped,
        search: SearchProgress::Skipped,
        context: SignalsContext {
            text_settled: true,
            documents: DocumentReading::Reading(Vec::new()),
            ..SignalsContext::default()
        },
    };
    let document = ReleaseDocument {
        labels: Vec::new(),
        barcodes: Vec::new(),
        album_first_year: None,
        source_tracks: bae_catalogs::search::SourceTracks::Listed { count: 1 },
        track_titles: Vec::new(),
        notes: Vec::new(),
        links: Vec::new(),
        album_links: AlbumLinks::Read(vec![MetadataRef::new(Catalog::Discogs, "909")]),
        stored: std::sync::Arc::new(bae_catalogs::source_release::SourceRelease::for_test(
            LookupRef::new(LookupCatalog::MusicBrainz, "mb-release"),
        )),
    };
    let (event_tx, _event_rx) = mpsc::unbounded_channel();
    let driver = Driver {
        inner: inner.clone(),
        run: IdentifyRunId::for_test(1),
        key: crate::CandidateKey::test("k"),
        subject: RunSubject::Release,
        class: CallClass::REQUESTED,
        event_tx,
        token: CancellationToken::new(),
    };
    let settled = driver
        .advance(
            reading,
            IdentifyEvent::ReleasesRead {
                read: vec![ReleaseReading {
                    release: LookupRef::new(LookupCatalog::MusicBrainz, "mb-release"),
                    document: Ok(document),
                }],
            },
            &mut RunStanding::NotStarted,
        )
        .await;
    assert!(settled.is_terminal(), "the step ends the run: {settled:?}");

    drivers_ended(&inner).await;
    let stored = inner
        .tables
        .load_source_release(&LookupRef::new(LookupCatalog::Discogs, "4242"))
        .await
        .unwrap()
        .expect("the release was stored");
    assert!(
        stored
            .records()
            .iter()
            .any(|record| record.album_ref()
                == Some(MetadataRef::new(Catalog::MusicBrainz, "mb-group"))),
        "the kept link joins the group to the stored release: {:?}",
        stored.records()
    );
}

/// A run reads the sources this library asks once, at its start. So the run
/// that replaces it is what a switched-off source reaches — the replacement
/// asks what is left, and the source nobody asks any more is not asked again.
/// This is the restart `AppServices::set_metadata_source_enabled` starts for
/// every key `IdentifyService::running_keys` names.
#[tokio::test(flavor = "multi_thread")]
async fn a_run_restarted_over_a_shorter_provider_list_asks_only_what_is_left() {
    let fixture = Fixture::new("restart-drops-a-source").await;
    fixture.use_discogs().await;
    fixture.import.register_artwork_analyzer(Arc::new(BarcodeAnalyzer {
        barcode: PAIRED_BARCODE.to_string(),
    }));
    let dir = fixture.barcode_candidate("From Barcode");
    let key = crate::CandidateKey::folder(&dir);
    let probed = fixture.probed_total_ms(&dir);
    fixture.provider.route(
        "/release?",
        200,
        mb_barcode_search_json(&[("mb-drop-1", "rg-drop-1", PAIRED_BARCODE)]),
    );
    fixture.provider.route(
        "/release/mb-drop-1?",
        200,
        mb_release_json("mb-drop-1", "rg-drop-1", &[probed, 0]),
    );
    fixture.provider.route(
        "/database/search",
        200,
        discogs_search_json("70000201", PAIRED_BARCODE_AS_DISCOGS_PRINTS_IT),
    );
    fixture.provider.route("/releases/70000201", 200, discogs_release_json("70000201"));
    fixture.musicbrainz.seed_discogs_page_lookup(&[(
        bae_catalogs::musicbrainz::DiscogsPage::Release("70000201".to_string()),
        None,
    )]);
    // Hold MusicBrainz's answer, so the run is genuinely still asking when the
    // source is switched off rather than racing a run that already settled.
    fixture.provider.hold("/release?");
    // Identified only when asked: nothing found here is identified on its own.
    fixture.set_identify_automatically(false).await;
    fixture.scan(1).await;

    let mut events = fixture.import.every_event();
    fixture.start_explicit_lookup_and_await_run(&dir).await;
    let asked_both = await_run_state(&mut events, &key, |_, _| true).await;
    fixture.provider.wait_for_request("/database/search", 1).await;

    fixture
        .parts
        .edit_preferences(|prefs| {
            prefs.identification.catalogs.set(bae_catalogs::catalog::LookupCatalog::Discogs, false);
        })
        .await;
    fixture.identification().rerun_identify(key.clone());
    await_run_state(&mut events, &key, |run, _| run != asked_both).await;
    fixture.provider.release();

    let row = fixture.await_identified_row(&dir).await;
    let verdict = identify_result(&row).verdict.clone();
    let TerminalVerdict::Found { findings: bae_identify::Findings { matches, .. }, .. } = &verdict
    else {
        panic!("expected a Found verdict, got {verdict:?}");
    };
    assert_eq!(
        matches.iter().map(|result| result.source).collect::<Vec<_>>(),
        vec![bae_catalogs::catalog::LookupCatalog::MusicBrainz],
        "the run that stored the answer asked only the source still switched on"
    );
    assert_eq!(
        fixture.provider.count_containing("/database/search"),
        1,
        "and Discogs was asked once, by the run that started while it was on: {:?}",
        fixture.provider.requests()
    );
}
