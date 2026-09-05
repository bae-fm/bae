//! What the import event channel puts on the UI bus.

use super::*;
use crate::config::{Config, ConfigHandle};
use crate::db::Database;
use crate::import::ImportEvent;
use crate::library::{AppServices, LibraryManager};
use crate::util::rate_limiter::CallPriority;
use coven::StoreDir;
use std::sync::Arc;
use std::time::Duration;
use tempfile::TempDir;

/// A real `AppServices` with the import trio behind it and nothing in its
/// library — every test here drives the import event channel directly.
async fn services() -> (AppServices, TempDir) {
    let temp_dir = TempDir::new().expect("a temp library dir");
    let database = Database::new_test(
        temp_dir
            .path()
            .join("test.db")
            .to_str()
            .expect("a UTF-8 temp path"),
        Arc::new(coven::SystemClock),
        Arc::new(coven::UuidProvider),
    )
    .await
    .expect("the test database opens");
    crate::config::install_test_keyring();
    let config = Config::with_defaults(
        format!("ui-bus-test-{}", uuid::Uuid::new_v4()),
        "test-device".to_string(),
        StoreDir::new(temp_dir.path().to_path_buf()),
        "Test Library".to_string(),
    );
    let manager = LibraryManager::new(
        database,
        Arc::new(ConfigHandle::new(config)),
        Arc::new(coven::SystemClock),
        Arc::new(coven::UuidProvider),
        crate::diagnostics::Diagnostics::noop(),
        tokio::runtime::Handle::current(),
        crate::import::cover_art::RemoteImageCache::for_test(),
    );
    let services = AppServices::for_test(manager)
        .await
        .expect("the import services start");
    (services, temp_dir)
}

fn extracted(catalog: &str) -> crate::signals::Signals {
    crate::signals::Signals {
        disc_id: crate::signals::DiscIdSignal::Absent { track_count: 9 },
        barcode: crate::signals::BarcodeSignal::Settled { codes: Vec::new() },
        text: crate::signals::TextSignal::Settled {
            catalogs: vec![crate::signals::SourcedValue::new(
                catalog.to_string(),
                crate::signals::SignalOrigin::Artwork,
            )],
            free_text: Vec::new(),
        },
        durations: crate::import::probe::SourceDurations::totalling(1_000),
    }
}

/// Wait for the runtime recorder to take a key's snapshot in, or give up.
async fn recorded(services: &AppServices, key: &str) -> crate::signals::Signals {
    tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            if let Some(signals) = services.candidate_signals(key) {
                return signals;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .expect("the key's latest snapshot reads back")
}

fn catalog_of(signals: &crate::signals::Signals) -> &str {
    &signals.text.catalogs()[0].value
}

/// Wait for the bus to deliver a signals event for `key`, or give up.
async fn signals_for(
    events: &mut tokio::sync::broadcast::Receiver<UiBusEvent>,
    key: &str,
) -> crate::signals::Signals {
    let deadline = tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            match events.recv().await {
                Ok(UiBusEvent::CandidateSignalsUpdated {
                    key: delivered,
                    signals,
                }) if delivered == key => return signals,
                Ok(_) => continue,
                Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => continue,
                Err(tokio::sync::broadcast::error::RecvError::Closed) => {
                    panic!("the UI bus closed")
                }
            }
        }
    });
    deadline.await.expect("the signals event is delivered")
}

/// Extraction's snapshots reach the one form that reads them without going
/// near the candidate's runtime — routed by key, the way a loudness tick is —
/// and the key's latest snapshot is readable on its own, so a form that opens
/// partway through a run starts with the pool the run has already built.
#[tokio::test(flavor = "multi_thread")]
async fn extracted_signals_reach_the_bus_by_key_and_read_back_for_that_key() {
    let (services, _temp) = services().await;
    let bus = UiEventBus::new();
    bus.wire(&services, &tokio::runtime::Handle::current());
    let mut events = bus.subscribe();

    let key = "reidentify:release-1";
    assert!(
        services.candidate_signals(key).is_none(),
        "nothing has been extracted for the key yet"
    );

    services.import_emit_event_for_test(ImportEvent::SignalsUpdated {
        candidate_key: "/watch/other".to_string(),
        signals: extracted("OTHER-1"),
        artwork: crate::signals::ArtworkScan::Absent,
        priority: CallPriority::Background,
    });
    services.import_emit_event_for_test(ImportEvent::SignalsUpdated {
        candidate_key: key.to_string(),
        signals: extracted("CAT-1"),
        artwork: crate::signals::ArtworkScan::Absent,
        priority: CallPriority::Background,
    });

    let delivered = signals_for(&mut events, key).await;
    assert_eq!(catalog_of(&delivered), "CAT-1");

    // The recorder reads the same channel on its own task, so its write lands
    // independently of the relay's delivery — wait for it rather than assume
    // the two are in step.
    assert_eq!(catalog_of(&recorded(&services, key).await), "CAT-1");
    assert_eq!(
        catalog_of(&recorded(&services, "/watch/other").await),
        "OTHER-1"
    );
}
