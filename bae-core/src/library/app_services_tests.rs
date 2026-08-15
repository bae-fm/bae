use super::*;
use crate::config::{Config, ConfigHandle};
use crate::db::{Database, DbAlbum, DbArtist, DbRelease, DbTrack};
use coven::StoreDir;
use tempfile::TempDir;

/// Build a real `AppServices` — library manager, actor-backed playback,
/// and (natively) the import/identify/extraction trio — wired up exactly
/// as `bootstrap` does for desktop, seeded with one release of
/// `track_count` tracks. Starts playing the release from its first track,
/// so by the time this returns the queue's context is a real,
/// actor-resolved tail — not a hand-built `PlaybackQueueProjection` — for
/// the upcoming-page subscription to project. The seeded tracks have no
/// backing audio file, so preparing the first track for playback fails
/// fast (a DB lookup, no I/O); that failure only stops playback, it
/// doesn't touch the queue the `PlayRelease` command already set.
async fn playing_app_services(track_count: usize) -> (AppServices, Vec<String>, TempDir) {
    let temp_dir = TempDir::new().unwrap();
    let db_path = temp_dir.path().join("test.db");
    let database = Database::new_test(
        db_path.to_str().unwrap(),
        Arc::new(coven::SystemClock),
        std::sync::Arc::new(coven::UuidProvider),
    )
    .await
    .unwrap();

    let artist = DbArtist {
        id: bae_test_support::test_uuid("e36744a5-1a36-460f-891c-e7e558034edf"),
        name: "Test Artist".to_string(),
        sort_name: None,
        discogs_artist_id: None,
        musicbrainz_artist_id: None,
        created_at: chrono::Utc::now(),
    };
    database.insert_artist(&artist).await.unwrap();
    let album = DbAlbum::new_test("Album Title", &artist.id);
    let release = DbRelease::new_test(&album.id, "c61a9e19-f3ba-4728-842c-c59dbc82e238");
    database.insert_album(&album).await.unwrap();
    database.insert_release(&release).await.unwrap();
    let mut track_ids = Vec::with_capacity(track_count);
    for i in 0..track_count {
        let track_id = bae_test_support::test_uuid(&format!("track-{i}"));
        let track = DbTrack::new_test(
            &release.id,
            &track_id,
            &format!("Track {i}"),
            Some(i as i32),
        );
        database.insert_track(&track).await.unwrap();
        track_ids.push(track_id);
    }

    let library_id = format!("app-services-test-{}", uuid::Uuid::new_v4());
    let config = Config::with_defaults(
        library_id.clone(),
        "test-device".to_string(),
        StoreDir::new(temp_dir.path().to_path_buf()),
        "Test Library".to_string(),
    );
    let config_handle = Arc::new(ConfigHandle::new(config));
    crate::config::install_test_keyring();
    let manager = LibraryManager::new(
        database,
        config_handle,
        Arc::new(coven::SystemClock),
        Arc::new(coven::UuidProvider),
        crate::diagnostics::Diagnostics::noop(),
        tokio::runtime::Handle::current(),
        crate::import::cover_art::RemoteImageCache::for_test(),
    );

    // A device-less output, not the real cpal sink: this test drives the
    // playback actor for its queue behavior and never plays audio (track
    // prep fails before any stream is built). Building a cpal output would
    // reach for the system audio device, and on Windows a second such build
    // on a fresh actor thread faults — cpal's process-global WASAPI device
    // enumerator is left dangling once the first actor thread that made it
    // exits (the enumerator dies with that thread's COM apartment), and the
    // test builds one player per case.
    let playback = manager.start_playback_service_with_output(
        tokio::runtime::Handle::current(),
        50,
        false,
        Box::new(crate::playback::audio_output::FailingAudioOutput),
    );

    #[cfg(not(any(target_os = "ios", target_os = "android")))]
    let services = {
        let import = manager
            .start_import_service(tokio::runtime::Handle::current())
            .await
            .unwrap();
        AppServices::new(manager, playback, import)
    };
    #[cfg(any(target_os = "ios", target_os = "android"))]
    let services = AppServices::new(manager, playback);

    services.playback_play_release(release.id.clone(), Some(0), false);

    (services, track_ids, temp_dir)
}

async fn queue_value_with_context(services: &AppServices) -> crate::queue::ResolvedQueueSnapshot {
    let mut values = services.subscribe_queue_values(&tokio::runtime::Handle::current());
    loop {
        let value = tokio::time::timeout(std::time::Duration::from_secs(5), values.recv())
            .await
            .expect("queue subscription delivers")
            .expect("queue subscription stays open")
            .expect("queue projection resolves");
        if value.context.is_some() {
            return value;
        }
    }
}

async fn upcoming_page(
    services: &AppServices,
    offset: u32,
    limit: u32,
) -> crate::queue::ResolvedQueueUpcomingPage {
    let mut values =
        services.subscribe_queue_upcoming_values(&tokio::runtime::Handle::current(), offset, limit);
    tokio::time::timeout(std::time::Duration::from_secs(5), values.recv())
        .await
        .expect("upcoming-page subscription delivers")
        .expect("upcoming-page subscription stays open")
        .expect("upcoming page resolves")
}

/// The upcoming-page subscription slices a real, actor-resolved context
/// tail in order and stamps the page with the same revision as the queue
/// value that selected it.
#[tokio::test]
async fn queue_upcoming_subscription_slices_and_orders_a_live_context_tail() {
    let (services, track_ids, _temp_dir) = playing_app_services(12).await;

    let snapshot = queue_value_with_context(&services).await;
    let page = upcoming_page(&services, 2, 5).await;

    assert_eq!(
        page.revision, snapshot.revision,
        "the page is stamped with the same revision as the live queue's own snapshot"
    );
    let page_track_ids: Vec<&str> = page.items.iter().map(|i| i.track_id.as_str()).collect();
    // track_ids[0] is the currently playing track, so the context tail
    // is track_ids[1..]; offset 2 into that tail lands on track_ids[3].
    let expected: Vec<&str> = track_ids[3..8].iter().map(String::as_str).collect();
    assert_eq!(
        page_track_ids, expected,
        "the slice preserves the tail's order"
    );
}

/// A limit reaching past the live tail's end clamps to what remains, and
/// an offset past the end returns no items rather than erroring — both
/// through the real `AppServices` -> `PlaybackHandle` -> `PlaybackQueue`
/// chain, not `clamp_upcoming_page` called directly.
#[tokio::test]
async fn queue_upcoming_subscription_clamps_to_the_live_tails_end() {
    let (services, track_ids, _temp_dir) = playing_app_services(12).await;
    let snapshot = queue_value_with_context(&services).await;

    // The tail has 11 entries (track_ids[1..12]); offset 9 has only 2
    // left, so a limit of 100 clamps down to those 2.
    let page = upcoming_page(&services, 9, 100).await;
    let page_track_ids: Vec<&str> = page.items.iter().map(|i| i.track_id.as_str()).collect();
    assert_eq!(
        page_track_ids,
        vec![track_ids[10].as_str(), track_ids[11].as_str()],
        "the limit clamps to what remains instead of erroring"
    );

    let empty_page = upcoming_page(&services, 50, 10).await;
    assert!(
        empty_page.items.is_empty(),
        "an offset past the tail's end yields no items"
    );
    assert_eq!(
        empty_page.revision, snapshot.revision,
        "both subscriptions project the current queue revision"
    );
}
