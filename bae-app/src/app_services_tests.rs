use super::*;
use bae_records::{DbAlbum, DbArtist, DbRelease, DbTrack};
use bae_store::Store;
use bae_util::test_channels::HANG_GUARD;
use tempfile::TempDir;

/// Build a real `AppServices` — library manager, actor-backed playback,
/// and (natively) the import/identify/extraction trio — through the step
/// `bootstrap` starts them with on desktop, seeded with one release of
/// `track_count` tracks. Starts playing the release from its first track,
/// so the queue's context lane is the one a real play fills, for the
/// upcoming-window subscription to read. The seeded tracks have no
/// backing audio file, so preparing the first track for playback fails
/// fast (a DB lookup, no I/O); that failure only stops playback, it
/// doesn't touch the queue the `PlayRelease` command already set.
async fn playing_app_services(track_count: usize) -> (AppServices, Vec<String>, TempDir) {
    let temp_dir = TempDir::new().unwrap();
    let db = Store::new_test(bae_library::SCHEMA, temp_dir.path(), Arc::new(coven::SystemClock))
        .await
        .unwrap();
    let records = bae_records::test_fixtures::test_records(&db);

    let artist = DbArtist {
        id: bae_util::test_ids::test_uuid("e36744a5-1a36-460f-891c-e7e558034edf"),
        name: "Test Artist".to_string(),
        sort_name: None,
        discogs_artist_id: None,
        musicbrainz_artist_id: None,
        created_at: chrono::Utc::now(),
    };
    records.insert_artist(&artist).await.unwrap();
    let album = DbAlbum::new_test("Album Title", &coven::UuidIds);
    let release = DbRelease::new_test(&album.id, "c61a9e19-f3ba-4728-842c-c59dbc82e238");
    records.insert_album(&album).await.unwrap();
    records
        .insert_album_artist(&bae_records::DbAlbumArtist::new_test(&album.id, &artist.id))
        .await
        .unwrap();
    records.insert_release(&release).await.unwrap();
    records
        .insert_release_medium(&bae_records::DbReleaseMedium::new_test(&release.id))
        .await
        .unwrap();
    let mut track_ids = Vec::with_capacity(track_count);
    for i in 0..track_count {
        let track_id = bae_util::test_ids::test_uuid(&format!("track-{i}"));
        let track = DbTrack::new_test(
            &release.id,
            &track_id,
            &format!("Track {i}"),
            u32::try_from(i + 1).expect("a test track number fits u32"),
        );
        records.insert_track(&track).await.unwrap();
        track_ids.push(track_id);
    }

    // Playback on a device with no hardware behind it, not the real cpal
    // sink: this test drives the playback actor for its queue behavior and
    // never plays audio (track prep fails before any stream is built).
    // Building a cpal output would reach for the system audio device, and on
    // Windows a second such build on a fresh actor thread faults — cpal's
    // process-global WASAPI device enumerator is left dangling once the first
    // actor thread that made it exits (the enumerator dies with that thread's
    // COM apartment), and the test builds one player per case.
    let library = bae_library::test_libraries::TestLibrary::open(
        db,
        bae_app_dir::AppDir::under_home(temp_dir.path()).unwrap(),
        bae_catalogs::fake_catalogs::FakeCatalogs::offline().providers(),
        bae_util::http::Http::for_test(),
    )
    .await;
    let services =
        crate::test_services::start_services(&library).await.expect("the services start");

    let played = services
        .playback_play_release(release.id.clone(), Some(track_ids[0].clone()), false)
        .await
        .expect("the play is answered");
    assert_eq!(played, bae_playback::PlayOutcome::Playing);

    (services, track_ids, temp_dir)
}

async fn queue_value_with_context(services: &AppServices) -> bae_playback::ResolvedQueueSnapshot {
    let values = services.subscribe_queue_values();
    loop {
        let value = tokio::time::timeout(HANG_GUARD, values.next())
            .await
            .expect("queue subscription delivers")
            .expect("queue projection resolves");
        if value.context.is_some() {
            return value;
        }
    }
}

fn windows(windows: &[(u64, u64)]) -> bae_store::LibraryPageWindows {
    windows.iter().map(|&(offset, limit)| bae_store::LibraryPageWindow::at(offset, limit)).collect()
}

/// The subscription's next value that `accept` takes, skipping the ones
/// before it — the value for the empty initial window set, or one resolved
/// before the queue reached the state a test drives it to.
async fn upcoming_until(
    subscription: &bae_library::LiveQueueUpcoming,
    accept: impl Fn(&bae_library::QueueUpcomingSnapshot) -> bool,
) -> bae_library::QueueUpcomingSnapshot {
    loop {
        let value = tokio::time::timeout(HANG_GUARD, subscription.next())
            .await
            .expect("upcoming subscription delivers")
            .expect("upcoming windows resolve");
        if accept(&value) {
            return value;
        }
    }
}

fn window_track_ids(snapshot: &bae_library::QueueUpcomingSnapshot) -> Vec<Vec<&str>> {
    snapshot
        .windows
        .iter()
        .map(|window| window.rows.iter().map(|item| item.track_id.as_str()).collect())
        .collect()
}

/// One subscription reads every requested window of a real, actor-resolved
/// context tail in one value, each in order, with the tail's length.
#[tokio::test]
async fn queue_upcoming_reads_every_window_of_a_live_context_tail_in_one_value() {
    let (services, track_ids, _temp_dir) = playing_app_services(12).await;
    queue_value_with_context(&services).await;
    let subscription = services.subscribe_queue_upcoming();

    subscription.set(windows(&[(2, 3), (7, 2)])).unwrap();
    let value = upcoming_until(&subscription, |value| value.windows.len() == 2).await;

    assert_eq!(value.upcoming_total, 11);
    // track_ids[0] is playing, so the tail is track_ids[1..]; offset 2 into
    // it is track_ids[3].
    assert_eq!(
        window_track_ids(&value),
        vec![
            track_ids[3..6].iter().map(String::as_str).collect::<Vec<_>>(),
            track_ids[8..10].iter().map(String::as_str).collect::<Vec<_>>(),
        ]
    );
}

/// A window reaching past the tail's end holds what remains, and one that
/// starts past it holds nothing.
#[tokio::test]
async fn queue_upcoming_clamps_windows_to_the_live_tails_end() {
    let (services, track_ids, _temp_dir) = playing_app_services(12).await;
    queue_value_with_context(&services).await;
    let subscription = services.subscribe_queue_upcoming();

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
    let subscription = services.subscribe_queue_upcoming();

    subscription.set(windows(&[(0, 2)])).unwrap();
    let first = upcoming_until(&subscription, |value| value.windows.len() == 1).await;

    subscription.set(windows(&[(4, 2)])).unwrap();
    let moved = upcoming_until(&subscription, |value| {
        value.windows.first().map(|window| window.window.offset) == Some(4)
    })
    .await;
    assert_eq!(moved.windows.len(), 1, "the earlier window is no longer read");
    assert_eq!(moved.context_version, first.context_version, "only the windows moved");
    assert_eq!(moved.upcoming_total, 11);
    assert_eq!(window_track_ids(&moved), vec![vec![track_ids[5].as_str(), track_ids[6].as_str()]]);
}

/// A change to the context lane reaches the windows through the subscription
/// that is already open, marked as a change: skipping ahead moves the tail
/// under them, and dropping the context empties it.
#[tokio::test]
async fn queue_upcoming_follows_queue_changes_without_resubscribing() {
    let (services, track_ids, _temp_dir) = playing_app_services(12).await;
    queue_value_with_context(&services).await;
    let subscription = services.subscribe_queue_upcoming();
    subscription.set(windows(&[(2, 3)])).unwrap();
    let first = upcoming_until(&subscription, |value| value.windows.len() == 1).await;
    let skip_to = first.windows[0].rows[0].entry_id.parse().unwrap();

    services.playback_skip_to_entry(skip_to).await.unwrap();
    let moved =
        upcoming_until(&subscription, |value| value.upcoming_total < first.upcoming_total).await;
    assert!(moved.context_version > first.context_version, "the queue changed under the windows");
    assert_eq!(moved.upcoming_total, 8, "track_ids[3] plays; track_ids[4..] follow");
    assert_eq!(
        moved.windows[0].rows.iter().map(|item| item.track_id.as_str()).collect::<Vec<_>>(),
        track_ids[6..9].iter().map(String::as_str).collect::<Vec<_>>()
    );

    // Dropping the context empties the tail the windows read.
    services.playback_clear_playing_from().await.unwrap();
    let cleared = upcoming_until(&subscription, |value| value.upcoming_total == 0).await;
    assert!(cleared.windows.iter().all(|window| window.rows.is_empty()));
}

/// Wait until the storage inspector, set to one release, shows its export
/// entry as `matches` looks for.
#[cfg(desktop)]
async fn inspected_export_until(
    inspector: &bae_library::LiveDetail<bae_views::storage_inspector::StorageInspector>,
    matches: impl Fn(&bae_transfers::OutputPhaseRow) -> bool,
) -> bae_transfers::OutputPhaseRow {
    use bae_views::storage_inspector::StorageInspectorTransfer;
    loop {
        let snapshot = tokio::time::timeout(HANG_GUARD, inspector.next())
            .await
            .expect("the inspector delivers")
            .expect("the inspector reads");
        let Some(value) = snapshot.value else {
            continue;
        };
        for transfer in value.transfers {
            if let StorageInspectorTransfer::Output { operation, .. } = transfer {
                if matches(&operation) {
                    return operation;
                }
            }
        }
    }
}

/// A queued export names its release by id only, so an album title edited
/// while it waits shows on its row: the inspector's read delivers the row
/// with the new title, and a fresh read of the queue agrees.
#[cfg(desktop)]
#[tokio::test(flavor = "multi_thread")]
async fn a_queued_release_shows_its_title_as_edited_while_it_waits() {
    let (services, _track_ids, temp_dir) = playing_app_services(1).await;
    let release_id = "c61a9e19-f3ba-4728-842c-c59dbc82e238";
    let manager = &services.inner.manager;
    manager.set_outputs_paused(true).await.unwrap();
    manager.enqueue_export(release_id, temp_dir.path().join("export-out")).await.unwrap();

    let inspector = services.subscribe_storage_inspector();
    inspector.set(Some(release_id.to_string())).unwrap();
    let queued = inspected_export_until(&inspector, |row| row.release.is_some()).await;
    let release = queued.release.as_ref().unwrap();
    assert_eq!(release.title, "Album Title");
    assert_eq!(release.file_count, 0);
    assert_eq!(queued.op.release_id, release_id);

    manager
        .apply_release_metadata_user_edit(
            release_id,
            &bae_import::ReleaseUserEdit {
                album_title: "Renamed Album".to_string(),
                album_artist_assignments: vec![bae_import::Credited::plain(
                    bae_import::ArtistAssignment::Picked {
                        credit_name: None,
                        artist: bae_records::ExistingArtist {
                            artist_id: bae_util::test_ids::test_uuid(
                                "e36744a5-1a36-460f-891c-e7e558034edf",
                            ),
                            name: "Test Artist".to_string(),
                            sort_name: None,
                            musicbrainz_artist_id: None,
                            discogs_artist_id: None,
                        },
                    },
                )],
                album_year: None,
                pressing: bae_types::pressing::Pressing::blank(),
                tracks: vec![bae_import::TrackUserEdit {
                    title: "Track 0".to_string(),
                    artist_assignments: bae_import::TrackArtistAssignments::AlbumArtists,
                    file: None,
                }],
            },
        )
        .await
        .unwrap();

    inspected_export_until(&inspector, |row| {
        row.release.as_ref().is_some_and(|release| release.title == "Renamed Album")
    })
    .await;
    let read = services.output_snapshot().await.unwrap();
    assert_eq!(
        read.rows[0].release.as_ref().map(|release| release.title.as_str()),
        Some("Renamed Album")
    );
    assert!(read.paused);
}

/// The inspector reads the release it is set to in core: its entry in each
/// queue, and nothing of a release the library does not hold.
#[cfg(desktop)]
#[tokio::test(flavor = "multi_thread")]
async fn the_storage_inspector_reads_the_release_it_is_set_to() {
    use bae_views::storage_inspector::StorageInspectorTransfer;
    let (services, _track_ids, temp_dir) = playing_app_services(1).await;
    let release_id = "c61a9e19-f3ba-4728-842c-c59dbc82e238";
    let manager = &services.inner.manager;
    manager.set_outputs_paused(true).await.unwrap();
    manager.enqueue_export(release_id, temp_dir.path().join("export-out")).await.unwrap();

    let inspector = services.subscribe_storage_inspector();
    inspector.set(Some(release_id.to_string())).unwrap();
    let next = || async {
        tokio::time::timeout(HANG_GUARD, inspector.next())
            .await
            .expect("the inspector delivers")
            .expect("the inspector reads")
    };
    let shown = loop {
        let snapshot = next().await;
        assert_eq!(snapshot.id.as_deref(), Some(release_id));
        let value = snapshot.value.expect("the library holds the release");
        if !value.transfers.is_empty() {
            break value;
        }
    };
    let [StorageInspectorTransfer::Output { operation }] = shown.transfers.as_slice() else {
        panic!("only the export queue holds the release: {:?}", shown.transfers);
    };
    assert_eq!(operation.op.release_id, release_id);
    assert!(shown.files.is_empty(), "the seeded release has no files");

    let missing = "f0a5c8e2-7d61-4b9e-a3c4-1e2d3f4a5b6c";
    inspector.set(Some(missing.to_string())).unwrap();
    let gone = next().await;
    assert_eq!(gone.id.as_deref(), Some(missing));
    assert!(gone.value.is_none(), "no such release");
}

/// Closing the library ends the task behind a subscription its holder still
/// keeps before the store closes: the subscription ends, and never hears from
/// the closed store.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn closing_the_library_ends_a_held_subscriptions_task_before_its_store_closes() {
    let (services, _track_ids, _temp_dir) = playing_app_services(3).await;
    let values = services.subscribe_download_counts();
    tokio::time::timeout(HANG_GUARD, values.next())
        .await
        .expect("the download counts subscription delivers")
        .expect("the download counts resolve");

    services.close().await.unwrap();

    let ended =
        tokio::time::timeout(HANG_GUARD, values.next()).await.expect("the subscription ends");
    assert!(
        matches!(ended, Err(bae_store::live_query::LiveQueryError::Cancelled)),
        "the subscription heard from the closed store: {ended:?}"
    );
}

/// A live read its holder still keeps ends with the library the same way,
/// before the store closes.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn closing_the_library_ends_a_held_live_read_before_its_store_closes() {
    let (services, _track_ids, _temp_dir) = playing_app_services(3).await;
    let release = services.subscribe_release_detail();
    release.set(Some("c61a9e19-f3ba-4728-842c-c59dbc82e238".to_string())).unwrap();
    tokio::time::timeout(HANG_GUARD, release.next())
        .await
        .expect("the release detail delivers")
        .expect("the release detail resolves");

    services.close().await.unwrap();

    // Values sent before the close may still be waiting to be read.
    loop {
        match tokio::time::timeout(HANG_GUARD, release.next()).await.expect("the live read ends") {
            Ok(_) => {}
            Err(bae_store::live_query::LiveQueryError::Query(error)) => {
                panic!("the live read heard from the closed store: {error}")
            }
            Err(bae_store::live_query::LiveQueryError::Cancelled) => break,
        }
    }
}

/// The Storage sheet's read of a release's files answers the request it is
/// set to, naming it: the release's files in the order asked for, and none
/// for a release the library does not hold.
#[cfg(desktop)]
#[tokio::test(flavor = "multi_thread")]
async fn the_release_files_read_answers_the_request_it_is_set_to() {
    use bae_views::{ReleaseFileColumn, ReleaseFileOrder, ReleaseFilesRequest};
    let (services, _track_ids, _temp_dir) = playing_app_services(1).await;
    let files = services.subscribe_release_files();
    let request = |release_id: &str| ReleaseFilesRequest {
        release_id: release_id.to_string(),
        order: ReleaseFileOrder { column: ReleaseFileColumn::Size, descending: true },
    };
    let answer = |asked: ReleaseFilesRequest| {
        let files = &files;
        async move {
            files.set(Some(asked.clone())).unwrap();
            tokio::time::timeout(HANG_GUARD, async {
                loop {
                    let snapshot = files.next().await.expect("the read runs");
                    if snapshot.request.as_ref() == Some(&asked) {
                        break snapshot.files;
                    }
                }
            })
            .await
            .expect("the read answers the request")
        }
    };
    let held = answer(request("c61a9e19-f3ba-4728-842c-c59dbc82e238")).await;
    assert_eq!(held.map(|files| files.len()), Some(0), "the release holds no files");
    assert!(answer(request("no-such-release")).await.is_none());
}
