use super::*;
use crate::config::{Config, ConfigHandle};
use crate::db::Database;
use crate::identify::IdentifyState;
use crate::signals::{BarcodeSignal, DiscIdSignal, Signals, TextSignal};
use std::time::Duration;
use tokio::sync::mpsc::UnboundedReceiver;

async fn setup_inner() -> (Arc<IdentifyServiceInner>, tempfile::TempDir) {
    let temp_dir = tempfile::TempDir::new().unwrap();
    let db_path = temp_dir.path().join("test.db");
    let database = Database::new_test(db_path.to_str().unwrap(), Arc::new(coven::SystemClock))
        .await
        .unwrap();
    let library_dir = coven::StoreDir::new(temp_dir.path());
    let library_id = format!("test-{}", temp_dir.path().display());
    let config = Config::with_defaults(
        library_id.clone(),
        "test-device".to_string(),
        library_dir,
        "Test Library".to_string(),
    );
    crate::config::install_test_keyring();
    let manager = LibraryManager::new(
        database,
        crate::config::AppDir::under_home(temp_dir.path()),
        Arc::new(ConfigHandle::new(config)),
        Arc::new(coven::SystemClock),
        Arc::new(coven::UuidProvider),
        crate::diagnostics::Diagnostics::noop(),
        tokio::runtime::Handle::current(),
        crate::import::cover_art::RemoteImageCache::for_test(crate::util::http::Http::for_test()),
        crate::providers::Providers::offline(),
    );
    let candidates = CandidateRuntime::default();
    let event_tx = ImportEventBus::new(candidates.clone());
    let handle = IdentifyServiceHandle::new(
        manager,
        tokio::runtime::Handle::current(),
        event_tx,
        candidates,
    );
    (handle.inner, temp_dir)
}

/// A removal sent while a run is in flight ends it by the time the send
/// returns.
#[tokio::test]
async fn a_removed_candidates_run_ends_in_the_send_that_removes_it() {
    let (inner, _tmp) = setup_inner().await;
    let token =
        inner
            .candidates
            .start_work(CandidateWork::Identify, "k".to_string(), |token, _| token);

    inner.event_tx.send(ImportEvent::Scan(
        crate::import::ScanEvent::CandidateRemoved {
            candidate_key: "k".to_string(),
        },
    ));

    assert!(
        token.is_cancelled(),
        "the removed candidate's run is cancelled"
    );
    assert!(
        !inner.candidates.is_working(CandidateWork::Identify, "k"),
        "and no longer registered"
    );
}

/// Neither a disc-ID artifact nor a barcode source, so the reducer settles on
/// `ManualOnly` with no network effect — the driver loop runs end to end
/// without touching MB or Discogs.
fn absent_signals() -> Signals {
    Signals {
        origin: crate::signals::AudioOrigin::default(),
        disc_id: DiscIdSignal::Absent,
        barcode: BarcodeSignal::Absent,
        text: TextSignal::Settled {
            catalogs: vec![],
            free_text: vec![],
        },
        text_pool: Vec::new(),
        isrcs: Vec::new(),
        track_titles: Vec::new(),
    }
}

/// Wait until every driver task has returned, so everything they report
/// is already on the bus.
async fn drivers_ended(inner: &Arc<IdentifyServiceInner>) {
    inner.driver_tasks.close();
    tokio::time::timeout(Duration::from_secs(30), inner.driver_tasks.wait())
        .await
        .expect("every driver returns");
}

/// Read `k`'s states until `wanted` answers one, or the wait runs out.
async fn await_state<T>(
    events: &mut UnboundedReceiver<ImportEvent>,
    wanted: impl Fn(&IdentifyState) -> Option<T>,
) -> Option<T> {
    tokio::time::timeout(Duration::from_secs(5), async {
        while let Some(event) = events.recv().await {
            if let ImportEvent::IdentifyStateChanged {
                candidate_key,
                state,
                ..
            } = event
            {
                if candidate_key == "k" {
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
fn reported_states(events: &mut UnboundedReceiver<ImportEvent>) -> Vec<IdentifyState> {
    std::iter::from_fn(|| events.try_recv().ok())
        .filter_map(|event| match event {
            ImportEvent::IdentifyStateChanged {
                candidate_key,
                state,
                ..
            } if candidate_key == "k" => Some(state),
            _ => None,
        })
        .collect()
}

fn settled_snapshot() -> SignalsSnapshot {
    SignalsSnapshot {
        signals: absent_signals(),
        audio: crate::signals::AudioFacts::default(),
        artwork: crate::signals::ArtworkScan::Absent,
    }
}

/// What extraction says while its artwork pass is still going: nothing a
/// run can settle on.
fn scanning_snapshot() -> SignalsSnapshot {
    SignalsSnapshot {
        signals: Signals {
            barcode: BarcodeSignal::Scanning { codes: vec![] },
            text: TextSignal::Scanning {
                catalogs: vec![],
                free_text: vec![],
            },
            ..absent_signals()
        },
        audio: crate::signals::AudioFacts::default(),
        artwork: crate::signals::ArtworkScan::Reading {
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
    let handle = IdentifyServiceHandle {
        inner: inner.clone(),
    };
    let mut bus_rx = inner.event_tx.every_event();

    let (snapshots, watch) = tokio::sync::watch::channel(None);
    assert!(handle.start(
        handle.new_run(),
        "k".to_string(),
        CallPriority::Interactive,
        IdentificationSteps::default(),
        LookupChoices::default(),
        None,
        watch,
    ));
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
        !handle.is_running("k"),
        "the run that answered is not still in flight"
    );

    // And nothing follows it: a terminal state is not chased by the `Idle`
    // a teardown would report.
    let after = reported_states(&mut bus_rx);
    assert!(
        after.is_empty(),
        "the settled run reported nothing after its verdict: {after:?}"
    );
}

/// The watch holds the latest snapshot rather than queueing them. Two
/// land before the driver is even up — as they do when extraction is
/// quick, or the runtime is busy — and the driver reads the settled one,
/// which is the whole of what was read and the only one it needs.
#[tokio::test(flavor = "multi_thread")]
async fn a_run_reads_the_latest_snapshot_however_many_landed_before_it_looked() {
    let (inner, _tmp) = setup_inner().await;
    let handle = IdentifyServiceHandle {
        inner: inner.clone(),
    };
    let mut bus_rx = inner.event_tx.every_event();

    let (snapshots, watch) = tokio::sync::watch::channel(None);
    snapshots.send_replace(Some(scanning_snapshot()));
    snapshots.send_replace(Some(settled_snapshot()));
    assert!(handle.start(
        handle.new_run(),
        "k".to_string(),
        CallPriority::Interactive,
        IdentificationSteps::default(),
        LookupChoices::default(),
        None,
        watch,
    ));

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
    let handle = IdentifyServiceHandle {
        inner: inner.clone(),
    };
    let mut bus_rx = inner.event_tx.every_event();

    let (snapshots, watch) = tokio::sync::watch::channel(None);
    assert!(handle.start(
        handle.new_run(),
        "k".to_string(),
        CallPriority::Interactive,
        IdentificationSteps::default(),
        LookupChoices::default(),
        None,
        watch,
    ));
    snapshots.send_replace(Some(scanning_snapshot()));
    drop(snapshots);

    assert!(
        await_state(&mut bus_rx, |state| state.is_terminal().then_some(()))
            .await
            .is_none(),
        "no verdict comes of an extraction that said nothing settled"
    );
    assert!(handle.is_running("k"), "the run is still in flight");

    handle.cancel("k");
    drivers_ended(&inner).await;
    assert!(
        reported_states(&mut bus_rx)
            .iter()
            .any(|state| matches!(state, IdentifyState::Idle)),
        "and its cancel still lands"
    );
}

/// A cancel mid-run is the other way out: `Idle` says the run wrote
/// nothing, and the driver is gone behind it.
#[tokio::test(flavor = "multi_thread")]
async fn a_cancel_mid_run_reports_idle_and_deregisters() {
    let (inner, _tmp) = setup_inner().await;
    let handle = IdentifyServiceHandle {
        inner: inner.clone(),
    };
    let mut bus_rx = inner.event_tx.every_event();

    let (_snapshots, watch) = tokio::sync::watch::channel(None);
    assert!(handle.start(
        handle.new_run(),
        "k".to_string(),
        CallPriority::Interactive,
        IdentificationSteps::default(),
        LookupChoices::default(),
        None,
        watch,
    ));
    assert!(handle.is_running("k"), "the run is in flight");
    assert_eq!(handle.running_keys(), vec!["k".to_string()]);

    // No signals: the run sits in `Triangulating` until the cancel lands.
    handle.cancel("k");

    drivers_ended(&inner).await;
    assert!(
        reported_states(&mut bus_rx)
            .iter()
            .any(|state| matches!(state, IdentifyState::Idle)),
        "the cancelled run reports Idle"
    );
    assert!(handle.running_keys().is_empty());
}

/// The step that settles a run also asks to keep what its documents state
/// each album is. The run ends on that step, and what it asked for is kept
/// all the same: a stored release of the album then names the other.
#[tokio::test(flavor = "multi_thread")]
async fn what_a_run_keeps_is_kept_by_the_step_that_ends_it() {
    use crate::identify::documents::{DocumentReading, ReleaseDocument, ReleaseReading};
    use crate::identify::state::{
        BarcodeProgress, CatalogProgress, DiscidProgress, IsrcProgress, SearchProgress,
        SignalsContext,
    };
    use crate::import::album_links::{AlbumLink, AlbumLinks, AlbumStatement};
    use crate::import::{MetadataRef, SourcePayload};

    let (inner, _tmp) = setup_inner().await;
    let discogs = crate::import::payloads::ReleasePayloads::for_test(
        MetadataRef::new(Catalog::Discogs, "4242"),
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
    inner
        .library_manager
        .save_source_release(&discogs)
        .await
        .unwrap();

    let found = crate::import::search::MetadataResult::for_test(
        Catalog::MusicBrainz,
        "mb-release",
        Some("mb-group"),
    );
    let reading = IdentifyState::Triangulating {
        discid: DiscidProgress::Done {
            results: vec![(found, crate::db::LibraryStatus::absent("mb-release"))],
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
        barcode: None,
        album_first_year: None,
        source_tracks: crate::import::search::SourceTracks::Listed { count: 1 },
        track_titles: Vec::new(),
        notes: Vec::new(),
        links: Vec::new(),
        album_links: AlbumLinks::Read(vec![AlbumLink {
            album: MetadataRef::new(Catalog::Discogs, "909"),
            stated: AlbumStatement::Page,
        }]),
    };
    let (event_tx, _event_rx) = mpsc::unbounded_channel();
    let driver = Driver {
        inner: inner.clone(),
        run: IdentifyRunId::for_test(1),
        key: "k".to_string(),
        priority: CallPriority::Interactive,
        event_tx,
        token: CancellationToken::new(),
    };
    let settled = driver.advance(
        reading,
        IdentifyEvent::ReleasesRead {
            read: vec![ReleaseReading {
                release: MetadataRef::new(Catalog::MusicBrainz, "mb-release"),
                document: Ok(document),
            }],
        },
    );
    assert!(settled.is_terminal(), "the step ends the run: {settled:?}");

    drivers_ended(&inner).await;
    let stored = inner
        .library_manager
        .load_source_release(&MetadataRef::new(Catalog::Discogs, "4242"))
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

/// A cancelled run's lookup ends where it stands — here, waiting between
/// retries on a provider that never answers — rather than running on.
#[tokio::test]
async fn a_cancelled_run_drops_the_lookup_it_was_waiting_on() {
    struct Dropped(Option<tokio::sync::oneshot::Sender<()>>);
    impl Drop for Dropped {
        fn drop(&mut self) {
            let _ = self.0.take().map(|tx| tx.send(()));
        }
    }
    let (dropped_tx, dropped_rx) = tokio::sync::oneshot::channel();
    let (started_tx, started_rx) = tokio::sync::oneshot::channel();
    let token = CancellationToken::new();
    spawn_until_cancelled(&tokio::runtime::Handle::current(), &token, async move {
        let _guard = Dropped(Some(dropped_tx));
        let _ = started_tx.send(());
        std::future::pending::<()>().await;
    });
    started_rx.await.expect("the lookup starts");
    token.cancel();
    tokio::time::timeout(Duration::from_secs(2), dropped_rx)
        .await
        .expect("the lookup is dropped once its run is cancelled")
        .expect("the guard reports its drop");
}
