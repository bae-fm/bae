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
    let database = Database::new_test(db_path.to_str().unwrap(), Arc::new(coven::SystemClock))
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
        crate::config::AppDir::under_home(temp_dir.path()),
        config_handle,
        Arc::new(coven::SystemClock),
        Arc::new(coven::UuidProvider),
        crate::diagnostics::Diagnostics::noop(),
        tokio::runtime::Handle::current(),
        crate::import::cover_art::RemoteImageCache::for_test(crate::util::http::Http::for_test()),
        crate::providers::Providers::offline(),
    );

    // A device with no hardware behind it, not the real cpal sink: this test
    // drives the playback actor for its queue behavior and never plays audio
    // (track prep fails before any stream is built). Building a cpal output
    // would reach for the system audio device, and on Windows a second such
    // build on a fresh actor thread faults — cpal's process-global WASAPI
    // device enumerator is left dangling once the first actor thread that made
    // it exits (the enumerator dies with that thread's COM apartment), and the
    // test builds one player per case.
    let playback = manager.start_playback_service_with_audio_device(
        tokio::runtime::Handle::current(),
        50,
        false,
        Box::new(crate::playback::audio_output::FailingAudioDevice),
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

    services.playback_play_release(release.id.clone(), Some(track_ids[0].clone()), false);

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

fn windows(windows: &[(u64, u64)]) -> crate::library::LibraryPageWindows {
    windows
        .iter()
        .map(|&(offset, limit)| crate::library::LibraryPageWindow { offset, limit })
        .collect()
}

/// The subscription's next value that `accept` takes, skipping the ones
/// before it — the value for the empty initial window set, or one resolved
/// before the queue reached the state a test drives it to.
async fn upcoming_until(
    subscription: &crate::library::QueueUpcomingSubscription,
    accept: impl Fn(&crate::library::QueueUpcomingSnapshot) -> bool,
) -> crate::library::QueueUpcomingSnapshot {
    loop {
        let value = tokio::time::timeout(std::time::Duration::from_secs(5), subscription.next())
            .await
            .expect("upcoming subscription delivers")
            .expect("upcoming windows resolve");
        if accept(&value) {
            return value;
        }
    }
}

fn window_track_ids(snapshot: &crate::library::QueueUpcomingSnapshot) -> Vec<Vec<&str>> {
    snapshot
        .windows
        .iter()
        .map(|window| {
            window
                .items
                .iter()
                .map(|item| item.track_id.as_str())
                .collect()
        })
        .collect()
}

/// One subscription reads every requested window of a real, actor-resolved
/// context tail in one value, each in order, stamped with the revision of the
/// queue value the windows were sliced from.
#[tokio::test]
async fn queue_upcoming_reads_every_window_of_a_live_context_tail_in_one_value() {
    let (services, track_ids, _temp_dir) = playing_app_services(12).await;
    let queue = queue_value_with_context(&services).await;
    let subscription = services.subscribe_queue_upcoming(&tokio::runtime::Handle::current());

    subscription.set(windows(&[(2, 3), (7, 2)])).unwrap();
    let value = upcoming_until(&subscription, |value| value.windows.len() == 2).await;

    assert_eq!(value.revision, queue.revision);
    // track_ids[0] is playing, so the tail is track_ids[1..]; offset 2 into
    // it is track_ids[3].
    assert_eq!(
        window_track_ids(&value),
        vec![
            track_ids[3..6]
                .iter()
                .map(String::as_str)
                .collect::<Vec<_>>(),
            track_ids[8..10]
                .iter()
                .map(String::as_str)
                .collect::<Vec<_>>(),
        ]
    );
}

/// A window reaching past the tail's end holds what remains, and one that
/// starts past it holds nothing.
#[tokio::test]
async fn queue_upcoming_clamps_windows_to_the_live_tails_end() {
    let (services, track_ids, _temp_dir) = playing_app_services(12).await;
    queue_value_with_context(&services).await;
    let subscription = services.subscribe_queue_upcoming(&tokio::runtime::Handle::current());

    // The tail has 11 entries (track_ids[1..12]).
    subscription.set(windows(&[(9, 100), (50, 10)])).unwrap();
    let value = upcoming_until(&subscription, |value| value.windows.len() == 2).await;

    assert_eq!(
        window_track_ids(&value),
        vec![vec![track_ids[10].as_str(), track_ids[11].as_str()], vec![]]
    );
}

/// Moving the windows moves the same subscription: the value for the new
/// windows arrives on it, and the old windows are gone from it.
#[tokio::test]
async fn queue_upcoming_moves_its_windows_in_place() {
    let (services, track_ids, _temp_dir) = playing_app_services(12).await;
    queue_value_with_context(&services).await;
    let subscription = services.subscribe_queue_upcoming(&tokio::runtime::Handle::current());

    subscription.set(windows(&[(0, 2)])).unwrap();
    upcoming_until(&subscription, |value| value.windows.len() == 1).await;

    subscription.set(windows(&[(4, 2)])).unwrap();
    let moved = upcoming_until(&subscription, |value| {
        value.windows.first().map(|window| window.window.offset) == Some(4)
    })
    .await;
    assert_eq!(
        moved.windows.len(),
        1,
        "the earlier window is no longer read"
    );
    assert_eq!(
        window_track_ids(&moved),
        vec![vec![track_ids[5].as_str(), track_ids[6].as_str()]]
    );
}

/// A queue revision reaches the windows through the subscription that is
/// already open: one that leaves the windows' entries alone restamps the
/// same items, and one that moves the tail redelivers the windows' new
/// entries.
#[tokio::test]
async fn queue_upcoming_follows_queue_revisions_without_resubscribing() {
    let (services, track_ids, _temp_dir) = playing_app_services(12).await;
    let queue = queue_value_with_context(&services).await;
    let subscription = services.subscribe_queue_upcoming(&tokio::runtime::Handle::current());
    subscription.set(windows(&[(2, 3)])).unwrap();
    let first = upcoming_until(&subscription, |value| value.windows.len() == 1).await;

    // The manual lane is not part of the context tail the windows slice.
    services.playback_add_to_queue(vec![track_ids[0].clone()]);
    let restamped = upcoming_until(&subscription, |value| value.revision > first.revision).await;
    assert_eq!(
        restamped.windows, first.windows,
        "the windows' entries are unchanged"
    );

    // Dropping the context empties the tail the windows slice.
    services.playback_clear_playing_from();
    let cleared = upcoming_until(&subscription, |value| {
        value.windows.iter().all(|window| window.items.is_empty())
    })
    .await;
    assert!(cleared.revision > queue.revision);
}

/// A library release's re-identify session keeps its choices in core: each
/// change is made to the ones before it, a run started afterwards reads them
/// without being handed them, and ending the session forgets them.
#[cfg(not(any(target_os = "ios", target_os = "android")))]
#[tokio::test(flavor = "multi_thread")]
async fn a_release_session_keeps_its_choices_in_core() {
    use crate::import::LookupChoiceEdit;
    let (services, _track_ids, _temp_dir) = playing_app_services(1).await;
    let key = "reidentify:release-1".to_string();
    let release = "release-1".to_string();

    services.edit_release_lookup_choices(
        key.clone(),
        release.clone(),
        LookupChoiceEdit::ToggleDiscId,
    );
    services.edit_release_lookup_choices(
        key.clone(),
        release.clone(),
        LookupChoiceEdit::ToggleCatalog {
            number: "WPCR-80001".to_string(),
        },
    );
    services.identify_release_for_lookup(key.clone(), release);

    let held = services.release_lookup_choices().get(&key).cloned();
    let held = held.expect("the session's choices are held");
    assert!(held.disc_id_excluded);
    assert_eq!(held.chosen_catalogs, vec!["WPCR-80001".to_string()]);

    services
        .end_release_identification(key.clone())
        .await
        .unwrap();
    assert!(services.release_lookup_choices().get(&key).is_none());
}

/// The next export-queue value whose only row satisfies `matches`, reading
/// past the values before it.
#[cfg(not(any(target_os = "ios", target_os = "android")))]
async fn output_value_until(
    values: &mut tokio::sync::mpsc::UnboundedReceiver<
        Result<crate::library::OutputSnapshot, crate::library::LibraryError>,
    >,
    matches: impl Fn(&crate::library::OutputRow) -> bool,
) -> crate::library::OutputSnapshot {
    loop {
        let value = tokio::time::timeout(std::time::Duration::from_secs(5), values.recv())
            .await
            .expect("export queue subscription delivers")
            .expect("export queue subscription stays open")
            .expect("export queue resolves");
        if let [row] = value.rows.as_slice() {
            if matches(row) {
                return value;
            }
        }
    }
}

/// A queued export names its release by id only, so an album title edited
/// while it waits shows on its row: the same subscription delivers the row
/// with the new title, and a fresh read agrees.
#[cfg(not(any(target_os = "ios", target_os = "android")))]
#[tokio::test(flavor = "multi_thread")]
async fn a_queued_release_shows_its_title_as_edited_while_it_waits() {
    let (services, _track_ids, temp_dir) = playing_app_services(1).await;
    let release_id = "c61a9e19-f3ba-4728-842c-c59dbc82e238";
    let manager = &services.inner.manager;
    manager.set_outputs_paused(true);
    manager
        .enqueue_export(release_id, temp_dir.path().join("export-out"))
        .await
        .unwrap();

    let mut values = services.subscribe_output_values(&tokio::runtime::Handle::current());
    let queued = output_value_until(&mut values, |row| row.release.is_some()).await;
    let release = queued.rows[0].release.as_ref().unwrap();
    assert_eq!(release.title, "Album Title");
    assert_eq!(release.file_count, 0);
    assert_eq!(queued.rows[0].op.release_id, release_id);

    manager
        .apply_release_metadata_user_edit(
            release_id,
            &crate::import::ReleaseUserEdit {
                album_title: "Renamed Album".to_string(),
                album_artist_assignments: vec![crate::import::ArtistAssignment::Picked {
                    artist: crate::import::ExistingArtist {
                        artist_id: bae_test_support::test_uuid(
                            "e36744a5-1a36-460f-891c-e7e558034edf",
                        ),
                        name: "Test Artist".to_string(),
                        sort_name: None,
                        musicbrainz_artist_id: None,
                        discogs_artist_id: None,
                    },
                }],
                album_year: None,
                pressing: crate::pressing::Pressing::blank(),
                tracks: vec![crate::import::TrackUserEdit {
                    title: "Track 0".to_string(),
                    side: None,
                    track_number: Some(0),
                    artist_assignments: crate::import::TrackArtistAssignments::AlbumArtists,
                    file: None,
                }],
            },
        )
        .await
        .unwrap();

    output_value_until(&mut values, |row| {
        row.release
            .as_ref()
            .is_some_and(|release| release.title == "Renamed Album")
    })
    .await;
    let read = services.output_snapshot().await.unwrap();
    assert_eq!(
        read.rows[0]
            .release
            .as_ref()
            .map(|release| release.title.as_str()),
        Some("Renamed Album")
    );
    assert!(read.paused);
}
