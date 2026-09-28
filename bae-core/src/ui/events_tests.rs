//! What the import runtime tells a UI subscriber.

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

/// A real `AppServices` over an empty library.
async fn services() -> (AppServices, TempDir) {
    let temp_dir = TempDir::new().expect("a temp library dir");
    let database = Database::new_test(
        temp_dir
            .path()
            .join("test.db")
            .to_str()
            .expect("a UTF-8 temp path"),
        Arc::new(coven::SystemClock),
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
        crate::config::AppDir::under_home(temp_dir.path()),
        Arc::new(ConfigHandle::new(config)),
        Arc::new(coven::SystemClock),
        Arc::new(coven::UuidProvider),
        crate::diagnostics::Diagnostics::noop(),
        tokio::runtime::Handle::current(),
        crate::import::cover_art::RemoteImageCache::for_test(crate::util::http::Http::for_test()),
        crate::providers::Providers::offline(),
    );
    let services = AppServices::for_test(manager)
        .await
        .expect("the import services start");
    (services, temp_dir)
}

fn extracted(catalog: &str) -> crate::signals::Signals {
    crate::signals::Signals {
        origin: crate::signals::AudioOrigin::default(),
        disc_id: crate::signals::DiscIdSignal::Absent,
        barcode: crate::signals::BarcodeSignal::Settled { codes: Vec::new() },
        text: crate::signals::TextSignal::Settled {
            catalogs: vec![catalog.to_string()],
            free_text: Vec::new(),
        },
        text_pool: Vec::new(),
        isrcs: Vec::new(),
        track_titles: Vec::new(),
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
    &signals.text.catalogs()[0]
}

/// Wait for the subscriber to hear a signals event for `key`, or give up.
async fn signals_for(events: &mut UiEvents, key: &str) -> crate::signals::Signals {
    let deadline = tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            let heard = events.next().await.expect("the UI events are still told");
            for event in heard {
                if let UiEvent::CandidateSignalsUpdated {
                    key: delivered,
                    signals,
                } = event
                {
                    if delivered == key {
                        return signals;
                    }
                }
            }
        }
    });
    deadline.await.expect("the signals event is delivered")
}

/// Extraction's snapshots reach a subscriber by key, and each key's latest
/// snapshot reads back on its own.
#[tokio::test(flavor = "multi_thread")]
async fn extracted_signals_reach_a_subscriber_by_key_and_read_back_for_that_key() {
    let (services, _temp) = services().await;
    let mut events = services.subscribe_ui_events();

    let key = "reidentify:release-1";
    assert!(
        services.candidate_signals(key).is_none(),
        "nothing has been extracted for the key yet"
    );

    services.import_emit_event_for_test(ImportEvent::SignalsUpdated {
        candidate_key: "/watch/other".to_string(),
        run: crate::identify::IdentifyRunId::for_test(1),
        signals: extracted("OTHER-1"),
        artwork: crate::signals::ArtworkScan::Absent,
        priority: CallPriority::Background,
    });
    services.import_emit_event_for_test(ImportEvent::SignalsUpdated {
        candidate_key: key.to_string(),
        run: crate::identify::IdentifyRunId::for_test(2),
        signals: extracted("CAT-1"),
        artwork: crate::signals::ArtworkScan::Absent,
        priority: CallPriority::Background,
    });

    let delivered = signals_for(&mut events, key).await;
    assert_eq!(catalog_of(&delivered), "CAT-1");

    // The recorder runs on its own task, so wait for its write.
    assert_eq!(catalog_of(&recorded(&services, key).await), "CAT-1");
    assert_eq!(
        catalog_of(&recorded(&services, "/watch/other").await),
        "OTHER-1"
    );
}

/// A subscriber that arrives after the import runtime moved reads the values
/// as they stand, not only the changes after it arrived.
#[tokio::test(flavor = "multi_thread")]
async fn a_late_subscriber_reads_the_import_values_as_they_stand() {
    let (services, _temp) = services().await;
    let mut early = services.subscribe_ui_events();
    let key = "/watch/a/rel1";
    services.import_emit_event_for_test(ImportEvent::SignalsUpdated {
        candidate_key: key.to_string(),
        run: crate::identify::IdentifyRunId::for_test(1),
        signals: extracted("CAT-1"),
        artwork: crate::signals::ArtworkScan::Absent,
        priority: CallPriority::Background,
    });
    assert_eq!(catalog_of(&signals_for(&mut early, key).await), "CAT-1");

    let mut late = services.subscribe_ui_events();
    assert_eq!(catalog_of(&signals_for(&mut late, key).await), "CAT-1");
}
